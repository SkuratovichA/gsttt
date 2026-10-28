//! Package manifests and tsconfig files are JSON with comments and trailing commas. They are read into a small tree that keeps
//! the order of keys, because the conditions of an `exports` map are order-sensitive: the first matching key wins.
//!
//! `crates/contracts/src/tree.rs` (`from_json`) holds a twin of this reader: both turn a `jsonc_parser` tree into an ordered
//! tree with a depth cap and a line index, and both cap the depth at 64. The two crates cannot depend on each other, so the
//! copy stays until integration moves one ordered-JSON reader into `gsttt-core`.

use gsttt_core::fact::Line;
use gsttt_core::text::LineIndex;
use jsonc_parser::ast::{ObjectPropName, Value};
use jsonc_parser::common::Ranged;
use jsonc_parser::{CollectOptions, ParseOptions, parse_to_ast};

/// Nesting beyond this is read as an opaque value, so a pathological file cannot exhaust the stack.
const MAX_DEPTH: usize = 64;

/// A JSON value, keeping what resolution needs: strings, arrays and ordered objects.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Json {
    Str(String),
    Array(Vec<Json>),
    Object(Vec<Entry>),
    /// Numbers, booleans, `null` and anything too deeply nested.
    /// The keyword `null`, which an `exports` map uses to close a path.
    Null,
    Other,
}

/// One property of an object, with the line of its key.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Entry {
    pub key: String,
    pub value: Json,
    pub line: Line,
}

/// Why a file is not JSON with comments.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub(crate) struct JsonError {
    pub line: Line,
    pub message: String,
}

/// Reads `text` as JSON with comments and trailing commas.
pub(crate) fn parse(text: &str) -> Result<Json, JsonError> {
    let parsed = parse_to_ast(text, &CollectOptions::default(), &ParseOptions::default()).map_err(
        |error| JsonError {
            line: Line::try_from(error.line_display()).unwrap_or(Line::MAX),
            message: error.to_string(),
        },
    )?;
    let lines = LineIndex::new(text);
    Ok(parsed
        .value
        .map_or(Json::Other, |value| convert(&value, &lines, 0)))
}

fn convert(value: &Value<'_>, lines: &LineIndex, depth: usize) -> Json {
    if depth > MAX_DEPTH {
        return Json::Other;
    }
    match value {
        Value::StringLit(text) => Json::Str(text.value.to_string()),
        Value::Array(array) => Json::Array(
            array
                .elements
                .iter()
                .map(|element| convert(element, lines, depth + 1))
                .collect(),
        ),
        Value::Object(object) => Json::Object(
            object
                .properties
                .iter()
                .map(|property| Entry {
                    key: key_text(&property.name),
                    value: convert(&property.value, lines, depth + 1),
                    line: lines.line(u32::try_from(property.name.range().start).unwrap_or(0)),
                })
                .collect(),
        ),
        Value::NullKeyword(_) => Json::Null,
        _ => Json::Other,
    }
}

fn key_text(name: &ObjectPropName<'_>) -> String {
    match name {
        ObjectPropName::String(text) => text.value.to_string(),
        ObjectPropName::Word(word) => word.value.to_string(),
    }
}

impl Json {
    /// The text of a string value.
    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(text) => Some(text),
            _ => None,
        }
    }

    /// The property `key` of an object, with its line.
    pub(crate) fn entry(&self, key: &str) -> Option<&Entry> {
        match self {
            Self::Object(entries) => entries.iter().find(|entry| entry.key == key),
            _ => None,
        }
    }

    /// The value of the property `key` of an object.
    pub(crate) fn get(&self, key: &str) -> Option<&Self> {
        self.entry(key).map(|entry| &entry.value)
    }

    /// The properties of an object, in file order.
    pub(crate) fn entries(&self) -> &[Entry] {
        match self {
            Self::Object(entries) => entries,
            _ => &[],
        }
    }

    /// The strings of a string value or of an array of strings.
    pub(crate) fn strings(&self) -> Vec<&str> {
        match self {
            Self::Str(text) => vec![text.as_str()],
            Self::Array(items) => items.iter().filter_map(Self::as_str).collect(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn comments_and_trailing_commas_are_fine() {
        let json =
            parse("{\n  // base\n  \"a\": [\"x\", \"y\",],\n  /* c */ \"b\": 1,\n}").unwrap();
        assert_eq!(json.get("a").unwrap().strings(), ["x", "y"]);
        assert_eq!(json.entry("b").unwrap().line, 4);
    }

    #[test]
    fn keys_keep_their_order() {
        let json = parse(r#"{"z": "1", "a": "2", "m": "3"}"#).unwrap();
        let keys: Vec<_> = json
            .entries()
            .iter()
            .map(|entry| entry.key.as_str())
            .collect();
        assert_eq!(keys, ["z", "a", "m"]);
    }

    #[test]
    fn an_invalid_file_names_its_line() {
        let error = parse("{\n  \"a\": 1,\n  \"b\": ]\n}").unwrap_err();
        assert_eq!(error.line, 3);
    }

    proptest! {
        #[test]
        fn any_text_is_read_or_refused_without_panicking(text in "\\PC{0,200}") {
            drop(parse(&text));
        }

        #[test]
        fn nesting_cannot_exhaust_the_stack(depth in 0usize..400) {
            let text = format!("{}1{}", "[".repeat(depth), "]".repeat(depth));
            drop(parse(&text));
        }
    }
}
