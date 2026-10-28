//! Editions, on the `editions` fixture: three crates with the same files, told apart by their manifests.

use gsttt_scenario::scenario;
use serde_json::json;

use crate::support::{index, unresolved};

#[scenario("rust", "A crate without an edition is read as 2015")]
#[test]
fn a_crate_without_an_edition_is_read_as_2015() {
    let out = index("editions");
    // In 2015 `use a::b` starts at the crate root, so the child module `a` of `sub` is not the one it names.
    assert_eq!(
        out.to("legacy/src/sub.rs", "::a::b"),
        json!({ "file": "legacy/src/a.rs" })
    );
    // From 2018 it starts at the module, and the child comes first.
    assert_eq!(
        out.to("modern/src/sub.rs", "a::b"),
        json!({ "file": "modern/src/sub/a.rs" })
    );
}

#[scenario("rust", "A workspace edition is inherited")]
#[test]
fn a_workspace_edition_is_inherited() {
    let out = index("editions");
    assert_eq!(
        out.to("inherited/member/src/sub.rs", "::a::b"),
        json!({ "file": "inherited/member/src/a.rs" })
    );
}

#[scenario(
    "rust",
    "A leading double colon starts at the crate root in 2015 and at a crate after"
)]
#[test]
fn a_leading_double_colon_starts_at_the_crate_root_in_2015_and_at_a_crate_after() {
    let out = index("editions");
    assert_eq!(
        out.to("legacy/src/rooted.rs", "::a::b"),
        json!({ "file": "legacy/src/a.rs" })
    );
    assert_eq!(
        out.to("legacy/src/rooted.rs", "::serde::Serialize"),
        json!({ "external": "serde" })
    );
    assert_eq!(
        out.to("modern/src/rooted.rs", "::a::b"),
        unresolved("unknown-package")
    );
    assert_eq!(
        out.to("modern/src/rooted.rs", "::serde::Serialize"),
        json!({ "external": "serde" })
    );
}
