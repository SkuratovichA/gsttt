//! `gsttt hook session-start` reads the header of the snapshot and prints one line, or nothing.

use std::path::Path;

use camino::Utf8PathBuf;
use gsttt::hook::session_line;
use gsttt_core::Setup;
use gsttt_core::engine::refresh::Engine;
use gsttt_core::engine::snapshot::{self, Header};
use gsttt_core::engine::workspace::Workspace;
use gsttt_core::text::noun;
use gsttt_scenario::scenario;
use jiff::Timestamp;

use super::support::{Ran, Sandbox, commit_all, init_repo, is_empty, ran};

const HOOK: [&str; 2] = ["hook", "session-start"];

/// The workspace of `tree`, with its state under `cache` when it is in no repository, which is where the binary
/// looks when its `XDG_CACHE_HOME` is `cache`.
fn workspace(tree: &Path, cache: &Path) -> Workspace {
    let utf8 = |path: &Path| Utf8PathBuf::from_path_buf(path.to_owned()).unwrap();
    Workspace::locate_with(&utf8(tree), Some(utf8(cache))).unwrap()
}

/// Leaves a snapshot of `tree` where the binary will find it, as an index with no connector does.
fn index(workspace: &Workspace) {
    Engine::new(workspace.clone(), &[], &[], &Setup::default())
        .refresh()
        .unwrap();
}

fn assert_silent(hook: &Ran) {
    assert_eq!(hook.code, Some(0), "{hook:?}");
    assert_eq!(hook.stdout, "", "{hook:?}");
    assert_eq!(hook.stderr, "", "{hook:?}");
}

#[scenario("cli", "The hook prints one line from the snapshot header")]
#[test]
fn the_line_comes_from_the_header() {
    let header = Header {
        built_by: "0.1.0".to_owned(),
        built_at: "2026-10-28T10:00:00Z".to_owned(),
        head: Some("a1b2c3d4e5f60718293a4b5c6d7e8f9012345678".to_owned()),
        dirty: 3,
        files: 4213,
        connectors: vec!["ts".to_owned()],
        coverage: String::new(),
    };
    let two_hours_later: Timestamp = "2026-10-28T12:00:00Z".parse().unwrap();
    assert_eq!(
        session_line(&header, two_hours_later).as_deref(),
        Some(
            "gsttt: 4,213 files indexed at a1b2c3d (3 dirty), 2 h ago; use the gsttt tools for impact before \
             changing shared code"
        )
    );
}

#[scenario("cli", "The hook is silent without an index")]
#[test]
fn the_hook_is_silent_without_an_index() {
    let sandbox = Sandbox::new();
    assert_silent(&sandbox.run(&HOOK));
    assert!(
        is_empty(sandbox.cache.path()) && is_empty(sandbox.state.path()),
        "the hook writes nothing"
    );
}

#[test]
fn the_hook_is_silent_whatever_else_goes_wrong() {
    let sandbox = Sandbox::new();
    assert_silent(&sandbox.run(&["-C", "does/not/exist", "hook", "session-start"]));
    assert_silent(&sandbox.run(&[
        "-C",
        sandbox.home.path().to_str().unwrap(),
        "hook",
        "session-start",
    ]));

    let workspace = workspace(sandbox.work.path(), sandbox.cache.path());
    index(&workspace);
    std::fs::write(workspace.snapshot_path(), b"not a snapshot").unwrap();
    assert_silent(&sandbox.run(&HOOK));
}

#[scenario("cli", "The hook does not read the config")]
#[test]
fn the_hook_does_not_read_the_config() {
    let sandbox = Sandbox::new();
    let tree = sandbox.work.path();
    init_repo(tree);
    std::fs::write(tree.join("a.txt"), "a\n").unwrap();
    std::fs::create_dir(tree.join(".gsttt")).unwrap();
    std::fs::write(tree.join(".gsttt/init.lua"), "g.ts {{{ this is not lua\n").unwrap();
    index(&workspace(tree, sandbox.cache.path()));

    let hook = sandbox.run(&HOOK);
    assert_eq!(hook.code, Some(0), "{hook:?}");
    assert!(hook.stdout.starts_with("gsttt: "), "{hook:?}");
    assert_eq!(hook.stderr, "", "{hook:?}");
}

#[test]
fn the_hook_prints_one_line_about_the_stored_index() {
    let sandbox = Sandbox::new();
    let tree = sandbox.work.path();
    init_repo(tree);
    std::fs::write(tree.join("a.txt"), "a\n").unwrap();
    let head = commit_all(tree);
    let workspace = workspace(tree, sandbox.cache.path());
    index(&workspace);
    let header = snapshot::peek(workspace.snapshot_path().as_std_path()).unwrap();

    let hook = sandbox.run(&HOOK);
    assert_eq!(hook.code, Some(0), "{hook:?}");
    assert_eq!(hook.stderr, "", "{hook:?}");
    let line = hook.stdout.strip_suffix('\n').unwrap();
    assert!(!line.contains('\n'), "{hook:?}");
    // The age depends on the clock, so the line is checked around it.
    let start = format!(
        "gsttt: {} indexed at {}, ",
        noun(header.files, "file"),
        head.get(..7).unwrap()
    );
    assert!(
        line.starts_with(&start),
        "{line:?} does not start with {start:?}"
    );
    assert!(
        line.ends_with("; use the gsttt tools for impact before changing shared code"),
        "{line:?}"
    );
}

#[cfg(unix)]
#[test]
fn the_hook_starts_no_git_process() {
    use std::os::unix::fs::PermissionsExt;

    let sandbox = Sandbox::new();
    let tree = sandbox.work.path();
    init_repo(tree);
    std::fs::write(tree.join("a.txt"), "a\n").unwrap();
    commit_all(tree);
    index(&workspace(tree, sandbox.cache.path()));

    // The only program on the PATH is a `git` that leaves a file behind when it is started.
    let shim = tempfile::tempdir().unwrap();
    let marker = shim.path().join("git-was-started");
    let script = shim.path().join("git");
    std::fs::write(
        &script,
        format!("#!/bin/sh\n: > '{}'\nexit 1\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut command = sandbox.gsttt();
    command.env("PATH", shim.path());

    let mut git = std::process::Command::new("git");
    git.env("PATH", shim.path());
    assert!(!git.status().unwrap().success());
    assert!(marker.exists(), "the shim records that it was started");
    std::fs::remove_file(&marker).unwrap();

    let hook = ran(command.args(HOOK));
    assert!(hook.stdout.starts_with("gsttt: "), "{hook:?}");
    assert!(!marker.exists(), "the hook started git");
}
