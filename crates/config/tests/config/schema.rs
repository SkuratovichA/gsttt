//! The JSON Schemas of the config and of a pack, which an editor checks the files with.

use std::collections::BTreeSet;

use gsttt_config::{config_schema, pack_schema};
use serde_json::{Value, json};

use crate::fakes::Registry;

/// The schema of the config for the stand-in registry, which is the one the config tests run against.
fn config() -> Value {
    let registry = Registry::standard();
    config_schema(&registry.connectors()).to_value()
}

/// The names of the properties of an object schema.
fn property_names(schema: &Value) -> BTreeSet<&str> {
    schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

/// The description `reference` points to, which is how a draft-07 schema refers to a type.
fn followed<'a>(schema: &'a Value, reference: &Value) -> &'a Value {
    let name = reference["$ref"]
        .as_str()
        .and_then(|reference| reference.strip_prefix("#/definitions/"))
        .unwrap();
    &schema["definitions"][name]
}

#[gsttt_scenario::scenario("config", "Options become a schema of the module's table")]
#[test]
fn options_become_a_schema_of_the_modules_table() {
    let schema = config();
    let ts = followed(&schema, &schema["properties"]["ts"]);
    let options = &ts["properties"];

    assert_eq!(options["tsconfig"]["type"], "string");
    assert_eq!(
        options["tsconfig"]["description"],
        "The tsconfig the project reads, relative to the repository root."
    );
    assert_eq!(options["conditions"]["type"], "array");
    assert_eq!(options["conditions"]["items"]["type"], "string");
    assert_eq!(
        options["conditions"]["description"],
        "Export conditions to follow, in order."
    );
    assert_eq!(
        property_names(ts),
        BTreeSet::from(["tsconfig", "conditions"])
    );
}

#[gsttt_scenario::scenario("config", "The schema closes the top level")]
#[test]
fn the_schema_closes_the_top_level() {
    let schema = config();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        property_names(&schema),
        BTreeSet::from([
            "exclude",
            "http_prefixes",
            "max_file_bytes",
            "detect",
            "disable",
            "disable_packs",
            "external",
            "ts",
            "python",
            "rust",
            "openapi",
        ])
    );
}

#[test]
fn every_table_the_reader_checks_is_closed() {
    let schema = config();
    for module in ["ts", "python", "rust", "openapi"] {
        let table = followed(&schema, &schema["properties"][module]);
        assert_eq!(table["additionalProperties"], false, "{module}");
    }
    let external = followed(&schema, &schema["properties"]["external"]["items"]);
    assert_eq!(external["additionalProperties"], false);
    // The options of an external are a table of named values, which the reader refuses to take as anything else.
    assert_eq!(external["properties"]["options"]["type"], "object");
}

#[gsttt_scenario::scenario("config", "A pack file has a schema")]
#[test]
fn a_pack_file_has_a_schema() {
    let schema = pack_schema().to_value();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        property_names(&schema),
        BTreeSet::from([
            "name",
            "language",
            "detect",
            "calls",
            "decorators",
            "chains",
            "embeds",
            "assigns",
        ])
    );
    // The rules of each kind are tables that refuse unknown keys as well.
    for kind in ["calls", "decorators", "chains", "embeds", "assigns"] {
        assert_eq!(schema["properties"][kind]["type"], "array", "{kind}");
        let rule = followed(&schema, &schema["properties"][kind]["items"]);
        assert_eq!(rule["additionalProperties"], false, "{kind}");
    }
}

#[test]
fn each_type_is_described_once_under_definitions() {
    let schema = config();
    let described: BTreeSet<&str> = schema["definitions"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        described,
        BTreeSet::from(["ExternalSetup", "NoOptions", "Provide", "Role", "TsOptions"])
    );
    // `python`, `rust` and `openapi` have the same options, so they point at the same description.
    let reference = json!({ "$ref": "#/definitions/NoOptions" });
    for module in ["python", "rust", "openapi"] {
        assert_eq!(schema["properties"][module], reference, "{module}");
    }

    // `Role` is the role of every kind of rule and of a `provides` entry, and it is spelled out once.
    let pack = pack_schema().to_value();
    assert_eq!(pack.to_string().matches(r#""const":"declares""#).count(), 1);
    assert_eq!(
        schema.to_string().matches(r#""const":"declares""#).count(),
        1
    );
}

/// The JSON pointer to the first place `schema` offers `null` at: a `null` value, `"type": "null"`, or a `"type"` list that holds `"null"`. `pointer` is where `schema` itself sits.
fn null_at(schema: &Value, pointer: &str) -> Option<String> {
    match schema {
        Value::Null => Some(pointer.to_owned()),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(index, item)| null_at(item, &format!("{pointer}/{index}"))),
        Value::Object(map) => {
            let offers_null = match map.get("type") {
                Some(Value::String(name)) => name == "null",
                Some(Value::Array(names)) => names.iter().any(|name| name == "null"),
                _ => false,
            };
            if offers_null {
                return Some(format!("{pointer}/type"));
            }
            map.iter().find_map(|(key, item)| {
                // RFC 6901 escapes `~` and `/` in a key.
                let key = key.replace('~', "~0").replace('/', "~1");
                null_at(item, &format!("{pointer}/{key}"))
            })
        }
        Value::Bool(_) | Value::Number(_) | Value::String(_) => None,
    }
}

#[test]
fn no_schema_offers_null() {
    // TOML has no null, so an editor must not complete one. The registry holds a module without options and one with optional options, which are `null` in the schema of their types.
    let registry = Registry::with_null_options();
    let config = config_schema(&registry.connectors()).to_value();
    assert_eq!(null_at(&config, ""), None, "the config schema offers null");
    assert_eq!(
        null_at(&pack_schema().to_value(), ""),
        None,
        "the pack schema offers null"
    );

    // A module without options is turned on with an empty table, which the reader gives as `null`, the value of `()`.
    assert_eq!(
        config["properties"]["unit"],
        json!({ "type": "object", "additionalProperties": false })
    );
}

#[test]
fn the_config_schema_is_pinned() {
    let registry = Registry::standard();
    insta::assert_json_snapshot!(config_schema(&registry.connectors()));
}

#[test]
fn the_pack_schema_is_pinned() {
    insta::assert_json_snapshot!(pack_schema());
}
