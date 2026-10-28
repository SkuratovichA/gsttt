//! The files the config is read from: what they may be and how large.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use gsttt_config::{CONFIG_PATH, ConfigError};
use gsttt_core::Setup;

use crate::tree::{Tree, evaluate_at, failure};

const MEBIBYTE: usize = 1024 * 1024;

/// A TOML comment of `size` bytes.
fn comment(size: usize) -> String {
    format!("#{}", "x".repeat(size - 1))
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
    // Read, it would fail with a syntax error, which is not the message asserted below.
    outside.write("elsewhere.toml", "the file outside was read");
    let tree = Tree::empty();
    fs::create_dir_all(tree.root().join(".gsttt")).unwrap();
    symlink(
        outside.root().join("elsewhere.toml"),
        tree.root().join(CONFIG_PATH),
    )
    .unwrap();
    assert_eq!(
        tree.evaluate().unwrap_err().to_string(),
        "cannot read .gsttt/config.toml: resolves outside the repository"
    );
}

#[test]
fn a_config_that_links_inside_the_repository_is_read() {
    let tree = Tree::empty();
    tree.write("shared/gsttt.toml", "exclude = [\"linked\"]\n");
    fs::create_dir_all(tree.root().join(".gsttt")).unwrap();
    symlink("../shared/gsttt.toml", tree.root().join(CONFIG_PATH)).unwrap();
    assert_eq!(tree.evaluate().unwrap().exclude, ["linked"]);
}

#[gsttt_scenario::scenario("config", "A config that links to nothing is no config")]
#[test]
fn a_link_to_nothing_is_no_config() {
    let tree = Tree::empty();
    fs::create_dir_all(tree.root().join(".gsttt")).unwrap();
    symlink("missing.toml", tree.root().join(CONFIG_PATH)).unwrap();
    assert_eq!(tree.evaluate().unwrap(), Setup::default());
}

#[gsttt_scenario::scenario("config", "A pipe is not a config")]
#[test]
fn a_pipe_is_not_a_config() {
    let tree = Tree::empty();
    make_pipe(&tree.root().join(CONFIG_PATH));
    assert_eq!(
        evaluate_without_waiting(&tree).unwrap_err().to_string(),
        "cannot read .gsttt/config.toml: is not a regular file"
    );
}

#[test]
fn a_pipe_is_not_a_pack() {
    let tree = Tree::empty();
    make_pipe(&tree.root().join(".gsttt/packs/pipe.toml"));
    assert_eq!(
        evaluate_without_waiting(&tree).unwrap_err().to_string(),
        "cannot read .gsttt/packs/pipe.toml: is not a regular file"
    );
}

#[gsttt_scenario::scenario("config", "A huge config is refused")]
#[test]
fn a_config_over_a_mebibyte_is_refused() {
    // Exactly the limit is still a config.
    assert!(Tree::with_config(&comment(MEBIBYTE)).evaluate().is_ok());
    let message = failure(&comment(MEBIBYTE + 1));
    assert_eq!(
        message,
        "cannot read .gsttt/config.toml: is larger than 1 MiB"
    );
}

#[test]
fn a_pack_over_a_mebibyte_is_refused() {
    let tree = Tree::empty();
    tree.write(".gsttt/packs/big.toml", &comment(MEBIBYTE + 1));
    assert_eq!(
        tree.evaluate().unwrap_err().to_string(),
        "cannot read .gsttt/packs/big.toml: is larger than 1 MiB"
    );
}

#[test]
fn a_config_that_is_not_text_is_refused() {
    let tree = Tree::empty();
    fs::create_dir_all(tree.root().join(".gsttt")).unwrap();
    fs::write(tree.root().join(CONFIG_PATH), [0xff, 0xfe, b'\n']).unwrap();
    assert_eq!(
        tree.evaluate().unwrap_err().to_string(),
        "cannot read .gsttt/config.toml: is not UTF-8 text"
    );
}

#[gsttt_scenario::scenario("config", "A pack that links outside its folder is refused")]
#[test]
fn a_pack_that_links_outside_its_folder_is_refused() {
    // The config is inside the repository, so only the folder of the packs says no.
    let tree = Tree::with_config("exclude = [\"a\"]\n");
    fs::create_dir_all(tree.root().join(".gsttt/packs")).unwrap();
    symlink("../config.toml", tree.root().join(".gsttt/packs/x.toml")).unwrap();
    assert_eq!(
        tree.evaluate().unwrap_err().to_string(),
        "cannot read .gsttt/packs/x.toml: resolves outside .gsttt/packs"
    );
}

#[gsttt_scenario::scenario("config", "A pack that links to nothing is refused")]
#[test]
fn a_pack_that_links_to_nothing_is_refused() {
    let tree = Tree::empty();
    fs::create_dir_all(tree.root().join(".gsttt/packs")).unwrap();
    symlink("missing.toml", tree.root().join(".gsttt/packs/x.toml")).unwrap();
    assert_eq!(
        tree.evaluate().unwrap_err().to_string(),
        "cannot read .gsttt/packs/x.toml: is a link that leads nowhere"
    );
}

#[gsttt_scenario::scenario("config", "A pack folder that links outside the repository is refused")]
#[test]
fn a_pack_folder_that_links_outside_the_repository_is_refused() {
    let outside = Tree::empty();
    // Read, it would fail with a syntax error, which is not the message asserted below.
    outside.write("packs/x.toml", "the file outside was read");
    let tree = Tree::empty();
    fs::create_dir_all(tree.root().join(".gsttt")).unwrap();
    symlink(
        outside.root().join("packs"),
        tree.root().join(".gsttt/packs"),
    )
    .unwrap();
    assert_eq!(
        tree.evaluate().unwrap_err().to_string(),
        "cannot read .gsttt/packs/x.toml: resolves outside the repository"
    );
}

#[test]
fn a_pack_folder_that_links_inside_the_repository_is_read() {
    let tree = Tree::empty();
    tree.write(
        "shared/packs/ktor.toml",
        "name = \"ktor\"\nlanguage = \"rust\"\n",
    );
    fs::create_dir_all(tree.root().join(".gsttt")).unwrap();
    symlink("../shared/packs", tree.root().join(".gsttt/packs")).unwrap();
    let setup = tree.evaluate().unwrap();
    assert_eq!(setup.packs.len(), 1);
}
