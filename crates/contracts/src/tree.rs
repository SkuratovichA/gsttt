//! A small tree for YAML and JSON documents that remembers the line of every key, so OpenAPI documents and buf
//! configs are read in one way whichever syntax they are written in.

use gsttt_core::fact::Line;
use gsttt_core::text::LineIndex;
use jsonc_parser::common::Ranged;
use jsonc_parser::{CollectOptions, ParseOptions, ast, parse_to_ast};
use saphyr::{LoadableYamlNode, MarkedYaml};

use crate::common::byte_offset;

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

/// Why a document does not parse, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyntaxError {
    pub(crate) line: Line,
    pub(crate) message: String,
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
