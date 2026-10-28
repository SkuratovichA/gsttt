//! The questions answer from the index they refresh first: the subject they are asked about, the cut of long lists,
//! and what `--no-refresh` and `--json` change.

use std::fs;

use gsttt_scenario::scenario;
use serde_json::Value;

use super::support::{Ran, Sandbox, init_repo, json, typescript_project, write};

/// The lines of `text` that name one of the importers `importers` wrote.
fn listed(text: &str) -> usize {
    text.lines()
        .filter(|line| line.starts_with("importers/"))
        .count()
}

/// A git repository in which 45 files import `lib/util.ts`: `app/a.ts` of the project and 44 `importers`.
fn forty_five_importers(sandbox: &Sandbox) -> std::path::PathBuf {
    let repo = sandbox.work.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    typescript_project(&repo);
    for number in 1..=44 {
        let text =
            format!("import {{ util }} from '../lib/util';\nexport const f{number} = util;\n");
        write(&repo, &format!("importers/f{number:02}.ts"), &text);
    }
    repo.canonicalize().unwrap()
}

#[scenario("cli", "A subject the index does not hold exits 1")]
#[test]
fn a_subject_the_index_does_not_hold_exits_1() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    // The last case answers from the index the cases before it built.
    for args in [
        &["dependencies", "lib/missing.ts"][..],
        &["dependents", "lib/missing.ts"][..],
        &["path", "app/a.ts", "lib/missing.ts"][..],
        &["path", "lib/missing.ts", "app/a.ts"][..],
        &["--json", "dependencies", "lib/missing.ts"][..],
        &["--no-refresh", "dependencies", "lib/missing.ts"][..],
    ] {
        let run = sandbox.run_in(&repo, args);
        assert_eq!(run.code, Some(1), "{args:?}: {run:?}");
        assert_eq!(run.stdout, "", "{args:?}");
        assert!(
            run.stderr.contains("lib/missing.ts is not in the index"),
            "{args:?}: {run:?}"
        );
    }
}

#[test]
fn a_subject_the_index_holds_but_nothing_imports_is_an_answer() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    let run = sandbox.run_in(&repo, &["--json", "dependents", "app/b.ts"]);
    let answer = json(&run);
    assert_eq!(answer["in_index"], true);
    assert_eq!(answer["files"], Value::Array(Vec::new()));
}

#[scenario("cli", "A question sees an edit made after the last index")]
#[test]
fn a_question_sees_an_edit_made_after_the_last_index() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    assert_eq!(sandbox.run_in(&repo, &["index"]).code, Some(0));
    write(&repo, "app/b.ts", "import { util } from '../lib/util';\n");

    let run = sandbox.run_in(&repo, &["dependents", "lib/util.ts"]);
    assert_eq!(run.code, Some(0), "{run:?}");
    assert!(run.stdout.contains("app/b.ts"), "{run:?}");
    assert!(run.stdout.contains("app/a.ts"), "{run:?}");

    // The question stored what it refreshed, so the next one has nothing to catch up with.
    let status = sandbox.run_in(&repo, &["status"]);
    assert!(
        status
            .stdout
            .contains("nothing has changed since the index was built"),
        "{status:?}"
    );
}

#[scenario("cli", "--no-refresh answers from the stored index")]
#[test]
fn no_refresh_answers_from_the_stored_index() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    assert_eq!(sandbox.run_in(&repo, &["index"]).code, Some(0));
    write(&repo, "app/b.ts", "import { util } from '../lib/util';\n");

    let run = sandbox.run_in(&repo, &["--no-refresh", "dependents", "lib/util.ts"]);
    assert_eq!(run.code, Some(0), "{run:?}");
    assert!(run.stdout.contains("app/a.ts"), "{run:?}");
    assert!(!run.stdout.contains("app/b.ts"), "{run:?}");
    assert!(
        run.stdout
            .ends_with("stale: 1 file changed since the index\n"),
        "{run:?}"
    );
}

#[scenario("cli", "A long list is cut")]
#[test]
fn a_long_list_is_cut() {
    let sandbox = Sandbox::new();
    let repo = forty_five_importers(&sandbox);

    let run = sandbox.run_in(&repo, &["dependents", "lib/util.ts"]);
    assert_eq!(run.code, Some(0), "{run:?}");
    // `app/a.ts` sorts first, so the 30 lines shown are it and 29 of the importers.
    assert!(
        run.stdout
            .starts_with("dependents of lib/util.ts (45):\napp/a.ts"),
        "{run:?}"
    );
    assert_eq!(listed(&run.stdout), 29, "{run:?}");
    assert!(
        run.stdout.lines().any(|line| line == "… 15 more"),
        "{run:?}"
    );
}

