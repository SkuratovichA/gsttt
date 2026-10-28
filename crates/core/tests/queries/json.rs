//! The JSON of every answer holds everything its text holds, with no list cut.

use gsttt_core::contract::Role;
use gsttt_core::fact::{Import, ImportKind, ParseFailure, UnresolvedReason};
use gsttt_core::index::Freshness;
use gsttt_core::query::{
    Answer, Entries, ImpactRequest, contract, coverage, dependencies, dependents, impact, path,
    status,
};
use gsttt_scenario::scenario;
use serde_json::{Value, json, to_value};

use crate::fixture::{To, Tree, package, resolving};

fn hub(count: usize) -> Tree {
    (0..count).fold(Tree::new(), |tree, n| {
        tree.imports(&format!("src/user{n:02}.ts"), "src/hub.ts")
    })
}

#[scenario("queries", "JSON keeps every entry")]
#[test]
fn json_keeps_every_entry() {
    let index = hub(45).build();
    let answer = dependents(&index, Freshness::Fresh, "src/hub.ts");

    let value = to_value(&answer).unwrap();

    let files = value["files"].as_array().unwrap();
    assert_eq!(files.len(), 45);
    assert_eq!(files[0]["path"], "src/user00.ts");
    assert_eq!(files[44]["path"], "src/user44.ts");
    assert!(answer.to_string().contains("… 15 more"));
}

#[test]
fn json_holds_the_counts_behind_the_share_of_resolved_imports() {
    let index = resolving(29, 100).build();
    let answer = dependencies(&index, Freshness::Fresh, "src/a.ts");

    let value = to_value(&answer).unwrap();

    let share = &value["footer"]["coverage"][0];
    assert_eq!(share["imports"], 100);
    assert_eq!(share["resolved_imports"], 29);
}

#[test]
fn json_holds_the_footer_as_data() {
    let index = hub(2).commit("a1b2c3d4e5", 3).build();
    let stale = Freshness::Stale {
        added: 1,
        removed: 0,
        changed: 2,
    };

    let value = to_value(dependents(&index, stale, "src/hub.ts")).unwrap();

    assert_eq!(
        value["footer"]["freshness"],
        json!({"state": "stale", "added": 1, "removed": 0, "changed": 2})
    );
    assert_eq!(value["footer"]["head"], "a1b2c3d4e5");
    assert_eq!(value["footer"]["dirty"], 3);
    assert_eq!(value["footer"]["coverage"][0]["connector"], "ts");
    assert_eq!(value["footer"]["coverage"][0]["read"], 3);
}

#[test]
fn json_keeps_the_edge_kinds_of_a_dependent() {
    let index = Tree::new()
        .import(
            "b.ts",
            Import::new("./a", ImportKind::ReExportAll, 1),
            crate::fixture::file("a.ts"),
        )
        .build();

    let value = to_value(dependents(&index, Freshness::Fresh, "a.ts")).unwrap();

    assert_eq!(
        value["files"][0],
        json!({"path": "b.ts", "kinds": ["re-export-all"], "type_only": false})
    );
}

#[test]
fn json_of_dependencies_keeps_packages_builtins_and_unresolved_imports() {
    let index = Tree::new()
        .import(
            "a.ts",
            Import::new("react", ImportKind::Static, 2),
            package("react"),
        )
        .import(
            "a.ts",
            Import::new("node:fs", ImportKind::Static, 3),
            To::Builtin,
        )
        .import(
            "a.ts",
            Import::new("./missing", ImportKind::Static, 4),
            To::Missing(UnresolvedReason::NotFound),
        )
        .build();

    let value = to_value(dependencies(&index, Freshness::Fresh, "a.ts")).unwrap();

    assert_eq!(value["packages"], json!([{"name": "react", "lines": [2]}]));
    assert_eq!(
        value["builtins"],
        json!([{"name": "node:fs", "lines": [3]}])
    );
    assert_eq!(value["unresolved"][0]["spec"], "./missing");
    assert_eq!(value["unresolved"][0]["line"], 4);
    assert_eq!(value["unresolved"][0]["reason"]["reason"], "not-found");
}

#[test]
fn json_of_impact_keeps_the_reason_of_each_file() {
    let index = Tree::new()
        .imports("b.ts", "a.ts")
        .serves("a.ts", "http GET /x")
        .calls("c.ts", "http GET /x")
        .build();
    let request = ImpactRequest {
        files: vec!["a.ts".to_owned()],
        ..ImpactRequest::default()
    };

    let value = to_value(impact(&index, Freshness::Fresh, &request)).unwrap();

    assert_eq!(
        value["affected"],
        json!([
            {"path": "b.ts", "distance": 1, "reason": {"via": "imports", "file": "a.ts"}},
            {"path": "c.ts", "distance": 1, "reason": {"via": "contract", "role": "calls", "key": "http GET /x"}},
        ])
    );
}

