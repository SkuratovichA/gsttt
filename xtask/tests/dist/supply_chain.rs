//! `deny.toml`, run by `cargo deny` on a small project made in a temporary directory. The tests need `cargo-deny` installed, and fail without it.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

use crate::support::{printed, workspace_root, write};

/// A project with one dependency, `copyleft`, under `license`, and the `deny.toml` of this repository.
fn project(license: &str) -> TempDir {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    write(
        root,
        "Cargo.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\nlicense = \"MIT\"\n\n[dependencies]\ncopyleft = { path = \"copyleft\" }\n",
    );
    write(root, "src/main.rs", "fn main() {}\n");
    write(
        root,
        "copyleft/Cargo.toml",
        &format!(
            "[package]\nname = \"copyleft\"\nversion = \"0.1.0\"\nedition = \"2021\"\nlicense = \"{license}\"\n"
        ),
    );
    write(root, "copyleft/src/lib.rs", "");
    fs::copy(workspace_root().join("deny.toml"), root.join("deny.toml")).unwrap();
    project
}

/// `cargo deny check licenses` in `project`, which needs no advisory database and no network.
fn check_licenses(project: &Path) -> Output {
    Command::new("cargo")
        .args(["deny", "--offline", "check", "licenses"])
        .current_dir(project)
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .unwrap()
}

#[gsttt_scenario::scenario("distribution", "A copyleft dependency fails")]
#[test]
fn a_copyleft_dependency_fails() {
    let project = project("GPL-3.0-only");

    let output = check_licenses(project.path());

    let text = printed(&output);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("copyleft"), "the crate is not named: {text}");
    assert!(text.contains("GPL-3.0-only"), "{text}");
}

#[test]
fn a_permissive_dependency_passes() {
    let project = project("MIT");

    let output = check_licenses(project.path());

    assert!(output.status.success(), "{}", printed(&output));
}

#[test]
fn deny_toml_ignores_no_advisory_and_trusts_only_crates_io() {
    let text = fs::read_to_string(workspace_root().join("deny.toml")).unwrap();
    let deny: toml::Table = text.parse().unwrap();

    let ignored = deny["advisories"]["ignore"].as_array().unwrap();
    assert_eq!(ignored.len(), 0, "an advisory is ignored: {ignored:?}");
    let sources = deny["sources"].as_table().unwrap();
    assert_eq!(sources["unknown-registry"].as_str(), Some("deny"));
    assert_eq!(sources["unknown-git"].as_str(), Some("deny"));
    // The default allows crates.io alone; naming another registry or a git host would widen it.
    assert!(!sources.contains_key("allow-registry"));
    assert!(!sources.contains_key("allow-git"));
    let licenses = deny["licenses"]["allow"].as_array().unwrap();
    assert!(
        licenses
            .iter()
            .filter_map(toml::Value::as_str)
            .all(|license| !license.contains("GPL")),
        "{licenses:?}"
    );
}
