//! `impact` takes paths or `--changed`, passes what is gone as deleted, follows dependents as far as `--depth` lets it, and with `--name` lists the callers of a name.

use std::fs;

use gsttt_scenario::scenario;
use serde_json::{Value, json as value};

use super::support::{Sandbox, commit_all, git, is_empty, json, typescript_project, write};

/// The paths of the entries of `list`, a list of answers that carry one.
fn paths(list: &Value) -> Vec<&str> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap())
        .collect()
}

#[scenario("cli", "--changed includes untracked files")]
#[test]
fn changed_includes_untracked_files() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    // Created, and never added to git.
    write(&repo, "app/new.ts", "export const n = 1;\n");

    let answer = json(&sandbox.run_in(&repo, &["--json", "impact", "--changed"]));
    assert_eq!(answer["changed"], value!(["app/new.ts"]));
    let text = sandbox.run_in(&repo, &["impact", "--changed"]);
    assert!(
        text.stdout.starts_with("impact of app/new.ts\n"),
        "{text:?}"
    );
}

#[scenario("cli", "--changed passes a deleted file as deleted")]
#[test]
fn changed_passes_a_deleted_file_as_deleted() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    fs::remove_file(repo.join("lib/util.ts")).unwrap();

    let text = sandbox.run_in(&repo, &["impact", "--changed"]);
    assert_eq!(text.code, Some(0), "{text:?}");
    assert!(
        text.stdout
            .contains("\nlib/util.ts is deleted, so it is not traced\n"),
        "{text:?}"
    );
    let answer = json(&sandbox.run_in(&repo, &["--json", "impact", "--changed"]));
    assert_eq!(paths(&answer["deleted"]), ["lib/util.ts"]);
    assert_eq!(answer["changed"], value!([]));
}

#[scenario("cli", "A named path that is not on disk counts as deleted")]
#[test]
fn a_named_path_that_is_not_on_disk_counts_as_deleted() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    fs::remove_file(repo.join("lib/util.ts")).unwrap();

    let text = sandbox.run_in(&repo, &["impact", "lib/util.ts"]);
    assert_eq!(text.code, Some(0), "{text:?}");
    assert!(
        text.stdout
            .contains("\nlib/util.ts is deleted, so it is not traced\n"),
        "{text:?}"
    );
    let answer = json(&sandbox.run_in(&repo, &["--json", "impact", "lib/util.ts"]));
    assert_eq!(paths(&answer["deleted"]), ["lib/util.ts"]);
}

#[scenario("cli", "--changed outside git exits 2")]
#[test]
fn changed_outside_git_exits_2() {
    let sandbox = Sandbox::new();
    typescript_project(sandbox.work.path());
    for args in [
        &["impact", "--changed"][..],
        &["--no-refresh", "impact", "--changed"][..],
        &["--json", "impact", "--changed"][..],
    ] {
        let run = sandbox.run(args);
        assert_eq!(run.code, Some(2), "{args:?}: {run:?}");
        assert_eq!(run.stdout, "", "{args:?}");
        assert!(
            run.stderr.contains("`--changed` needs a git repository"),
            "{args:?}: {run:?}"
        );
        assert!(
            run.stderr.contains("name the files instead"),
            "{args:?}: {run:?}"
        );
    }
    assert!(is_empty(sandbox.cache.path()), "nothing was indexed");
}

#[test]
fn a_named_path_on_disk_is_traced_and_depth_limits_the_walk() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    // lib/util.ts is imported by app/a.ts, which app/c.ts imports.
    write(
        &repo,
        "app/c.ts",
        "import { a } from './a';\nexport const c = a;\n",
    );

    let everything = json(&sandbox.run_in(&repo, &["--json", "impact", "lib/util.ts"]));
    assert_eq!(paths(&everything["affected"]), ["app/a.ts", "app/c.ts"]);
    assert_eq!(everything["depth"], Value::Null);

    let one_hop =
        json(&sandbox.run_in(&repo, &["--json", "impact", "--depth", "1", "lib/util.ts"]));
    assert_eq!(paths(&one_hop["affected"]), ["app/a.ts"]);
    assert_eq!(one_hop["depth"], 1);
}

#[test]
fn names_go_to_the_query_for_every_path_and_a_walk_by_name_takes_one_hop_unless_a_depth_is_given() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();

    let by_name = json(&sandbox.run_in(
        &repo,
        &[
            "--json",
            "impact",
            "lib/util.ts",
            "app/b.ts",
            "--name",
            "util",
            "--name",
            "other",
        ],
    ));
    assert_eq!(by_name["changed"], value!(["lib/util.ts", "app/b.ts"]));
    assert_eq!(by_name["names"], value!(["util", "other"]));
    assert_eq!(by_name["depth"], 1);

    let deeper = json(&sandbox.run_in(
        &repo,
        &[
            "--json",
            "impact",
            "lib/util.ts",
            "--name",
            "util",
            "--depth",
            "3",
        ],
    ));
    assert_eq!(deeper["depth"], 3);

    let whole = json(&sandbox.run_in(&repo, &["--json", "impact", "lib/util.ts"]));
    assert!(whole.get("names").is_none(), "{whole}");
    assert_eq!(whole["depth"], Value::Null);
}

