//! The fingerprint of a setup.

use gsttt_core::Setup;
use gsttt_core::engine::keys::canonical_json;
use xxhash_rust::xxh3::xxh3_128;

/// A hash of the setup's canonical JSON: keys sorted, no whitespace, integers without a fraction.
///
/// The setup holds values and not the text they came from, so a comment or a reformatted line in the config leaves it unchanged, and any value that changes the index changes it. The only way to fail is a setup that cannot be written as JSON, which no setup the evaluator builds is.
pub fn fingerprint(setup: &Setup) -> Result<u128, serde_json::Error> {
    let value = serde_json::to_value(setup)?;
    Ok(xxh3_128(canonical_json(&value).as_bytes()))
}

#[cfg(test)]
mod tests {
    use gsttt_core::setup::ModuleSetup;
    use serde_json::json;

    use super::*;

    #[test]
    fn the_canonical_text_sorts_keys_and_has_no_whitespace() {
        let text = canonical_json(
            &json!({ "b": 1, "a": [true, null, "x\"y"], "c": { "z": 0.5, "y": {} } }),
        );
        assert_eq!(
            text,
            r#"{"a":[true,null,"x\"y"],"b":1,"c":{"y":{},"z":0.5}}"#
        );
    }

    #[test]
    fn equal_setups_have_the_same_fingerprint_and_a_changed_value_changes_it() {
        let base = Setup::default();
        assert_eq!(
            fingerprint(&base).unwrap(),
            fingerprint(&base.clone()).unwrap()
        );
        let mut prefixed = base.clone();
        prefixed.http_prefixes.push("/api".to_owned());
        assert_ne!(fingerprint(&base).unwrap(), fingerprint(&prefixed).unwrap());
        let mut with_module = base.clone();
        with_module.modules.push(ModuleSetup {
            module: "ts".to_owned(),
            options: json!({ "tsconfig": "a.json" }),
        });
        assert_ne!(
            fingerprint(&with_module).unwrap(),
            fingerprint(&base).unwrap()
        );
    }
}
