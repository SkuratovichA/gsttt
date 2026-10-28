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

/// The root is a canonical path, and a path spelled through a link to a directory of the repository names a file in
/// it all the same: `/tmp` is a link on macOS, and so is the directory the tests are built in.
#[scenario("cli", "A path spelled through a linked directory is accepted")]
#[test]
#[cfg(unix)]
fn a_path_spelled_through_a_linked_directory_is_accepted() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    let link = sandbox.home.path().join("link");
    std::os::unix::fs::symlink(&repo, &link).unwrap();
    let through = |path: &str| format!("{}/{path}", link.display());

    // The directory `-C` names is read through the link, and so is the file named in full.
    let with_c = sandbox.run(&[
        "-C",
        link.to_str().unwrap(),
        "--json",
        "dependents",
        &through("lib/util.ts"),
    ]);
    assert_eq!(subject(&with_c), "lib/util.ts");

    let absolute = sandbox.run_in(
        &repo.join("app"),
        &["--json", "dependents", &through("lib/util.ts")],
    );
    assert_eq!(subject(&absolute), "lib/util.ts");

    // A file that is gone is still named by the part of its path that is there.
    let gone = sandbox.run_in(&repo, &["--json", "impact", &through("lib/gone.ts")]);
    let answer: Value = serde_json::from_str(&gone.stdout).unwrap();
    assert_eq!(answer["deleted"][0]["path"], "lib/gone.ts", "{gone:?}");

    // A link to a directory outside the repository does not lead inside it.
    let elsewhere = sandbox.home.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let out = sandbox.home.path().join("out");
    std::os::unix::fs::symlink(&elsewhere, &out).unwrap();
    let outside = sandbox.run_in(&repo, &["dependents", &format!("{}/x.ts", out.display())]);
    assert_usage_error(&outside, "is outside the repository");

    // Nor does the link itself, which is the root and no file in it.
    let root = sandbox.run_in(&repo, &["dependents", link.to_str().unwrap()]);
    assert_usage_error(&root, "is the root of the repository");
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
