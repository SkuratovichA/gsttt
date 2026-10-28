//! `cargo xtask check`.

use std::path::Path;

use tempfile::TempDir;

use crate::support::{printed, workspace_root, write, xtask};

const PINNED: &str = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1";

fn workflow(reference: &str) -> String {
    format!(
        "name: ci\non: push\njobs:\n  checks:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: {reference}\n"
    )
}

fn plugin_manifest(skills: &str) -> String {
    format!(
        r#"{{
  "name": "p",
  "skills": ["{skills}"],
  "hooks": {{ "SessionStart": [{{ "hooks": [{{ "type": "command", "command": "${{CLAUDE_PLUGIN_ROOT}}/scripts/run.sh start" }}] }}] }}
}}"#
    )
}

/// A repository that passes the check: a pinned workflow and a plugin whose manifest names files that exist.
fn clean_repository() -> TempDir {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    write(root, ".github/workflows/ci.yml", &workflow(PINNED));
    write(
        root,
        ".claude-plugin/marketplace.json",
        r#"{ "name": "m", "owner": { "name": "o" }, "plugins": [{ "name": "p", "source": "./plugin" }] }"#,
    );
    write(
        root,
        "plugin/.claude-plugin/plugin.json",
        &plugin_manifest("./skills/p"),
    );
    write(root, "plugin/skills/p/SKILL.md", "---\nname: p\n---\n");
    write(root, "plugin/scripts/run.sh", "#!/bin/sh\n");
    repo
}

/// The problems `cargo xtask check` finds in `root`, or `None` when it passes.
fn problems(root: &Path) -> Option<String> {
    let output = xtask(root, &["check"]);
    (!output.status.success()).then(|| printed(&output))
}

#[test]
fn a_clean_repository_passes() {
    let repo = clean_repository();

    assert_eq!(problems(repo.path()), None);
}

#[gsttt_scenario::scenario("distribution", "An unpinned action fails the check")]
#[test]
fn an_unpinned_action_fails_the_check() {
    let repo = clean_repository();
    write(
        repo.path(),
        ".github/workflows/ci.yml",
        &workflow("actions/checkout@v4"),
    );

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains(".github/workflows/ci.yml:7:"),
        "{problems}"
    );
    assert!(problems.contains("actions/checkout@v4"), "{problems}");
}

#[gsttt_scenario::scenario("distribution", "A manifest naming a missing file fails the check")]
#[test]
fn a_manifest_naming_a_missing_file_fails_the_check() {
    let repo = clean_repository();
    write(
        repo.path(),
        "plugin/.claude-plugin/plugin.json",
        &plugin_manifest("./skills/gone/SKILL.md"),
    );

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains("plugin/.claude-plugin/plugin.json"),
        "{problems}"
    );
    assert!(problems.contains("skills/gone/SKILL.md"), "{problems}");
}

#[test]
fn a_hook_command_naming_a_missing_script_fails_the_check() {
    let repo = clean_repository();
    std::fs::remove_file(repo.path().join("plugin/scripts/run.sh")).unwrap();

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains("plugin/.claude-plugin/plugin.json"),
        "{problems}"
    );
    assert!(
        problems.contains("${CLAUDE_PLUGIN_ROOT}/scripts/run.sh"),
        "{problems}"
    );
}

#[test]
fn a_skill_without_a_skill_file_fails_the_check() {
    let repo = clean_repository();
    std::fs::remove_file(repo.path().join("plugin/skills/p/SKILL.md")).unwrap();

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains("the skill `./skills/p` has no SKILL.md"),
        "{problems}"
    );
}

#[test]
fn a_manifest_that_does_not_parse_fails_the_check() {
    let repo = clean_repository();
    write(
        repo.path(),
        "plugin/.claude-plugin/plugin.json",
        "{ not json",
    );

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains("plugin/.claude-plugin/plugin.json: is not valid JSON"),
        "{problems}"
    );
}

#[test]
fn a_repository_without_a_marketplace_fails_the_check() {
    let repo = clean_repository();
    std::fs::remove_file(repo.path().join(".claude-plugin/marketplace.json")).unwrap();

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains(".claude-plugin/marketplace.json: the file is missing"),
        "{problems}"
    );
}

#[test]
fn this_repository_passes_its_own_check() {
    let output = xtask(&workspace_root(), &["check"]);

    assert!(output.status.success(), "{}", printed(&output));
}
