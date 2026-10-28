//! A path argument is read from the directory the run starts in, and one that leaves the repository is a usage
//! error told before anything reads the index.

use gsttt_scenario::scenario;
use serde_json::Value;

use super::support::{Ran, Sandbox};

fn assert_usage_error(run: &Ran, said: &str) {
    assert_eq!(run.code, Some(2), "{run:?}");
    assert_eq!(run.stdout, "", "{run:?}");
    assert!(run.stderr.contains(said), "{run:?} does not say {said:?}");
}

/// The path an answer in JSON is about, which is how the tests see what a path argument was read as.
fn subject(run: &Ran) -> String {
    assert_eq!(run.code, Some(0), "{run:?}");
    let answer: Value = serde_json::from_str(&run.stdout).unwrap();
    answer["path"].as_str().unwrap().to_owned()
}

#[scenario("cli", "A path outside the root exits 2")]
#[test]
fn a_path_outside_the_root_exits_2() {
    let sandbox = Sandbox::new();
    let app = sandbox.repository().join("app");
    let run = sandbox.run_in(&app, &["dependents", "../../elsewhere.ts"]);
    assert_usage_error(&run, "`../../elsewhere.ts` is outside the repository");
}

#[test]
fn every_command_with_paths_reads_each_of_them() {
    let sandbox = Sandbox::new();
    let app = sandbox.repository().join("app");
    let outside = "../../elsewhere.ts";
    for args in [
        vec!["impact", "ok.ts", outside],
        vec!["dependents", outside],
        vec!["dependencies", outside],
        vec!["path", outside, "ok.ts"],
        vec!["path", "ok.ts", outside],
        vec!["--json", "dependents", outside],
        vec!["--no-refresh", "dependents", outside],
    ] {
        let run = sandbox.run_in(&app, &args);
        assert_usage_error(&run, "is outside the repository");
    }
}

#[scenario("cli", "A path is read from the current directory")]
#[test]
fn a_path_is_read_from_the_current_directory() {
    let sandbox = Sandbox::new();
    let app = sandbox.repository().join("app");
    let run = sandbox.run_in(&app, &["--json", "dependents", "../lib/util.ts"]);
    assert_eq!(subject(&run), "lib/util.ts");
    assert!(run.stdout.contains(r#""path":"app/a.ts""#), "{run:?}");
}

#[scenario("cli", "An absolute path inside the root is accepted")]
#[test]
fn an_absolute_path_inside_the_root_is_accepted() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    let absolute = repo.join("lib/util.ts");
    let run = sandbox.run_in(
        &repo.join("app"),
        &["--json", "dependents", absolute.to_str().unwrap()],
    );
    assert_eq!(subject(&run), "lib/util.ts");
    assert!(run.stdout.contains(r#""path":"app/a.ts""#), "{run:?}");
}

#[test]
fn a_path_is_read_from_the_directory_c_names() {
    let sandbox = Sandbox::new();
    let app = sandbox.repository().join("app");
    let app = app.to_str().unwrap();
    // The sandbox starts the binary in its work directory, which holds the repository and not `app`.
    let run = sandbox.run(&["-C", app, "--json", "dependents", "../lib/util.ts"]);
    assert_eq!(subject(&run), "lib/util.ts");
    let outside = sandbox.run(&["-C", app, "dependents", "../../elsewhere.ts"]);
    assert_usage_error(&outside, "is outside the repository");
}

#[test]
fn the_root_itself_is_no_path() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    let app = repo.join("app");
    for (dir, argument) in [(&repo, "."), (&app, ".."), (&app, repo.to_str().unwrap())] {
        let run = sandbox.run_in(dir, &["dependents", argument]);
        assert_usage_error(&run, "is the root of the repository");
    }
}
