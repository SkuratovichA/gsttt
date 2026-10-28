//! The root of a run: where `-C` and the current directory lead, and which directories are refused.

use std::fs;

use gsttt_scenario::scenario;

use super::support::{Sandbox, init_repo, is_empty, listing, ran, typescript_project};

#[scenario("cli", "A missing -C directory exits 2")]
#[test]
fn a_missing_c_directory_exits_2() {
    let sandbox = Sandbox::new();
    for args in [
        &["-C", "does/not/exist", "status"][..],
        &["status", "-C", "does/not/exist"][..],
    ] {
        let ran = sandbox.run(args);
        assert_eq!(ran.code, Some(2), "{args:?}");
        assert_eq!(ran.stdout, "", "{args:?}");
        assert!(ran.stderr.contains("`does/not/exist`"), "{}", ran.stderr);
        assert!(ran.stderr.contains("does not exist"), "{}", ran.stderr);
    }
}

#[test]
fn c_names_a_directory_and_not_a_file() {
    let sandbox = Sandbox::new();
    fs::write(sandbox.work.path().join("a.ts"), "").unwrap();
    let ran = sandbox.run(&["-C", "a.ts", "status"]);
    assert_eq!(ran.code, Some(2));
    assert_eq!(ran.stdout, "");
    assert!(ran.stderr.contains("`a.ts`"), "{}", ran.stderr);
    assert!(ran.stderr.contains("is not a directory"), "{}", ran.stderr);
}

#[scenario("cli", "The home directory is refused")]
#[test]
fn the_home_directory_is_refused() {
    let sandbox = Sandbox::new();
    let mut command = sandbox.gsttt();
    command.current_dir(sandbox.home.path()).arg("index");
    let ran = ran(&mut command);
    assert_eq!(ran.code, Some(2));
    assert_eq!(ran.stdout, "");
    assert!(
        ran.stderr.contains("does not index the home directory"),
        "{}",
        ran.stderr
    );
    assert!(ran.stderr.contains("inside a project"), "{}", ran.stderr);
    for written in [&sandbox.home, &sandbox.cache, &sandbox.state] {
        assert!(is_empty(written.path()), "{:?} was written", written.path());
    }
}

#[test]
fn the_home_directory_is_refused_whichever_way_it_is_named() {
    let sandbox = Sandbox::new();
    let home = sandbox.home.path();
    let mut named = vec![home.to_path_buf(), home.join(".")];
    #[cfg(unix)]
    {
        let link = sandbox.work.path().join("home-link");
        std::os::unix::fs::symlink(home, &link).unwrap();
        named.push(link);
    }
    for directory in named {
        let ran = sandbox.run(&["-C", directory.to_str().unwrap(), "index"]);
        assert_eq!(ran.code, Some(2), "{directory:?}");
        assert!(
            ran.stderr.contains("does not index the home directory"),
            "{directory:?}: {}",
            ran.stderr
        );
    }
}

#[test]
fn a_repository_whose_root_is_the_home_directory_is_refused_from_below() {
    let sandbox = Sandbox::new();
    init_repo(sandbox.home.path());
    let src = sandbox.home.path().join("src");
    fs::create_dir(&src).unwrap();
    let ran = sandbox.run(&["-C", src.to_str().unwrap(), "index"]);
    assert_eq!(ran.code, Some(2));
    assert!(
        ran.stderr.contains("does not index the home directory"),
        "{}",
        ran.stderr
    );
}

#[test]
fn a_directory_inside_the_home_directory_is_not_refused() {
    let sandbox = Sandbox::new();
    let project = sandbox.home.path().join("project");
    fs::create_dir(&project).unwrap();
    let ran = sandbox.run(&["-C", project.to_str().unwrap(), "--no-refresh", "status"]);
    assert_eq!(ran.code, Some(1));
    let root = project.canonicalize().unwrap();
    assert!(
        ran.stderr.contains(&format!("`{}`", root.display())),
        "{}",
        ran.stderr
    );
}

