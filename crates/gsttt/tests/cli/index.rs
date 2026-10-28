//! `index` builds the index and says what it did, and `status` says how the stored one stands without touching it.

use std::fs;
use std::path::PathBuf;

use gsttt_scenario::scenario;
use serde_json::Value;

use super::support::{
    Sandbox, commit_all, init_repo, is_empty, listing, typescript_project, write,
};

/// The coverage line of the project of `typescript_project`.
const COVERAGE: &str = "coverage: ts read 3 of 3 files, 100% of imports resolved";

/// The one snapshot a run left under `dir`.
fn the_snapshot(dir: &std::path::Path) -> PathBuf {
    let found: Vec<String> = listing(dir)
        .into_iter()
        .filter(|path| path.ends_with("snapshot.gsttt"))
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    dir.join(&found[0])
}

#[scenario("cli", "index reports what it parsed and reused")]
#[test]
fn index_reports_what_it_parsed_and_reused() {
    let sandbox = Sandbox::new();
    typescript_project(sandbox.work.path());

    let first = sandbox.run(&["index"]);
    assert_eq!(first.code, Some(0), "{first:?}");
    assert!(first.stdout.starts_with("Indexed 3 files in "), "{first:?}");
    assert!(
        first.stdout.contains(": 3 parsed, 0 reused.\n"),
        "{first:?}"
    );

    write(sandbox.work.path(), "app/b.ts", "export const b = 22;\n");
    let second = sandbox.run(&["index"]);
    assert_eq!(second.code, Some(0), "{second:?}");
    assert!(
        second.stdout.starts_with("Indexed 3 files in "),
        "{second:?}"
    );
    assert!(
        second
            .stdout
            .ends_with(&format!(": 1 parsed, 2 reused.\n{COVERAGE}\n")),
        "{second:?}"
    );
    assert_eq!(second.stderr, "");
}

#[scenario("cli", "index on an unchanged tree says so")]
#[test]
fn index_on_an_unchanged_tree_says_so() {
    let sandbox = Sandbox::new();
    typescript_project(sandbox.work.path());
    assert_eq!(sandbox.run(&["index"]).code, Some(0));

    let second = sandbox.run(&["index"]);
    assert_eq!(second.code, Some(0), "{second:?}");
    assert_eq!(
        second.stdout,
        format!("The index already describes the tree: 3 files.\n{COVERAGE}\n")
    );
}

#[scenario("cli", "index --json carries the counts")]
#[test]
fn index_json_carries_the_counts() {
    let sandbox = Sandbox::new();
    typescript_project(sandbox.work.path());

    let ran = sandbox.run(&["--json", "index"]);
    assert_eq!(ran.code, Some(0), "{ran:?}");
    let summary: Value = serde_json::from_str(&ran.stdout).unwrap();
    let object = summary.as_object().unwrap();
    assert_eq!(object["files"], 3);
    assert_eq!(object["parsed"], 3);
    assert_eq!(object["reused"], 0);
    assert_eq!(object["unchanged"], false);
    assert_eq!(
        object["head"],
        Value::Null,
        "the directory is in no git repository"
    );
    assert_eq!(object["dirty"], 0);
    assert!(object["elapsed_ms"].is_u64(), "{summary}");
    assert_eq!(object["coverage"], COVERAGE);
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "coverage",
            "dirty",
            "elapsed_ms",
            "files",
            "head",
            "parsed",
            "reused",
            "unchanged"
        ]
    );

    let again: Value = serde_json::from_str(&sandbox.run(&["--json", "index"]).stdout).unwrap();
    assert_eq!(again["unchanged"], true, "{again}");
    assert_eq!(again["parsed"], 0);
    assert_eq!(again["reused"], 3);
}

#[test]
fn inside_git_the_summary_names_the_commit_and_the_files_that_differ_from_it() {
    let sandbox = Sandbox::new();
    let repo = sandbox.work.path();
    init_repo(repo);
    typescript_project(repo);
    let head = commit_all(repo);
    let short = head.get(..7).unwrap();

    let clean = sandbox.run(&["index"]);
    assert!(
        clean
            .stdout
            .starts_with(&format!("Indexed 3 files at {short} in ")),
        "{clean:?}"
    );

    write(repo, "app/b.ts", "export const b = 22;\n");
    let dirty = sandbox.run(&["index"]);
    assert!(
        dirty
            .stdout
            .starts_with(&format!("Indexed 3 files at {short} (1 dirty) in ")),
        "{dirty:?}"
    );

    let unchanged = sandbox.run(&["index"]);
    assert!(
        unchanged.stdout.starts_with(&format!(
            "The index already describes the tree: 3 files at {short}.\n"
        )),
        "{unchanged:?}"
    );

    let json: Value = serde_json::from_str(&sandbox.run(&["--json", "index"]).stdout).unwrap();
    assert_eq!(json["head"], head.as_str());
    assert_eq!(json["dirty"], 1);
}

#[scenario("cli", "status does not refresh")]
#[test]
fn status_does_not_refresh() {
    let sandbox = Sandbox::new();
    typescript_project(sandbox.work.path());
    assert_eq!(sandbox.run(&["index"]).code, Some(0));
    let snapshot = the_snapshot(sandbox.cache.path());
    let stored = || {
        (
            fs::read(&snapshot).unwrap(),
            fs::metadata(&snapshot).unwrap().modified().unwrap(),
        )
    };
    let before = stored();

    write(sandbox.work.path(), "app/b.ts", "export const b = 22;\n");
    for _ in 0..2 {
        let status = sandbox.run(&["status"]);
        assert_eq!(status.code, Some(0), "{status:?}");
        assert!(
            status
                .stdout
                .contains("1 file changed since the index was built"),
            "{status:?}"
        );
    }
    assert!(stored() == before, "the snapshot was rewritten");
}

#[scenario("cli", "status without an index exits 1")]
#[test]
fn status_without_an_index_exits_1() {
    let sandbox = Sandbox::new();
    typescript_project(sandbox.work.path());

    let ran = sandbox.run(&["status"]);
    assert_eq!(ran.code, Some(1), "{ran:?}");
    assert_eq!(ran.stdout, "");
    assert!(ran.stderr.contains("there is no index yet for"), "{ran:?}");
    assert!(is_empty(sandbox.cache.path()), "status built an index");
}

#[scenario("cli", "status names the version --version prints")]
#[test]
fn status_names_the_version_of_the_binary() {
    let sandbox = Sandbox::new();
    typescript_project(sandbox.work.path());
    assert_eq!(sandbox.run(&["index"]).code, Some(0));

    let printed = sandbox.run(&["--version"]).stdout;
    let version = printed.trim_end().strip_prefix("gsttt ").unwrap();
    let status = sandbox.run(&["status"]);
    assert_eq!(status.code, Some(0), "{status:?}");
    let built_by = status
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("built by gsttt "))
        .unwrap_or_else(|| panic!("status does not say what built the index: {status:?}"));
    assert_eq!(
        built_by.split([' ', ',']).next(),
        Some(version),
        "{status:?}"
    );
}
