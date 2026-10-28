//! The JSON Schemas an editor checks the config and the packs with. They are generated from the types the reader deserializes into, so that what an editor accepts and what the reader accepts cannot drift apart.
//!
//! Both are draft-07, which TOML language servers read, and neither offers `null`, which TOML cannot write.

use gsttt_core::connector::DynConnector;
use gsttt_core::rules::Pack;
use schemars::generate::SchemaSettings;
use schemars::transform::{RecursiveTransform, Transform};
use schemars::{Schema, SchemaGenerator};
use serde_json::{Value, json};

use crate::evaluate::Keys;

/// The generator both schemas are made with.
fn generator() -> SchemaGenerator {
    SchemaSettings::draft07()
        .with_transform(RecursiveTransform(without_null))
        .into_generator()
}

/// Takes `null` out of a schema. schemars lets an optional field be `null` and defaults it to `null`, and an editor would offer a value no TOML file can hold; this version of it has no setting to leave that out.
fn without_null(schema: &mut Schema) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    if object.get("default").is_some_and(Value::is_null) {
        object.remove("default");
    }

    // `"type": ["string", "null"]` is `"type": "string"`.
    let single_type = match object.get_mut("type") {
        Some(Value::Array(types)) => {
            types.retain(|name| name != "null");
            match types.as_mut_slice() {
                [only] => Some(only.take()),
                _ => None,
            }
        }
        _ => None,
    };
    if let Some(single_type) = single_type {
        object.insert("type".to_owned(), single_type);
    }

    // `"anyOf": [X, { "type": "null" }]` is X, written as `allOf` because a description may sit beside it there and not beside a `$ref`.
    let one_alternative_left = match object.get_mut("anyOf") {
        Some(Value::Array(alternatives)) => {
            alternatives
                .retain(|alternative| !alternative.get("type").is_some_and(|name| name == "null"));
            alternatives.len() == 1
        }
        _ => false,
    };
    if one_alternative_left
        && !object.contains_key("allOf")
        && let Some(alternatives) = object.remove("anyOf")
    {
        object.insert("allOf".to_owned(), alternatives);
    }
}

/// The table of a module. A module without options is turned on with an empty table, which the reader gives as `null`, the value of `()`, so its options, `{"type": "null"}` once `null` is out, become a table that takes no key. Only a module as a whole gets this: a `()` nested in the options is given no such table.
fn module_table(schema: Schema) -> Value {
    let value = Value::from(schema);
    if value.get("type").is_some_and(|name| name == "null") {
        return json!({ "type": "object", "additionalProperties": false });
    }
    value
}

/// The JSON Schema of `.gsttt/config.toml` for the modules of `registry`: the keys of the config, the `[[external]]` entries, and a table for each module, which holds the options of the module.
///
/// Every table the reader checks for unknown keys has `additionalProperties` false, so that an editor flags a misspelt key as the reader does. Each type is described once, under `definitions`, and referenced from where it is used.
#[must_use]
pub fn config_schema(registry: &[&dyn DynConnector]) -> Schema {
    let mut generator = generator();
    // The options come first, so that their types are in the generator when the root takes its definitions.
    let modules: Vec<(String, Value)> = registry
        .iter()
        .map(|connector| {
            let mut schema = connector.options_schema(&mut generator);
            // The generator transforms the root and its definitions, not a schema written inline here.
            RecursiveTransform(without_null).transform(&mut schema);
            (connector.meta().name.to_owned(), module_table(schema))
        })
        .collect();

    let mut schema = generator.into_root_schema_for::<Keys>();
    schema.insert("title".to_owned(), "gsttt config".into());
    let object = schema.ensure_object();
    if let Some(properties) = object
        .entry("properties")
        .or_insert_with(|| json!({}))
        .as_object_mut()
    {
        properties.extend(modules);
    }
    schema
}

/// The JSON Schema of a pack file in `.gsttt/packs/`: the fields of a pack and the five kinds of rules, each table closed to keys it does not have.
#[must_use]
pub fn pack_schema() -> Schema {
    let mut schema = generator().into_root_schema_for::<Pack>();
    schema.insert("title".to_owned(), "gsttt rule pack".into());
    schema
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluate::KEYS;

    #[test]
    fn the_keys_the_reader_knows_are_the_properties_of_the_schema() {
        let schema = config_schema(&[]);
        let properties = schema.get("properties").and_then(Value::as_object).unwrap();
        let mut in_schema: Vec<&str> = properties.keys().map(String::as_str).collect();
        in_schema.sort_unstable();
        let mut in_reader = KEYS.to_vec();
        in_reader.sort_unstable();
        assert_eq!(in_schema, in_reader);
    }
}
