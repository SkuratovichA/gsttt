//! What a file's imports say, on the `imports` fixture.

use gsttt_core::fact::{FileTags, ImportFlags};
use gsttt_scenario::scenario;
use insta::assert_json_snapshot;
use serde_json::json;

use crate::support::{index, printed, unresolved};

#[scenario("rust", "Every import form is read")]
#[test]
fn every_import_form_is_read() {
    let out = index("imports");
    assert_json_snapshot!(out.file("src/forms.rs"));
}

#[scenario("rust", "Paths in macros are read")]
#[test]
fn paths_in_macros_are_read() {
    let out = index("imports");
    let imports = out.file("src/macros.rs")["imports"].clone();
    assert_eq!(imports.as_array().unwrap().len(), 1);
    assert_eq!(imports[0]["spec"], "crate::a::helper");
    assert_eq!(imports[0]["line"], 2);
    assert_eq!(imports[0]["to"], json!({ "file": "src/a.rs" }));
}

#[scenario("rust", "Conditional imports are flagged")]
#[test]
fn conditional_imports_are_flagged() {
    let out = index("imports");
    let imports = out.file("src/conditional.rs")["imports"].clone();
    assert_json_snapshot!(imports);
    assert_eq!(imports[0]["flags"], printed(&ImportFlags::TEST_ONLY));
    assert_eq!(imports[1]["flags"], printed(&ImportFlags::OPTIONAL));
    assert_eq!(imports[2]["flags"], printed(&ImportFlags::TEST_ONLY));
}

#[scenario("rust", "A recoverable error keeps the imports")]
#[test]
fn a_recoverable_error_keeps_the_imports() {
    let out = index("imports");
    let file = out.file("src/broken.rs");
    assert_json_snapshot!(file);
    assert_eq!(file["imports"][0]["spec"], "crate::a::B");
}

#[scenario("rust", "Test files are tagged")]
#[test]
fn test_files_are_tagged() {
    let out = index("imports");
    for tagged in ["tests/api.rs", "src/a/tests.rs", "src/parse_tests.rs"] {
        assert_eq!(
            out.file(tagged)["tags"],
            printed(&FileTags::TEST),
            "{tagged}"
        );
    }
    assert_eq!(out.file("src/a.rs")["tags"], printed(&FileTags::NONE));
}

#[scenario("rust", "Aliases resolve through their use")]
#[test]
fn aliases_resolve_through_their_use() {
    let out = index("imports");
    assert_eq!(
        out.to("src/aliases.rs", "cc::Thing::new"),
        json!({ "file": "src/a/c.rs" })
    );
}

#[scenario("rust", "Includes resolve beside the file")]
#[test]
fn includes_resolve_beside_the_file() {
    let out = index("imports");
    assert_eq!(
        out.to("src/a.rs", "../fixtures/a.json"),
        json!({ "file": "fixtures/a.json" })
    );
    assert_eq!(
        out.to("src/a.rs", "${OUT_DIR}/gen.rs"),
        unresolved("generated")
    );
}