#[test]
fn json_of_a_path_keeps_each_hop() {
    let index = Tree::new()
        .imports("page.tsx", "api.ts")
        .calls("api.ts", "http GET /x")
        .serves("server.ts", "http GET /x")
        .build();

    let value = to_value(path(&index, Freshness::Fresh, "page.tsx", "server.ts")).unwrap();

    assert_eq!(
        value["steps"],
        json!([
            {"path": "page.tsx", "by": null},
            {"path": "api.ts", "by": {"via": "import", "kinds": ["static"]}},
            {"path": "server.ts", "by": {"via": "contract", "role": "serves", "key": "http GET /x"}},
        ])
    );
}

#[test]
fn json_of_a_missing_path_keeps_the_reach() {
    let index = Tree::new()
        .imports("a.ts", "b.ts")
        .imports("c.ts", "d.ts")
        .build();

    let value = to_value(path(&index, Freshness::Fresh, "a.ts", "d.ts")).unwrap();

    assert_eq!(value["steps"], json!([]));
    assert_eq!(value["reach"], 1);
}

#[test]
fn json_of_a_contract_lookup_says_which_kind_of_lookup_it_was() {
    let index = Tree::new()
        .declares("schema.sql", "table bookings")
        .serves("repo.ts", "table bookings")
        .build();

    let key = to_value(contract(&index, Freshness::Fresh, "table bookings")).unwrap();
    let search = to_value(contract(&index, Freshness::Fresh, "book")).unwrap();

    assert_eq!(key["found"]["lookup"], "key");
    assert_eq!(key["found"]["serves"][0]["path"], "repo.ts");
    assert_eq!(search["found"]["lookup"], "search");
    assert_eq!(
        search["found"]["matches"],
        json!([{"key": "table bookings", "declares": 1, "serves": 1, "calls": 0}])
    );
}

#[test]
fn json_of_coverage_keeps_failures_reasons_and_unclaimed_files() {
    let index = Tree::new()
        .failed(
            "src/broken.ts",
            ParseFailure::Syntax {
                line: 3,
                message: "unexpected token".to_owned(),
            },
        )
        .import(
            "src/a.ts",
            Import::new("./gone", ImportKind::Static, 7),
            To::Missing(UnresolvedReason::NotFound),
        )
        .blind(
            "src/client.ts",
            12,
            UnresolvedReason::NonLiteral,
            "fetch(url)",
        )
        .unread("src/main.kt")
        .build();

    let value = to_value(coverage(&index, Freshness::Fresh)).unwrap();

    let ts = &value["connectors"][0];
    assert_eq!(
        ts["failed"],
        json!([{"path": "src/broken.ts", "failure": "syntax", "line": 3, "message": "unexpected token"}])
    );
    assert_eq!(
        ts["unresolved"],
        json!([{"reason": "not-found", "count": 1, "first": {"path": "src/a.ts", "line": 7}}])
    );
    assert_eq!(
        ts["unread_sites"],
        json!([{"reason": "non-literal", "count": 1, "first": {"path": "src/client.ts", "line": 12}}])
    );
    assert_eq!(
        value["languages"],
        json!([{"language": "Kotlin", "files": 1, "extensions": ["kt"]}])
    );
}

#[test]
fn json_of_status_keeps_the_timings_in_microseconds() {
    let mut index = Tree::new()
        .imports("b.ts", "a.ts")
        .commit("a1b2c3d4e5", 3)
        .build();
    index.info.timings.total = 410_100;

    let value = to_value(status(&index, Freshness::Fresh)).unwrap();

    assert_eq!(value["timings"]["total"], 410_100);
    assert_eq!(value["built_by"], "0.0.0");
    assert_eq!(value["files"], 2);
    assert_eq!(value["freshness"], json!({"state": "fresh"}));
}

#[test]
fn every_answer_serialises_with_a_list_longer_than_the_text_shows() {
    let index = hub(45).build();
    let every = dependents(&index, Freshness::Fresh, "src/hub.ts").text(Entries::All);

    // Text with every entry asked for and JSON list the same files.
    let value: Value = to_value(dependents(&index, Freshness::Fresh, "src/hub.ts")).unwrap();
    let in_json: Vec<&str> = value["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect();
    let in_text: Vec<&str> = every
        .lines()
        .filter(|line| line.starts_with("src/user"))
        .map(|line| line.split("  ").next().unwrap())
        .collect();
    assert_eq!(in_json, in_text);
    assert_eq!(Role::Calls.to_string(), "calls");
}