#[scenario("cli", "--all lists every entry")]
#[test]
fn all_lists_every_entry() {
    let sandbox = Sandbox::new();
    let repo = forty_five_importers(&sandbox);

    let run = sandbox.run_in(&repo, &["--all", "dependents", "lib/util.ts"]);
    assert_eq!(run.code, Some(0), "{run:?}");
    assert_eq!(listed(&run.stdout), 44, "{run:?}");
    assert!(run.stdout.contains("\napp/a.ts"), "{run:?}");
    assert!(!run.stdout.contains("more\n"), "{run:?}");
}

#[scenario("cli", "--json prints one document with every entry")]
#[test]
fn json_prints_one_document_with_every_entry() {
    let sandbox = Sandbox::new();
    let repo = forty_five_importers(&sandbox);

    let cut = json(&sandbox.run_in(&repo, &["--json", "dependents", "lib/util.ts"]));
    assert_eq!(cut["files"].as_array().unwrap().len(), 45);
    let all = json(&sandbox.run_in(&repo, &["--json", "--all", "dependents", "lib/util.ts"]));
    assert_eq!(all, cut, "--all changes nothing under --json");
}

#[test]
fn dependencies_path_and_coverage_answer_from_the_index() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();

    let dependencies = sandbox.run_in(&repo, &["dependencies", "app/a.ts"]);
    assert_eq!(dependencies.code, Some(0), "{dependencies:?}");
    assert!(
        dependencies
            .stdout
            .starts_with("dependencies of app/a.ts\n"),
        "{dependencies:?}"
    );
    assert!(
        dependencies
            .stdout
            .contains("\nlib/util.ts  static import\n"),
        "{dependencies:?}"
    );

    let path = sandbox.run_in(&repo, &["path", "app/a.ts", "lib/util.ts"]);
    assert_eq!(path.code, Some(0), "{path:?}");
    assert!(
        path.stdout
            .starts_with("path from app/a.ts to lib/util.ts, 1 hop\n"),
        "{path:?}"
    );

    let coverage = sandbox.run_in(&repo, &["coverage"]);
    assert_eq!(coverage.code, Some(0), "{coverage:?}");
    assert!(
        coverage
            .stdout
            .starts_with("ts: 3 files in the repository root\n"),
        "{coverage:?}"
    );
}

#[test]
fn contract_joins_its_words_into_one_query() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();

    let run = sandbox.run_in(&repo, &["--json", "contract", "http", "GET", "/x"]);
    let answer = json(&run);
    assert_eq!(answer["query"], "http GET /x");
    assert_eq!(answer["contracts"], 0);

    let text = sandbox.run_in(&repo, &["contract", "http", "GET", "/x"]);
    assert_eq!(text.code, Some(0), "an empty lookup is an answer: {text:?}");
    assert!(
        text.stdout
            .starts_with("no contract key contains `http GET /x` among 0 contracts\n"),
        "{text:?}"
    );
}

/// The line `index` ends with is the line a question ends its footer with. The two are written in different places,
/// because `gsttt-core` offers no way to take the one from the other.
#[test]
fn the_coverage_line_of_index_is_the_one_a_question_ends_with() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();

    let index = sandbox.run_in(&repo, &["index"]);
    let coverage = sandbox.run_in(&repo, &["coverage"]);
    let line_of = |run: &Ran| {
        run.stdout
            .lines()
            .find(|line| line.starts_with("coverage: "))
            .map(str::to_owned)
    };
    assert!(line_of(&index).is_some(), "{index:?}");
    assert_eq!(line_of(&index), line_of(&coverage));
}

/// The document of `gsttt coverage --json` ends with the key `problems`. Serde writes the keys in the order of the fields of the report, so a field added after `problems` would move it.
#[test]
fn the_coverage_document_ends_with_problems() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();

    let coverage = sandbox.run_in(&repo, &["coverage", "--json"]);

    let document = json(&coverage);
    let problems = &document["problems"];
    assert!(problems.is_array(), "{coverage:?}");
    let ending = format!(",\"problems\":{problems}}}");
    assert!(
        coverage.stdout.trim_end().ends_with(&ending),
        "{coverage:?}"
    );
}
