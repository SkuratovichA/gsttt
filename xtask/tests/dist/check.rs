//! `cargo xtask check`.

use std::path::Path;

use tempfile::TempDir;

use crate::support::{git, printed, set_mode, workspace_root, write, write_executable, xtask};

const PINNED: &str = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1";

/// A workflow whose only job has `steps`, written as YAML indented for the list of steps. The first step is on line 7.
fn workflow_with_steps(steps: &str) -> String {
    format!("name: ci\non: push\njobs:\n  checks:\n    runs-on: ubuntu-latest\n    steps:\n{steps}")
}

/// A workflow with one step, `uses: <reference>`.
fn workflow(reference: &str) -> String {
    workflow_with_steps(&format!("      - uses: {reference}\n"))
}

const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

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
    git(root, &["init", "--quiet", "--initial-branch=main"]);
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
    write_executable(root, "plugin/scripts/run.sh", "#!/bin/sh\n");
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

#[gsttt_scenario::scenario(
    "distribution",
    "An unpinned action in a composite action fails the check"
)]
#[test]
fn an_unpinned_action_in_a_composite_action_fails_the_check() {
    let repo = clean_repository();
    write(
        repo.path(),
        ".github/actions/setup/action.yml",
        "name: setup\nruns:\n  using: composite\n  steps:\n    - uses: actions/cache@v4\n",
    );

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains(".github/actions/setup/action.yml:5:"),
        "{problems}"
    );
    assert!(problems.contains("actions/cache@v4"), "{problems}");
}

#[test]
fn a_composite_action_pinned_by_a_commit_id_passes() {
    let repo = clean_repository();
    write(
        repo.path(),
        ".github/actions/setup/action.yaml",
        &format!("name: setup\nruns:\n  using: composite\n  steps:\n    - uses: {PINNED}\n"),
    );

    assert_eq!(problems(repo.path()), None);
}

#[gsttt_scenario::scenario("distribution", "A Docker image pinned by its digest passes the check")]
#[test]
fn a_docker_image_pinned_by_its_digest_passes_the_check() {
    let repo = clean_repository();
    write(
        repo.path(),
        ".github/workflows/ci.yml",
        &workflow(&format!("docker://alpine@{DIGEST}")),
    );

    assert_eq!(problems(repo.path()), None);
}

#[test]
fn a_docker_image_pinned_by_a_tag_fails_the_check() {
    let repo = clean_repository();
    write(
        repo.path(),
        ".github/workflows/ci.yml",
        &workflow("docker://alpine:3.19"),
    );

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains(".github/workflows/ci.yml:7:"),
        "{problems}"
    );
    assert!(problems.contains("docker://alpine:3.19"), "{problems}");
}

#[gsttt_scenario::scenario("distribution", "A uses line in a script is not an action")]
#[test]
fn a_uses_line_in_a_script_is_not_an_action() {
    let repo = clean_repository();
    write(
        repo.path(),
        ".github/workflows/ci.yml",
        &workflow_with_steps(&format!(
            "      - uses: {PINNED}\n      - run: |\n          cat <<'EOF'\n          uses: actions/checkout@v4\n          EOF\n"
        )),
    );

    assert_eq!(problems(repo.path()), None);
}

#[test]
fn a_step_written_as_a_flow_mapping_is_checked() {
    let repo = clean_repository();
    write(
        repo.path(),
        ".github/workflows/ci.yml",
        &workflow_with_steps("      - { name: checkout, uses: actions/checkout@v4 }\n"),
    );

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains(".github/workflows/ci.yml:7:"),
        "{problems}"
    );
}

#[test]
fn a_quoted_key_is_checked() {
    let repo = clean_repository();
    write(
        repo.path(),
        ".github/workflows/ci.yml",
        &workflow_with_steps("      - \"uses\": actions/checkout@v4\n"),
    );

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains(".github/workflows/ci.yml:7:"),
        "{problems}"
    );
}

#[test]
fn an_unpinned_action_is_reported_even_when_its_line_cannot_be_found() {
    let repo = clean_repository();
    // The value is on the line after its key, which the scan of lines does not follow.
    write(
        repo.path(),
        ".github/workflows/ci.yml",
        &workflow_with_steps("      - uses:\n          actions/checkout@v4\n"),
    );

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains(".github/workflows/ci.yml: `uses: actions/checkout@v4` is not pinned"),
        "{problems}"
    );
}

#[gsttt_scenario::scenario("distribution", "A repository without workflows fails the check")]
#[test]
fn a_repository_without_workflows_fails_the_check() {
    let repo = clean_repository();
    std::fs::remove_dir_all(repo.path().join(".github")).unwrap();

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains(".github/workflows: the directory is missing"),
        "{problems}"
    );
}

/// A directory of a repository that nobody can list while this lives. Dropping it gives the directory its mode back, so that the repository can be removed.
struct Unlisted<'a> {
    root: &'a Path,
    relative: &'a str,
}

