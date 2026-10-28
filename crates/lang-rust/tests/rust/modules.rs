//! The module tree, on the `modules` fixture.

use gsttt_scenario::scenario;
use insta::assert_json_snapshot;
use serde_json::json;

use crate::support::{index, unresolved};

#[scenario("rust", "Module declarations find their files")]
#[test]
fn module_declarations_find_their_files() {
    let out = index("modules");
    assert_eq!(out.to("src/lib.rs", "a"), json!({ "file": "src/a.rs" }));
    assert_eq!(out.to("src/lib.rs", "b"), json!({ "file": "src/b/mod.rs" }));
    assert_eq!(out.to("src/a.rs", "c"), json!({ "file": "src/a/c.rs" }));
    assert_json_snapshot!(out.file("src/lib.rs")["imports"]);
}

#[scenario("rust", "A path attribute names the file")]
#[test]
fn a_path_attribute_names_the_file() {
    let out = index("modules");
    assert_eq!(
        out.to("src/lib.rs", "schema"),
        json!({ "file": "src/generated/schema.rs" })
    );
}

#[scenario("rust", "A missing module file is not found")]
#[test]
fn a_missing_module_file_is_not_found() {
    let out = index("modules");
    assert_eq!(out.to("src/lib.rs", "gone"), unresolved("not-found"));
}

#[scenario("rust", "Paths stop at the deepest module")]
#[test]
fn paths_stop_at_the_deepest_module() {
    let out = index("modules");
    assert_eq!(
        out.to("src/lib.rs", "crate::a::c::Thing"),
        json!({ "file": "src/a/c.rs" })
    );
    assert_eq!(
        out.to("src/a.rs", "self::c::Thing"),
        json!({ "file": "src/a/c.rs" })
    );
    assert_eq!(
        out.to("src/a/c.rs", "super::Helper"),
        json!({ "file": "src/a.rs" })
    );
}
