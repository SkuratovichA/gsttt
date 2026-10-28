//! The files the config is read from: what they may be and how large.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use gsttt_config::{ConfigError, INIT_PATH};
use gsttt_core::Setup;

use crate::tree::{Tree, evaluate_at, failure};

const MEBIBYTE: usize = 1024 * 1024;

/// A Lua comment of `size` bytes.
fn comment(size: usize) -> String {
    format!("--{}", "x".repeat(size - 2))
}

/// A named pipe at `path`, which nobody ever writes to.
fn make_pipe(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    // `mkfifo` is the one way to make it without unsafe code: the calls of `rustix` for it do not exist on macOS.
    let status = Command::new("mkfifo").arg(path).status().unwrap();
    assert!(status.success());
}

/// The evaluation of the tree, which fails the test instead of hanging it when something waits for a writer on a pipe.
fn evaluate_without_waiting(tree: &Tree) -> Result<Setup, ConfigError> {
    let root = tree.root().to_path_buf();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || sender.send(evaluate_at(&root)).ok());
    receiver
        .recv_timeout(Duration::from_secs(20))
        .expect("the evaluation waited for a writer on a pipe")
}

#[gsttt_scenario::scenario("config", "A config that links outside the repository is refused")]
#[test]
fn a_config_that_links_outside_the_repository_is_refused() {
    let outside = Tree::empty();
    outside.write("elsewhere.lua", "error('the file outside was read')");
    let tree = Tree::empty();
    fs::create_dir_all(tree.root().join(".gsttt")).unwrap();
    symlink(
        outside.root().join("elsewhere.lua"),
        tree.root().join(INIT_PATH),
    )
    .unwrap();
    assert_eq!(
        tree.evaluate().unwrap_err().to_string(),
        "cannot read .gsttt/init.lua: resolves outside the repository"
    );
}

#[test]
fn a_config_that_links_inside_the_repository_is_read() {
    let tree = Tree::empty();
    tree.write(
        "shared/gsttt.lua",
        r#"require("gsttt").setup { exclude = { "linked" } }"#,
    );
    fs::create_dir_all(tree.root().join(".gsttt")).unwrap();
    symlink("../shared/gsttt.lua", tree.root().join(INIT_PATH)).unwrap();
    assert_eq!(tree.evaluate().unwrap().exclude, ["linked"]);
}

#[test]
fn a_link_to_nothing_is_no_config() {
    let tree = Tree::empty();
    fs::create_dir_all(tree.root().join(".gsttt")).unwrap();
    symlink("missing.lua", tree.root().join(INIT_PATH)).unwrap();
    assert_eq!(tree.evaluate().unwrap(), Setup::default());
}

#[gsttt_scenario::scenario("config", "A pipe is not a config")]
#[test]
fn a_pipe_is_not_a_config() {
    let tree = Tree::empty();
    make_pipe(&tree.root().join(INIT_PATH));
    assert_eq!(
        evaluate_without_waiting(&tree).unwrap_err().to_string(),
        "cannot read .gsttt/init.lua: is not a regular file"
    );
}

#[test]
fn a_pipe_is_not_a_module() {
    let tree = Tree::with_init("require(\"pipe\")\n");
    make_pipe(&tree.root().join(".gsttt/lua/pipe.lua"));
    let message = evaluate_without_waiting(&tree).unwrap_err().to_string();
    assert!(
        message.starts_with(".gsttt/init.lua:1: cannot read module `pipe` at .gsttt/lua/pipe.lua:")
            && message.ends_with("is not a regular file"),
        "{message}"
    );
}

#[gsttt_scenario::scenario("config", "A huge config is refused")]
#[test]
fn a_config_over_a_mebibyte_is_refused() {
    // Exactly the limit is still a config.
    assert!(Tree::with_init(&comment(MEBIBYTE)).evaluate().is_ok());
    let message = failure(&comment(MEBIBYTE + 1));
    assert_eq!(message, "cannot read .gsttt/init.lua: is larger than 1 MiB");
}

#[test]
fn a_module_over_a_mebibyte_is_refused() {
    let tree = Tree::with_init("require(\"big\")\n");
    tree.write(".gsttt/lua/big.lua", &comment(MEBIBYTE + 1));
    let message = tree.evaluate().unwrap_err().to_string();
    assert!(
        message.contains("cannot read module `big` at .gsttt/lua/big.lua: is larger than 1 MiB"),
        "{message}"
    );
}

#[test]
fn a_config_that_is_not_text_is_refused() {
    let tree = Tree::empty();
    fs::create_dir_all(tree.root().join(".gsttt")).unwrap();
    fs::write(tree.root().join(INIT_PATH), [0xff, 0xfe, b'\n']).unwrap();
    assert_eq!(
        tree.evaluate().unwrap_err().to_string(),
        "cannot read .gsttt/init.lua: is not UTF-8 text"
    );
}
