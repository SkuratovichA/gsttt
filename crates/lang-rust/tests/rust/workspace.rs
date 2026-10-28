//! Projects, crate targets and dependencies, on the `workspace` fixture.

use gsttt_core::fact::FileTags;
use gsttt_scenario::scenario;
use insta::assert_json_snapshot;
use serde_json::json;

use crate::support::{index, printed, unresolved};

#[scenario("rust", "Every manifest is a project")]
#[test]
fn every_manifest_is_a_project() {
    let out = index("workspace");
    let roots = out.roots();
    assert_json_snapshot!(roots);
    for root in ["crates/core", "crates/cli"] {
        assert!(
            roots.iter().any(|candidate| candidate == root),
            "{root} is not among {roots:?}"
        );
    }
    let json = out.json();
    let projects = json["projects"].as_array().unwrap();
    assert!(
        projects
            .iter()
            .all(|project| project["connector"] == "rust")
    );
    assert_eq!(
        out.file("crates/core/src/lib.rs")["project"],
        "rust crates/core"
    );
    assert_eq!(out.file("Cargo.toml")["tags"], printed(&FileTags::MANIFEST));
}

#[scenario("rust", "Explicit targets are crate roots")]
#[test]
fn explicit_targets_are_crate_roots() {
    let out = index("workspace");
    assert_eq!(
        out.to("crates/cli/src/tool/entry.rs", "args"),
        json!({ "file": "crates/cli/src/tool/args.rs" })
    );
}

#[scenario("rust", "A workspace dependency takes its package from the root")]
#[test]
fn a_workspace_dependency_takes_its_package_from_the_root() {
    let out = index("workspace");
    assert_eq!(
        out.to("crates/web/src/main.rs", "engine::graph::Graph"),
        json!({ "file": "crates/core/src/graph.rs" })
    );
}

#[scenario("rust", "Targets of a package see its library")]
#[test]
fn targets_of_a_package_see_its_library() {
    let out = index("workspace");
    assert_eq!(
        out.to("crates/core/tests/engine.rs", "gsttt_core::index::Index"),
        json!({ "file": "crates/core/src/index.rs" })
    );
}

#[scenario("rust", "Dependencies, builtins and unknown names are told apart")]
#[test]
fn dependencies_builtins_and_unknown_names_are_told_apart() {
    let out = index("workspace");
    let imports = out.file("crates/web/src/lib.rs")["imports"].clone();
    assert_json_snapshot!(imports);
    assert_eq!(
        out.to("crates/web/src/lib.rs", "serde::Deserialize"),
        json!({ "external": "serde" })
    );
    assert_eq!(
        out.to("crates/web/src/lib.rs", "tokio::sync::Mutex"),
        json!({ "external": "tokio" })
    );
    assert_eq!(
        out.to("crates/web/src/lib.rs", "std::collections::HashMap"),
        json!("builtin")
    );
    assert_eq!(
        out.to("crates/web/src/lib.rs", "rand::random"),
        unresolved("unknown-package")
    );
}

#[scenario("rust", "Workspace crates resolve to their modules")]
#[test]
fn workspace_crates_resolve_to_their_modules() {
    let out = index("workspace");
    assert_eq!(
        out.to("crates/cli/src/tool/entry.rs", "gsttt_core::index::Index"),
        json!({ "file": "crates/core/src/index.rs" })
    );
    assert_eq!(
        out.to("crates/renamed/src/lib.rs", "engine::graph::Graph"),
        json!({ "file": "crates/core/src/graph.rs" })
    );
}