#[scenario("cli", "A blank --name exits 2")]
#[test]
fn a_blank_name_exits_2() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();

    for args in [
        &["impact", "lib/util.ts", "--name", ""][..],
        &["impact", "lib/util.ts", "--name", "  "][..],
        &[
            "--json",
            "impact",
            "lib/util.ts",
            "--name",
            "util",
            "--name",
            " ",
        ][..],
    ] {
        let run = sandbox.run_in(&repo, args);
        assert_eq!(run.code, Some(2), "{args:?}: {run:?}");
        assert_eq!(run.stdout, "", "{args:?}");
        assert!(
            run.stderr.contains("`--name` needs a name"),
            "{args:?}: {run:?}"
        );
        assert!(run.stderr.contains("--name getHash"), "{args:?}: {run:?}");
    }
    assert!(is_empty(sandbox.cache.path()), "nothing was indexed");
}

#[test]
fn a_blank_name_does_not_hide_the_argument_errors_that_came_first() {
    let sandbox = Sandbox::new();

    for (args, said) in [
        (
            &["impact", "--changed", "--name", ""][..],
            "`--name` needs paths",
        ),
        (
            &["impact", "--base", "main", "lib/util.ts", "--name", ""][..],
            "`--base` needs `--changed`",
        ),
    ] {
        let run = sandbox.run(args);
        assert_eq!(run.code, Some(2), "{args:?}: {run:?}");
        assert!(run.stderr.contains(said), "{args:?}: {run:?}");
    }
}

#[scenario("cli", "--name asks about a name")]
#[test]
fn name_asks_about_a_name() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "src/a.ts",
        "export function foo() {\n  return 1;\n}\nexport function bar() {\n  return 2;\n}\n",
    );
    // b.ts calls foo in run; c.ts imports the same file and calls only bar.
    write(
        &repo,
        "src/b.ts",
        "import { foo, bar } from './a';\nexport function run() {\n  return foo();\n}\nexport function stop() {\n  return bar();\n}\n",
    );
    write(
        &repo,
        "src/c.ts",
        "import { bar } from './a';\nexport const c = bar();\n",
    );

    let answer = json(&sandbox.run_in(&repo, &["--json", "impact", "src/a.ts", "--name", "foo"]));
    assert_eq!(answer["names"], value!(["foo"]));
    assert_eq!(paths(&answer["affected"]), ["src/b.ts"]);
    assert_eq!(
        answer["affected"][0]["callers"],
        value!([{ "name": "run", "line": 2 }])
    );

    let text = sandbox.run_in(&repo, &["impact", "src/a.ts", "--name", "foo"]);
    assert_eq!(text.code, Some(0), "{text:?}");
    assert!(
        text.stdout
            .starts_with("impact of foo in src/a.ts, within 1 hop\n"),
        "{text:?}"
    );
    assert!(
        text.stdout
            .contains("\nsrc/b.ts  distance 1  imports foo from src/a.ts; in run (line 2)\n"),
        "{text:?}"
    );

    // Asked about the whole file, c.ts is affected too, so it is the name that leaves it out.
    let whole = json(&sandbox.run_in(&repo, &["--json", "impact", "src/a.ts"]));
    assert_eq!(paths(&whole["affected"]), ["src/b.ts", "src/c.ts"]);
}

#[test]
fn a_path_the_index_does_not_hold_does_not_fail_the_others() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(&repo, "notes.txt", "no connector reads this\n");

    let run = sandbox.run_in(&repo, &["--json", "impact", "notes.txt", "lib/util.ts"]);
    let answer = json(&run);
    assert_eq!(answer["unknown"], value!(["notes.txt"]));
    assert_eq!(paths(&answer["affected"]), ["app/a.ts"]);
}

#[test]
fn nothing_changed_is_an_answer() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();

    let run = sandbox.run_in(&repo, &["impact", "--changed"]);
    assert_eq!(run.code, Some(0), "{run:?}");
    assert!(run.stdout.starts_with("impact of no files\n"), "{run:?}");
}

#[test]
fn base_compares_with_the_best_common_ancestor_of_head_and_the_revision() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    git(&repo, &["branch", "base"]);
    write(&repo, "app/c.ts", "export const c = 3;\n");
    commit_all(&repo);

    let from_head = json(&sandbox.run_in(&repo, &["--json", "impact", "--changed"]));
    assert_eq!(from_head["changed"], value!([]));
    let from_base =
        json(&sandbox.run_in(&repo, &["--json", "impact", "--changed", "--base", "base"]));
    assert_eq!(from_base["changed"], value!(["app/c.ts"]));
}

#[test]
fn a_base_that_is_no_revision_exits_2() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();

    let run = sandbox.run_in(&repo, &["impact", "--changed", "--base", "no-such-branch"]);
    assert_eq!(run.code, Some(2), "{run:?}");
    assert_eq!(run.stdout, "", "{run:?}");
    assert!(
        run.stderr
            .contains("`no-such-branch` is not a revision of this repository"),
        "{run:?}"
    );
    assert!(run.stderr.contains("--base"), "{run:?}");
}
