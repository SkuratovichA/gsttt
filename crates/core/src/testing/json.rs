//! Walks over the JSON documents gsttt writes, for the tests of every crate that writes one.

use serde_json::Value;

/// Every text under `value` that is the value of a `description` member: the descriptions of a JSON Schema, or of the tools an MCP server lists.
pub fn descriptions(value: &Value) -> Vec<&str> {
    match value {
        Value::Object(members) => members
            .iter()
            .flat_map(|(name, member)| match member {
                Value::String(text) if name == "description" => vec![text.as_str()],
                _ => descriptions(member),
            })
            .collect(),
        Value::Array(items) => items.iter().flat_map(descriptions).collect(),
        _ => Vec::new(),
    }
}
