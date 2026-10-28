//! The tree that connectors read a JSON or a YAML document into: the keys of a map keep the order they are written in, because the first matching key of a conditional `exports` map wins, and each key remembers its line, because a fact names one.
//!
//! JSON is read here, with the comments and trailing commas that `package.json` and tsconfig files allow. The YAML reader of the contract formats builds the same tree, so a reader walks a document whichever syntax it is written in.

use jsonc_parser::common::Ranged;
use jsonc_parser::{CollectOptions, ParseOptions, ast, parse_to_ast};

use crate::fact::Line;
use crate::text::LineIndex;

/// Documents nested deeper than this read as [`Value::Other`] below that level, so hostile input cannot exhaust the stack of the readers that walk the tree.
pub const MAX_DEPTH: usize = 64;

/// A JSON or YAML value. Of the scalars only strings and `null` are told apart, because no reader needs another one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// An object or a mapping, with its entries in the order they are written.
    Map(Vec<Entry>),
    /// An array or a sequence.
    Seq(Vec<Value>),
    /// A string.
    Str(String),
    /// The keyword `null`, which an `exports` map uses to close a path.
    Null,
    /// A number, a boolean, an empty document, or anything nested deeper than [`MAX_DEPTH`].
    Other,
}

/// One key of a map with the line it is written on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The text of the key, with its quotes and escapes read.
    pub key: String,
    /// The line the key starts on.
    pub line: Line,
    /// What the key holds.
    pub value: Value,
}

/// Why a document is not valid in its syntax, and where.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct SyntaxError {
    /// The line the parser reports, from 1.
    pub line: Line,
    /// What the parser says is wrong.
    pub message: String,
}

impl Value {
    /// The text of a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(text) => Some(text),
            _ => None,
        }
    }

    /// The entries of a map in the order they are written; empty for any other value.
    pub fn entries(&self) -> &[Entry] {
        match self {
            Self::Map(entries) => entries,
            _ => &[],
        }
    }

    /// The items of a sequence; empty for any other value.
    pub fn items(&self) -> &[Value] {
        match self {
            Self::Seq(items) => items,
            _ => &[],
        }
    }

    /// The first entry of a map named `key`, with its line.
    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries().iter().find(|entry| entry.key == key)
    }

    /// The value of the first entry of a map named `key`.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entry(key).map(|entry| &entry.value)
    }

    /// The strings of a string or of a sequence of strings.
    pub fn strings(&self) -> Vec<&str> {
        match self {
            Self::Str(text) => vec![text.as_str()],
            Self::Seq(items) => items.iter().filter_map(Self::as_str).collect(),
            _ => Vec::new(),
        }
    }
}

/// Reads `text` as JSON with comments and trailing commas.
pub fn parse_json(text: &str) -> Result<Value, SyntaxError> {
    let parsed = parse_to_ast(text, &CollectOptions::default(), &ParseOptions::default()).map_err(
        |error| SyntaxError {
            line: Line::try_from(error.line_display()).unwrap_or(Line::MAX),
            message: error.to_string(),
        },
    )?;
    let lines = LineIndex::new(text);
    Ok(parsed
        .value
        .as_ref()
        .map_or(Value::Other, |value| from_json(value, &lines, 0)))
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
                    line: lines.line_at(property.name.range().start),
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
        ast::Value::NullKeyword(_) => Value::Null,
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
            Value::Str(_) | Value::Null | Value::Other => {}
        }
    }

    fn keys(value: &Value) -> Vec<&str> {
        value
            .entries()
            .iter()
            .map(|entry| entry.key.as_str())
            .collect()
    }

    #[test]
    fn comments_and_trailing_commas_are_fine() {
        let tree = parse_json(
            "{\n  // base\n  \"a\": [\"x\", \"y\",],\n  /* c */ \"b\": {\n    \"c\": 1,\n  },\n}",
        )
        .unwrap();
        assert_eq!(tree.get("a").unwrap().strings(), ["x", "y"]);
        assert_eq!(tree.entry("a").unwrap().line, 3);
        assert_eq!(tree.entry("b").unwrap().line, 4);
        assert_eq!(tree.get("b").unwrap().entries()[0].line, 5);
    }

    #[test]
    fn keys_keep_their_order() {
        let tree = parse_json(r#"{"z": "1", "a": "2", "m": "3"}"#).unwrap();
        assert_eq!(keys(&tree), ["z", "a", "m"]);
    }

    #[test]
    fn a_key_is_the_text_it_spells_whether_it_is_quoted_escaped_or_bare() {
        let tree = parse_json("{\"a\\u0062\": 1, c: 2, \"d\\\\e\": 3}").unwrap();
        assert_eq!(keys(&tree), ["ab", "c", "d\\e"]);
    }

    #[test]
    fn only_strings_and_null_are_told_apart_from_the_other_scalars() {
        let tree =
            parse_json(r#"{"s": "text", "n": null, "i": 1, "b": true, "m": {}, "a": []}"#).unwrap();
        let values: Vec<&Value> = tree.entries().iter().map(|entry| &entry.value).collect();
        assert_eq!(
            values,
            [
                &Value::Str("text".to_owned()),
                &Value::Null,
                &Value::Other,
                &Value::Other,
                &Value::Map(Vec::new()),
                &Value::Seq(Vec::new()),
            ]
        );
        assert_eq!(parse_json("").unwrap(), Value::Other);
    }

    #[test]
    fn accessors_of_the_wrong_shape_find_nothing() {
        let tree = parse_json(r#"{"a": "x", "b": ["y", 1, "z"], "a2": 1}"#).unwrap();
        assert_eq!(tree.get("a").unwrap().strings(), ["x"]);
        assert_eq!(tree.get("b").unwrap().strings(), ["y", "z"]);
        assert_eq!(tree.get("b").unwrap().items().len(), 3);
        assert_eq!(tree.get("a").unwrap().entries(), []);
        assert!(tree.get("a").unwrap().get("b").is_none());
        assert_eq!(tree.get("a2").unwrap().strings(), Vec::<&str>::new());
        assert!(tree.get("missing").is_none());
        assert_eq!(tree.get("a").and_then(Value::as_str), Some("x"));
        assert_eq!(tree.get("b").and_then(Value::as_str), None);
    }

    #[test]
    fn an_invalid_file_names_its_line() {
        let error = parse_json("{\n  \"a\": 1,\n  \"b\": ]\n}").unwrap_err();
        assert_eq!(error.line, 3);
        assert_eq!(error.to_string(), error.message);
    }

    proptest! {
        #[test]
        fn lines_stay_inside_the_document(text in "[a-z0-9:\\-\\[\\]{}\", \n#&*<]{0,120}") {
            let count = Line::try_from(text.lines().count())
                .unwrap_or(Line::MAX)
                .saturating_add(1);
            match parse_json(&text) {
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
        fn any_text_is_read_or_refused_without_panicking(text in "\\PC{0,200}") {
            drop(parse_json(&text));
        }

        #[test]
        fn nesting_is_cut_at_the_depth_cap(depth in 0usize..400) {
            let text = format!("{}1{}", "[".repeat(depth), "]".repeat(depth));
            let mut value = parse_json(&text).unwrap();
            let mut levels = 0;
            while let Value::Seq(items) = value {
                levels += 1;
                value = items.into_iter().next().unwrap_or(Value::Other);
            }
            prop_assert_eq!(levels, depth.min(MAX_DEPTH + 1));
        }
    }
}
