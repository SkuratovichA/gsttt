//! Names that the scope of a file explains or leaves open, on the `scopes` fixture.

use gsttt_scenario::scenario;
use insta::assert_json_snapshot;
use serde_json::json;

use crate::support::{index, unresolved};

#[test]
fn a_path_that_starts_with_an_inline_module_lands_on_the_file_that_declares_it() {
    let out = index("scopes");
    assert_eq!(
        out.to("src/lib.rs", "self::shapes::Circle"),
        json!({ "file": "src/lib.rs" })
    );
    assert_eq!(
        out.to("src/lib.rs", "self::shapes::round::Ball"),
        json!({ "file": "src/lib.rs" })
    );
    assert_json_snapshot!(out.file("src/lib.rs")["imports"]);
}

/// The connector reads no dependency, so it cannot tell what `use proptest::prelude::*` brings in. The blind spots say so; this pins the answer so that a better one is a decision, made together with the sentence.
#[scenario("rust", "A name a glob import brings in is an unknown package")]
#[test]
fn a_name_a_glob_brings_in_is_an_unknown_package() {
    let out = index("scopes");
    assert_eq!(
        out.to("src/strategies.rs", "proptest::prelude"),
        json!({ "external": "proptest" })
    );
    assert_eq!(
        out.to("src/strategies.rs", "prop::collection::vec"),
        unresolved("unknown-package")
    );
    // Every project says so among its blind spots.
    assert!(
        out.json()["projects"]
            .to_string()
            .contains("names that a glob import brings in")
    );
}
