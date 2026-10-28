//! A small tree for YAML and JSON documents that remembers the line of every key, so OpenAPI documents and buf
//! configs are read in one way whichever syntax they are written in.

use gsttt_core::fact::Line;
use gsttt_core::text::LineIndex;
use jsonc_parser::common::Ranged;
use jsonc_parser::{CollectOptions, ParseOptions, ast, parse_to_ast};
use saphyr::{LoadableYamlNode, MarkedYaml};

use crate::common::{SyntaxError, byte_offset};

/// Documents nested deeper than this read as [`Value::Other`] below that level, so hostile input cannot exhaust the
/// stack of the readers that walk the tree.
pub(crate) const MAX_DEPTH: usize = 64;

/// A YAML or JSON value; scalars other than strings are not told apart because no reader needs them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Value {
    Map(Vec<Entry>),
    Seq(Vec<Value>),
    Str(String),
    Other,
}

/// One key of a map with the line it is written on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) key: String,
    pub(crate) line: Line,
    pub(crate) value: Value,
}

impl Value {
    /// The entries of a map; empty for any other value.
    pub(crate) fn entries(&self) -> &[Entry] {
        match self {
            Self::Map(entries) => entries,
            _ => &[],
        }
    }

    /// The items of a sequence; empty for any other value.
    pub(crate) fn items(&self) -> &[Value] {
        match self {
            Self::Seq(items) => items,
            _ => &[],
        }
    }

    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        self.entries()
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| &entry.value)
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(text) => Some(text),
            _ => None,
        }
    }
}

/// Reads the first document of a YAML text.
pub(crate) fn parse_yaml(text: &str) -> Result<Value, SyntaxError> {
    let documents = MarkedYaml::load_from_str(text).map_err(|error| SyntaxError {
        line: u32::try_from(error.marker().line())
            .unwrap_or(u32::MAX)
            .max(1),
        message: error.info().to_owned(),
    })?;
    Ok(documents
        .first()
        .map_or(Value::Other, |document| from_yaml(document, 0)))
}

/// Reads a JSON text, comments and trailing commas included.
pub(crate) fn parse_json(text: &str) -> Result<Value, SyntaxError> {
    let parsed = parse_to_ast(text, &CollectOptions::default(), &ParseOptions::default()).map_err(
        |error| SyntaxError {
            line: u32::try_from(error.line_display()).unwrap_or(u32::MAX),
            message: error.to_string(),
        },
    )?;
    let lines = LineIndex::new(text);
    Ok(parsed
        .value
        .as_ref()
        .map_or(Value::Other, |value| from_json(value, &lines, 0)))
}

fn from_yaml(node: &MarkedYaml<'_>, depth: usize) -> Value {
    if depth > MAX_DEPTH {
        return Value::Other;
    }
    if let Some(mapping) = node.data.as_mapping() {
        return Value::Map(
            mapping
                .iter()
                .filter_map(|(key, value)| {
                    Some(Entry {
                        key: key.data.as_str()?.to_owned(),
                        line: u32::try_from(key.span.start.line())
                            .unwrap_or(u32::MAX)
                            .max(1),
                        value: from_yaml(value, depth + 1),
                    })
                })
                .collect(),
        );
    }
    if let Some(sequence) = node.data.as_sequence() {
        return Value::Seq(
            sequence
                .iter()
                .map(|item| from_yaml(item, depth + 1))
                .collect(),
        );
    }
    node.data
        .as_str()
        .map_or(Value::Other, |text| Value::Str(text.to_owned()))
}

fn from_json(value: &ast::Value<'_>, lines: &LineIndex, depth: usize) -> Value {
    if depth > MAX_DEPTH {
        return Value::Other;
    }
    match value {
        ast::Value::Object(object) => Value::Map(
            object
                .properties
                .iter()
                .map(|property| Entry {
                    key: property.name.as_str().to_owned(),
                    line: lines.line(byte_offset(property.name.range().start)),
                    value: from_json(&property.value, lines, depth + 1),
                })
                .collect(),
        ),
        ast::Value::Array(array) => Value::Seq(
            array
                .elements
                .iter()
                .map(|item| from_json(item, lines, depth + 1))
                .collect(),
        ),
        ast::Value::StringLit(text) => Value::Str(text.value.to_string()),
        _ => Value::Other,
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn lines_of(value: &Value, out: &mut Vec<Line>) {
        match value {
            Value::Map(entries) => {
                for entry in entries {
                    out.push(entry.line);
                    lines_of(&entry.value, out);
                }
            }
            Value::Seq(items) => items.iter().for_each(|item| lines_of(item, out)),
            Value::Str(_) | Value::Other => {}
        }
    }

    #[test]
    fn yaml_keys_remember_their_lines() {
        let tree = parse_yaml("paths:\n  /a:\n    get: {}\n").unwrap();
        let paths = tree.get("paths").unwrap();
        assert_eq!(paths.entries()[0].key, "/a");
        assert_eq!(paths.entries()[0].line, 2);
        assert_eq!(paths.get("/a").unwrap().entries()[0].line, 3);
    }

    #[test]
    fn json_keys_remember_their_lines_and_comments_are_allowed() {
        let tree = parse_json("{\n  // note\n  \"paths\": {\n    \"/a\": {},\n  }\n}").unwrap();
        let paths = tree.get("paths").unwrap();
        assert_eq!(paths.entries()[0].line, 4);
    }

    #[test]
    fn deep_documents_are_cut_instead_of_walked_to_the_bottom() {
        let depth = MAX_DEPTH * 4;
        let text = format!("{}1{}", "[".repeat(depth), "]".repeat(depth));
        let mut value = parse_json(&text).unwrap();
        let mut levels = 0;
        while let Value::Seq(items) = value {
            levels += 1;
            value = items.into_iter().next().unwrap_or(Value::Other);
        }
        assert!(levels <= MAX_DEPTH + 1, "{levels} levels were kept");
    }

    proptest! {
        #[test]
        fn lines_stay_inside_the_document(
            text in "[a-z0-9:\\-\\[\\]{}\", \n#]{0,120}",
            json in any::<bool>(),
        ) {
            let count = u32::try_from(text.lines().count()).unwrap_or(u32::MAX).saturating_add(1);
            match if json { parse_json(&text) } else { parse_yaml(&text) } {
                Ok(tree) => {
                    let mut lines = Vec::new();
                    lines_of(&tree, &mut lines);
                    prop_assert!(
                        lines.iter().all(|line| (1..=count).contains(line)),
                        "{lines:?} of {count}"
                    );
                }
                Err(error) => prop_assert!(error.line >= 1),
            }
        }

        #[test]
        fn arbitrary_text_never_panics(text in "\\PC*") {
            drop(parse_yaml(&text));
            drop(parse_json(&text));
        }
    }
}
