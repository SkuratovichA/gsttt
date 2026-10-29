//! Fact keys: what a file's cached facts are filed under.
//!
//! A key covers everything `parse` can depend on, so equal keys mean equal facts: the fact format, the connector's name and version, the module options, the packs the connector received, the project's root and salt, the path and the content id. The part that is the same for every file of a module is hashed once and cloned per file.

use serde_json::Value;
use xxhash_rust::xxh3::Xxh3;

use super::files::ContentId;
use crate::connector::{DynConnector, Meta, feed};
use crate::path::RepoPath;
use crate::rules::Pack;
use crate::setup::Setup;

/// A file's facts are filed under this.
pub type FactKey = u128;

/// Bumped whenever the shape of the stored facts changes, which drops every cached fact.
pub const FACT_FORMAT: u32 = 1;

/// The state of the key hash after everything a module contributes.
pub fn module_state(meta: &Meta, options: &Value, packs: &[Pack]) -> Xxh3 {
    let mut hasher = Xxh3::new();
    feed(&mut hasher, &FACT_FORMAT.to_le_bytes());
    feed(&mut hasher, meta.name.as_bytes());
    feed(&mut hasher, &meta.version.to_le_bytes());
    feed(&mut hasher, canonical_json(options).as_bytes());
    let packs = serde_json::to_value(packs).unwrap_or(Value::Null);
    feed(&mut hasher, canonical_json(&packs).as_bytes());
    hasher
}

/// The key of one file of a project of a module. The root is part of it whatever the connector puts in the salt, so a file that moves to another project is never given the facts of the first.
pub fn file_key(module: &Xxh3, root: &str, salt: u64, path: &RepoPath, id: ContentId) -> FactKey {
    let mut hasher = module.clone();
    feed(&mut hasher, root.as_bytes());
    hasher.update(&salt.to_le_bytes());
    feed(&mut hasher, path.as_str().as_bytes());
    hasher.update(id.as_bytes());
    hasher.digest128()
}

/// What decides whether a stored index can answer without a run: the fact format, the whole setup, the connectors with their versions and the built-in packs. The version of gsttt itself is checked by the snapshot.
pub fn config_fingerprint(
    setup: &Setup,
    connectors: &[&dyn DynConnector],
    builtin: &[Pack],
) -> u128 {
    let mut hasher = Xxh3::new();
    feed(&mut hasher, &FACT_FORMAT.to_le_bytes());
    let setup = serde_json::to_value(setup).unwrap_or(Value::Null);
    feed(&mut hasher, canonical_json(&setup).as_bytes());
    for connector in connectors {
        let meta = connector.meta();
        feed(&mut hasher, meta.name.as_bytes());
        feed(&mut hasher, &meta.version.to_le_bytes());
    }
    let builtin = serde_json::to_value(builtin).unwrap_or(Value::Null);
    feed(&mut hasher, canonical_json(&builtin).as_bytes());
    hasher.digest128()
}

/// JSON text with the keys of every object in sorted order, so equal values hash equal whatever order a map holds them in.
pub fn canonical_json(value: &Value) -> String {
    let mut text = String::new();
    write_canonical(value, &mut text);
    text
}

fn write_canonical(value: &Value, text: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            text.push('{');
            for (position, key) in keys.into_iter().enumerate() {
                if position > 0 {
                    text.push(',');
                }
                text.push_str(&Value::String(key.clone()).to_string());
                text.push(':');
                if let Some(child) = map.get(key) {
                    write_canonical(child, text);
                }
            }
            text.push('}');
        }
        Value::Array(items) => {
            text.push('[');
            for (position, item) in items.iter().enumerate() {
                if position > 0 {
                    text.push(',');
                }
                write_canonical(item, text);
            }
            text.push(']');
        }
        other => text.push_str(&other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn objects_hash_the_same_whatever_order_their_keys_were_written_in() {
        let a = json!({ "b": 1, "a": { "y": [1, 2], "x": null } });
        let b = json!({ "a": { "x": null, "y": [1, 2] }, "b": 1 });
        assert_eq!(canonical_json(&a), canonical_json(&b));
        assert_eq!(canonical_json(&a), r#"{"a":{"x":null,"y":[1,2]},"b":1}"#);
    }

    /// A pack with one call rule on `callee` and one chain rule from `tx`, so that two packs may share a name and a place in the list and differ in their body only.
    fn pack(name: &str, callee: &str) -> Pack {
        typed_pack(name, callee, &[], &[])
    }

    /// `pack` with an import rule for each package of `imports` and the types of `root_types` as the root types of its chain rule.
    fn typed_pack(name: &str, callee: &str, imports: &[&str], root_types: &[&str]) -> Pack {
        let imports: Vec<Value> = imports
            .iter()
            .map(|from| json!({ "from": from, "kind": "table", "role": "calls" }))
            .collect();
        serde_json::from_value(json!({
            "name": name,
            "language": "ts",
            "calls": [{ "callee": callee, "kind": "http", "role": "calls", "key": "$arg1" }],
            "chains": [{ "root": "tx", "root_types": root_types, "kind": "table", "role": "calls" }],
            "imports": imports,
        }))
        .unwrap()
    }

    #[test]
    fn every_input_of_a_key_changes_it() {
        let meta = Meta {
            name: "ts",
            version: 1,
            summary: "",
            extensions: &[],
        };
        let id = ContentId::UNKNOWN;
        let path = RepoPath::normalize("a.ts").unwrap();
        let packs = [pack("a", "fetch"), pack("b", "axios.get")];
        let base = module_state(&meta, &Value::Null, &packs);
        let key = file_key(&base, "", 0, &path, id);

        let other_options = module_state(&meta, &json!({ "tsconfig": "x" }), &packs);
        let other_version = module_state(&Meta { version: 2, ..meta }, &Value::Null, &packs);
        // A connector may name the packs in force by their positions alone in its salt, because the key holds every pack: the body of one pack changes it with the names and the order kept, and so does the order.
        let other_body = module_state(
            &meta,
            &Value::Null,
            &[pack("a", "fetch"), pack("b", "axios.post")],
        );
        let other_order = module_state(
            &meta,
            &Value::Null,
            &[pack("b", "axios.get"), pack("a", "fetch")],
        );
        // The two lists are left out of a pack that has none, so that the packs written before they existed hash as they did, and that must not make a pack that lists them hash like one that does not.
        let other_imports = module_state(
            &meta,
            &Value::Null,
            &[
                typed_pack("a", "fetch", &["@prisma/client"], &[]),
                pack("b", "axios.get"),
            ],
        );
        let other_root_types = module_state(
            &meta,
            &Value::Null,
            &[
                typed_pack("a", "fetch", &[], &["PrismaClient"]),
                pack("b", "axios.get"),
            ],
        );
        let other_path = RepoPath::normalize("b.ts").unwrap();
        let other_id = crate::engine::files::blob_id(b"x");
        let keys = [
            file_key(&other_options, "", 0, &path, id),
            file_key(&other_version, "", 0, &path, id),
            file_key(&other_body, "", 0, &path, id),
            file_key(&other_order, "", 0, &path, id),
            file_key(&other_imports, "", 0, &path, id),
            file_key(&other_root_types, "", 0, &path, id),
            file_key(&base, "web", 0, &path, id),
            file_key(&base, "", 1, &path, id),
            file_key(&base, "", 0, &other_path, id),
            file_key(&base, "", 0, &path, other_id),
        ];
        assert!(keys.iter().all(|other| *other != key));
        assert_eq!(key, file_key(&base, "", 0, &path, id));
    }
}