impl<'a> Unlisted<'a> {
    /// `None` when the directory can be listed all the same, as it is for the superuser: then there is no failure to look at.
    fn new(root: &'a Path, relative: &'a str) -> Option<Self> {
        set_mode(root, relative, 0o000);
        let unlisted = Self { root, relative };
        std::fs::read_dir(root.join(relative))
            .is_err()
            .then_some(unlisted)
    }
}

impl Drop for Unlisted<'_> {
    fn drop(&mut self) {
        set_mode(self.root, self.relative, 0o755);
    }
}

#[test]
fn a_workflows_directory_that_cannot_be_listed_is_named_in_the_error() {
    let repo = clean_repository();
    let Some(_unlisted) = Unlisted::new(repo.path(), ".github/workflows") else {
        return;
    };

    let output = xtask(repo.path(), &["check"]);

    let printed = printed(&output);
    assert!(!output.status.success(), "{printed}");
    assert!(printed.contains("listing the workflows"), "{printed}");
    assert!(
        printed.contains(".github/workflows: cannot read"),
        "{printed}"
    );
}

#[test]
fn a_composite_actions_directory_that_cannot_be_listed_is_named_in_the_error() {
    let repo = clean_repository();
    write(
        repo.path(),
        ".github/actions/setup/action.yml",
        "name: setup\nruns:\n  using: composite\n  steps: []\n",
    );
    let Some(_unlisted) = Unlisted::new(repo.path(), ".github/actions") else {
        return;
    };

    let output = xtask(repo.path(), &["check"]);

    let printed = printed(&output);
    assert!(!output.status.success(), "{printed}");
    assert!(
        printed.contains("listing the composite actions"),
        "{printed}"
    );
    assert!(
        printed.contains(".github/actions: cannot read"),
        "{printed}"
    );
}

#[test]
fn a_workflow_that_is_not_yaml_fails_the_check() {
    let repo = clean_repository();
    write(repo.path(), ".github/workflows/ci.yml", "jobs: [unclosed\n");

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains(".github/workflows/ci.yml: is not valid YAML"),
        "{problems}"
    );
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

#[gsttt_scenario::scenario("distribution", "A script that is not executable fails the check")]
#[test]
fn a_script_that_is_not_executable_fails_the_check() {
    let repo = clean_repository();
    set_mode(repo.path(), "plugin/scripts/run.sh", 0o644);

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains("plugin/.claude-plugin/plugin.json"),
        "{problems}"
    );
    assert!(
        problems.contains("`${CLAUDE_PLUGIN_ROOT}/scripts/run.sh`, which is not executable"),
        "{problems}"
    );
}

#[test]
fn a_server_command_that_is_not_executable_fails_the_check() {
    let repo = clean_repository();
    write(
        repo.path(),
        "plugin/.claude-plugin/plugin.json",
        r#"{ "name": "p", "mcpServers": { "s": { "command": "${CLAUDE_PLUGIN_ROOT}/scripts/run.sh", "args": ["mcp"] } } }"#,
    );
    set_mode(repo.path(), "plugin/scripts/run.sh", 0o600);

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains("`${CLAUDE_PLUGIN_ROOT}/scripts/run.sh`, which is not executable"),
        "{problems}"
    );
}

#[test]
fn a_file_that_a_command_reads_needs_no_executable_bit() {
    let repo = clean_repository();
    write(repo.path(), "plugin/config.json", "{}\n");
    write(
        repo.path(),
        "plugin/.claude-plugin/plugin.json",
        r#"{ "name": "p", "mcpServers": { "s": { "command": "sh", "args": ["${CLAUDE_PLUGIN_ROOT}/scripts/run.sh", "${CLAUDE_PLUGIN_ROOT}/config.json"] } } }"#,
    );
    set_mode(repo.path(), "plugin/scripts/run.sh", 0o644);

    assert_eq!(problems(repo.path()), None);
}

#[gsttt_scenario::scenario("distribution", "A manifest path must start with ./")]
#[test]
fn a_path_without_dot_slash_fails_the_check() {
    let repo = clean_repository();
    write(
        repo.path(),
        "plugin/.claude-plugin/plugin.json",
        &plugin_manifest("skills/p"),
    );

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains(
            "plugin/.claude-plugin/plugin.json: `skills` names `skills/p`, but a path of the manifest must start with `./`"
        ),
        "{problems}"
    );
}

#[test]
fn an_absolute_path_fails_the_check_even_when_it_exists() {
    let repo = clean_repository();
    let skill = repo.path().join("plugin/skills/p");
    write(
        repo.path(),
        "plugin/.claude-plugin/plugin.json",
        &plugin_manifest(&skill.display().to_string()),
    );

    let problems = problems(repo.path()).expect("the check should fail");

    assert!(
        problems.contains("but a path of the manifest must start with `./`"),
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
