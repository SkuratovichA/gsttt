//! A path argument is read from the directory the run starts in, and one that leaves the repository is a usage
//! error told before anything reads the index.

use std::path::PathBuf;

use gsttt_scenario::scenario;

use super::support::{Ran, Sandbox, init_repo, ran};

/// A repository `repo` with a directory `app` in it, inside the work directory of the sandbox. Returns the
/// canonical path of the repository and the path of `app`.
fn repo_with_app(sandbox: &Sandbox) -> (PathBuf, PathBuf) {
    let repo = sandbox.work.path().join("repo");
    let app = repo.join("app");
    std::fs::create_dir_all(&app).unwrap();
    init_repo(&repo);
    (repo.canonicalize().unwrap(), app)
}

/// Runs the binary in `dir` with `args`.
fn run_in(sandbox: &Sandbox, dir: &std::path::Path, args: &[&str]) -> Ran {
    ran(sandbox.gsttt().current_dir(dir).args(args))
}

fn assert_usage_error(run: &Ran, said: &str) {
    assert_eq!(run.code, Some(2), "{run:?}");
    assert_eq!(run.stdout, "", "{run:?}");
    assert!(run.stderr.contains(said), "{run:?} does not say {said:?}");
}

/// The path arguments are read before the command is, so the commands that are not built yet still judge them.
/// What they answer for a path inside the repository is proven where the commands answer.
fn assert_read(run: &Ran) {
    assert_ne!(run.code, Some(2), "{run:?}");
    assert!(!run.stderr.contains("outside the repository"), "{run:?}");
}

#[scenario("cli", "A path outside the root exits 2")]
#[test]
fn a_path_outside_the_root_exits_2() {
    let sandbox = Sandbox::new();
    let (_, app) = repo_with_app(&sandbox);
    let run = run_in(&sandbox, &app, &["dependents", "../../elsewhere.ts"]);
    assert_usage_error(&run, "`../../elsewhere.ts` is outside the repository");
}

#[test]
fn every_command_with_paths_reads_each_of_them() {
    let sandbox = Sandbox::new();
    let (_, app) = repo_with_app(&sandbox);
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
        let run = run_in(&sandbox, &app, &args);
        assert_usage_error(&run, "is outside the repository");
    }
}

#[test]
fn a_path_inside_the_root_is_read_from_the_current_directory_or_in_full() {
    let sandbox = Sandbox::new();
    let (repo, app) = repo_with_app(&sandbox);
    let absolute = repo.join("lib/util.ts");
    for argument in [
        "../lib/util.ts",
        "./x.ts",
        "gone.ts",
        absolute.to_str().unwrap(),
    ] {
        assert_read(&run_in(&sandbox, &app, &["dependents", argument]));
    }
}

#[test]
fn a_path_is_read_from_the_directory_c_names() {
    let sandbox = Sandbox::new();
    let (_, app) = repo_with_app(&sandbox);
    let app = app.to_str().unwrap();
    let from_work = |argument: &str| {
        // The sandbox starts the binary in its work directory, which holds the repository.
        sandbox.run(&["-C", app, "dependents", argument])
    };
    assert_read(&from_work("../lib/util.ts"));
    assert_usage_error(
        &from_work("../../elsewhere.ts"),
        "is outside the repository",
    );
}

#[test]
fn the_root_itself_is_no_path() {
    let sandbox = Sandbox::new();
    let (repo, app) = repo_with_app(&sandbox);
    for (dir, argument) in [(&repo, "."), (&app, ".."), (&app, repo.to_str().unwrap())] {
        let run = run_in(&sandbox, dir, &["dependents", argument]);
        assert_usage_error(&run, "is the root of the repository");
    }
}