#[test]
fn the_filesystem_root_is_refused() {
    let sandbox = Sandbox::new();
    let ran = sandbox.run(&["-C", "/", "index"]);
    assert_eq!(ran.code, Some(2));
    assert_eq!(ran.stdout, "");
    assert!(ran.stderr.contains("does not index `/`"), "{}", ran.stderr);
    assert!(is_empty(sandbox.cache.path()));
}

#[scenario("cli", "-C works from another directory")]
#[test]
fn c_works_from_another_directory() {
    let sandbox = Sandbox::new();
    let repo = sandbox.home.path().join("repo");
    let src = repo.join("src");
    fs::create_dir_all(&src).unwrap();
    init_repo(&repo);
    typescript_project(&src);
    let before = listing(&src);

    // The work directory of the sandbox is outside the repository.
    let ran_index = sandbox.run(&["-C", src.to_str().unwrap(), "index"]);
    assert_eq!(ran_index.code, Some(0), "{ran_index:?}");
    assert!(
        ran_index.stdout.starts_with("Indexed 3 files"),
        "{ran_index:?}"
    );

    // The snapshot belongs to the repository and not to `src`: `status` never builds one, and from the root of the
    // repository it finds this one.
    assert_eq!(listing(&src), before, "`src` gained a file");
    let inside_git = listing(&repo.join(".git"));
    assert!(
        inside_git
            .iter()
            .any(|path| path.ends_with("snapshot.gsttt")),
        "{inside_git:?}"
    );
    let status = ran(sandbox.gsttt().current_dir(&repo).arg("status"));
    assert_eq!(status.code, Some(0), "{status:?}");
    assert!(status.stdout.contains("3 files"), "{status:?}");
}

#[scenario("cli", "A directory outside git is indexed")]
#[test]
fn a_directory_outside_git_is_indexed() {
    let sandbox = Sandbox::new();
    typescript_project(sandbox.work.path());
    let before = listing(sandbox.work.path());

    let ran = sandbox.run(&["index"]);
    assert_eq!(ran.code, Some(0), "{ran:?}");
    assert!(ran.stdout.starts_with("Indexed 3 files"), "{ran:?}");

    let kept = listing(sandbox.cache.path());
    assert!(
        kept.iter().any(|path| path.ends_with("snapshot.gsttt")),
        "the snapshot is not in the cache: {kept:?}"
    );
    assert_eq!(
        listing(sandbox.work.path()),
        before,
        "the directory gained a file"
    );
}

#[scenario("cli", "A failure leaves stdout empty")]
#[test]
fn a_failure_leaves_stdout_empty() {
    let sandbox = Sandbox::new();
    let ran = sandbox.run(&["--json", "--no-refresh", "dependents", "src/a.ts"]);
    assert_eq!(ran.code, Some(1));
    assert_eq!(ran.stdout, "");
    assert!(ran.stderr.contains("no index yet"), "{}", ran.stderr);
    assert!(ran.stderr.contains("`gsttt index`"), "{}", ran.stderr);
}

#[test]
fn every_question_and_status_need_an_index_to_read() {
    let sandbox = Sandbox::new();
    for args in [
        &["status"][..],
        &["--no-refresh", "impact", "src/a.ts"][..],
        &["--no-refresh", "dependents", "src/a.ts"][..],
        &["--no-refresh", "dependencies", "src/a.ts"][..],
        &["--no-refresh", "path", "src/a.ts", "src/b.ts"][..],
        &["--no-refresh", "contract", "http", "GET"][..],
        &["--no-refresh", "coverage"][..],
    ] {
        let ran = sandbox.run(args);
        assert_eq!(ran.code, Some(1), "{args:?}");
        assert_eq!(ran.stdout, "", "{args:?}");
        assert!(
            ran.stderr.contains("no index yet"),
            "{args:?}: {}",
            ran.stderr
        );
    }
}
