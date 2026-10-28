//! gsttt reading its own repository, the one tree these tests are allowed to point at.

use serde_json::{Value, json};

use crate::support::{Indexed, index_at};

/// Where the repository starts, two levels above this crate.
const REPOSITORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// Whether the project rooted at `root` names `needle` among its blind spots.
fn names(out: &Indexed, root: &str, needle: &str) -> bool {
    out.provides(root)
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|entry| entry["blind_spots"].as_array().unwrap())
        .any(|spot| spot.as_str().is_some_and(|spot| spot.contains(needle)))
}

/// Whether the blind spots explain an import that points nowhere. Code a build step generated is one. A path that starts with a name a glob import brought in is the other, but only a glob of a crate outside this repository can bring in a name that nothing here declares, so a path that starts with an unknown name in a file whose globs all land on files of the repository is a mistake.
fn explained(reason: &str, file: &Value) -> bool {
    let has_outside_glob = file["imports"].as_array().unwrap().iter().any(|import| {
        import["names"] == json!(["*"])
            && import["to"]["file"].is_null()
            && import["to"]["unresolved"].is_null()
    });
    match reason {
        "generated" => true,
        "unknown-package" => has_outside_glob,
        _ => false,
    }
}

/// Whether `path` is in a `tests/fixtures` directory, the trees that tests read as inputs.
fn is_fixture(path: &str) -> bool {
    path.starts_with("tests/fixtures/") || path.contains("/tests/fixtures/")
}

/// Imports of the Rust files that point nowhere and that the blind spots do not explain, as `<path>:<line> <spec> (<reason>)`. The trees under `tests/fixtures` are left out: they are inputs of tests, and the ones for imports are written to be unresolved.
fn unexplained(out: &Indexed) -> Vec<String> {
    let json = out.json();
    let mut found = Vec::new();
    for file in json["files"].as_array().unwrap() {
        let path = file["path"].as_str().unwrap();
        if is_fixture(path) || file["project"].is_null() {
            continue;
        }
        for import in file["imports"].as_array().unwrap() {
            let Some(reason) = import["to"]["unresolved"]["reason"].as_str() else {
                continue;
            };
            if !explained(reason, file) {
                found.push(format!(
                    "{path}:{} {} ({reason})",
                    import["line"], import["spec"]
                ));
            }
        }
    }
    found
}

#[test]
fn gsttt_reads_its_own_repository() {
    let out = index_at(REPOSITORY);
    // A tree that failed to open would have no imports at all, and nothing unresolved either.
    assert_eq!(
        out.file("crates/lang-rust/src/project.rs")["project"],
        "rust crates/lang-rust"
    );
    assert_eq!(
        out.to(
            "crates/lang-rust/src/project.rs",
            "gsttt_core::path::RepoPath"
        ),
        json!({ "file": "crates/core/src/path.rs" })
    );
    assert_eq!(
        out.to("crates/lang-rust/src/lib.rs", "project"),
        json!({ "file": "crates/lang-rust/src/project.rs" })
    );
    // A module declared inline is a module of the file that holds it, whichever file writes the path.
    assert_eq!(
        out.to("crates/core/src/fact.rs", "self::flag_sets::FileTags"),
        json!({ "file": "crates/core/src/fact.rs" })
    );
    // What stays unresolved is what the project says it cannot read.
    assert!(names(&out, "crates/lang-rust", "`include!`"));
    assert!(names(&out, "crates/lang-rust", "glob import"));
    assert_eq!(unexplained(&out), Vec::<String>::new());
}
