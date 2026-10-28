//! The Claude Code plugin in `plugin/` and the marketplace that lists it.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

use crate::support::{printed, workspace_root};

fn json_file(relative: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(workspace_root().join(relative)).unwrap()).unwrap()
}

fn wrapper() -> PathBuf {
    workspace_root().join("plugin/scripts/gsttt.sh")
}

/// The command of the session-start hook in the manifest of the plugin.
fn session_start_command() -> String {
    json_file("plugin/.claude-plugin/plugin.json")
        .pointer("/hooks/SessionStart/0/hooks/0/command")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned()
}

/// Runs a hook `command` as Claude Code does, through a shell, with `CLAUDE_PLUGIN_ROOT` set and `path` as the whole of `PATH`.
fn run_hook(command: &str, path: &Path) -> Output {
    Command::new("/bin/sh")
        .args(["-c", command])
        .env("CLAUDE_PLUGIN_ROOT", workspace_root().join("plugin"))
        .env("PATH", path)
        .output()
        .unwrap()
}

/// A directory that holds a `gsttt` which prints the arguments it was given.
fn directory_with_gsttt() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("gsttt");
    fs::write(&binary, "#!/bin/sh\necho \"gsttt $*\"\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    directory
}

#[gsttt_scenario::scenario("distribution", "A missing binary is one line")]
#[test]
fn a_missing_binary_is_one_line() {
    let no_programs = tempfile::tempdir().unwrap();

    let output = run_hook(&session_start_command(), no_programs.path());

    assert!(output.status.success(), "{}", printed(&output));
    let printed_line = String::from_utf8(output.stdout).unwrap();
    assert_eq!(printed_line.lines().count(), 1, "{printed_line}");
    assert!(printed_line.contains("install.sh"), "{printed_line}");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn a_missing_binary_is_silent_on_every_other_event() {
    let no_programs = tempfile::tempdir().unwrap();

    for arguments in [
        vec![],
        vec!["mcp"],
        vec!["hook"],
        vec!["hook", "pre-tool-use"],
        vec!["hook", "session-end"],
    ] {
        let output = Command::new(wrapper())
            .args(&arguments)
            .env("PATH", no_programs.path())
            .output()
            .unwrap();

        assert!(output.status.success(), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
        assert!(output.stderr.is_empty(), "{arguments:?}");
    }
}

#[test]
fn an_installed_binary_gets_the_hook_and_the_server_arguments() {
    let installed = directory_with_gsttt();

    let hook = run_hook(&session_start_command(), installed.path());
    let server = Command::new(wrapper())
        .arg("mcp")
        .env("PATH", installed.path())
        .output()
        .unwrap();

    assert_eq!(
        String::from_utf8_lossy(&hook.stdout),
        "gsttt hook session-start\n"
    );
    assert_eq!(String::from_utf8_lossy(&server.stdout), "gsttt mcp\n");
}

#[test]
fn the_marketplace_lists_the_plugin_and_the_plugin_declares_its_parts() {
    let marketplace = json_file(".claude-plugin/marketplace.json");
    let plugins = marketplace["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0]["name"], "gsttt");
    assert_eq!(plugins[0]["source"], "./plugin");

    let manifest = json_file("plugin/.claude-plugin/plugin.json");
    assert_eq!(manifest["name"], "gsttt");
    assert_eq!(manifest["skills"], json!(["./skills/gsttt"]));
    assert_eq!(manifest["mcpServers"]["gsttt"]["args"], json!(["mcp"]));
    // The server and the hook both run through the wrapper that survives a missing binary.
    assert_eq!(
        manifest["mcpServers"]["gsttt"]["command"],
        "${CLAUDE_PLUGIN_ROOT}/scripts/gsttt.sh"
    );
    assert!(session_start_command().contains("${CLAUDE_PLUGIN_ROOT}/scripts/gsttt.sh"));
    assert!(
        fs::metadata(wrapper()).unwrap().permissions().mode() & 0o111 != 0,
        "the wrapper is executable"
    );
}

#[test]
fn the_skill_has_a_name_and_a_description_and_names_no_tool() {
    let skill = fs::read_to_string(workspace_root().join("plugin/skills/gsttt/SKILL.md")).unwrap();

    assert!(
        skill.starts_with("---\nname: gsttt\ndescription: "),
        "{}",
        skill.lines().take(3).collect::<Vec<_>>().join("\n")
    );
    assert!(
        !skill.contains("mcp__"),
        "the skill names an MCP tool id, which the server decides"
    );
}
