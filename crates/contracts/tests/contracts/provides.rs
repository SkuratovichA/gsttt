//! What the connectors say they provide and what they cannot see.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::support::{Run, all_modules, sorted};

#[gsttt_scenario::scenario("contract-formats", "Blind spots are reported")]
#[test]
fn blind_spots_are_reported() {
    let run = Run::new("all_formats", &all_modules());
    let blind_spots: BTreeSet<&str> = run
        .output
        .projects
        .iter()
        .flat_map(|project| &project.provides)
        .flat_map(|provide| &provide.blind_spots)
        .map(String::as_str)
        .collect();
    for expected in [
        "GraphQL fragments spread from other files",
        "environment variables set in compose files",
        "CI and deployment manifests",
        "schemas generated at build time",
    ] {
        assert!(
            blind_spots.contains(expected),
            "{expected}: {blind_spots:?}"
        );
    }

    let provides: Vec<Value> = run
        .output
        .projects
        .iter()
        .map(|project| json!({ "connector": project.connector.name, "provides": project.provides }))
        .collect();
    insta::assert_json_snapshot!(sorted(Value::Array(provides)));
}
