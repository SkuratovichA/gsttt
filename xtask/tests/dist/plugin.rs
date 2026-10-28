//! The Claude Code plugin in `plugin/` and the marketplace that lists it.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

use crate::support::{printed, workspace_root, write_executable};

fn json_file(relative: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(workspace_root().join(relative)).unwrap()).unwrap()
}

fn wrapper() -> PathBuf {
    workspace_root().join("plugin/scripts/gsttt.sh")
}

/// The hook entry of the session start in the manifest of the plugin.
fn session_start_hook() -> Value {
    json_file("plugin/.claude-plugin/plugin.json")
        .pointer("/hooks/SessionStart/0/hooks/0")
        .unwrap()
        .clone()
}

/// The command of the session-start hook in the manifest of the plugin.
fn session_start_command() -> String {
    session_start_hook()
        .get("command")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned()
}

/// Runs a hook `command` as Claude Code does, through a shell, with `CLAUDE_PLUGIN_ROOT` set, `path` as the whole of `PATH` and `install_dir` as the install directory.
fn run_hook(command: &str, path: &Path, install_dir: &Path) -> Output {
    Command::new("/bin/sh")
        .args(["-c", command])
        .env_clear()
        .env("CLAUDE_PLUGIN_ROOT", workspace_root().join("plugin"))
        .env("PATH", path)
        .env("GSTTT_INSTALL_DIR", install_dir)
        .output()
        .unwrap()
}

/// The wrapper, ready to run on a machine whose `PATH` is `path` and whose install directory is `install_dir`, and which has nothing else in its environment.
fn wrapper_command(path: &Path, install_dir: &Path) -> Command {
    let mut command = Command::new(wrapper());
    command
        .env_clear()
        .env("PATH", path)
        .env("GSTTT_INSTALL_DIR", install_dir);
    command
}

/// A directory that holds a `gsttt` made of `script`.
fn directory_with_script(script: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    write_executable(directory.path(), "gsttt", &format!("#!/bin/sh\n{script}"));
    directory
}

/// A directory that holds a `gsttt` which prints the arguments it was given.
fn directory_with_gsttt() -> tempfile::TempDir {
    directory_with_script("echo \"gsttt $*\"\n")
}

#[gsttt_scenario::scenario("distribution", "A missing binary is one line")]
#[test]
fn a_missing_binary_is_one_line() {
    let nowhere = tempfile::tempdir().unwrap();

    let output = run_hook(&session_start_command(), nowhere.path(), nowhere.path());

    assert!(output.status.success(), "{}", printed(&output));
    let printed_line = String::from_utf8(output.stdout).unwrap();
    assert_eq!(printed_line.lines().count(), 1, "{printed_line}");
    assert!(printed_line.contains("install.sh"), "{printed_line}");
    // What a hook prints goes into the context of the model, so the line must read as a note to the user and not as an order to the model.
    assert!(
        printed_line.to_lowercase().contains("note for the user"),
        "{printed_line}"
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn a_missing_binary_is_silent_on_every_other_event() {
    let nowhere = tempfile::tempdir().unwrap();

    for arguments in [
        vec!["hook"],
        vec!["hook", "pre-tool-use"],
        vec!["hook", "session-end"],
    ] {
        let output = wrapper_command(nowhere.path(), nowhere.path())
            .args(&arguments)
            .output()
            .unwrap();

        assert!(output.status.success(), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
        assert!(output.stderr.is_empty(), "{arguments:?}");
    }
}

#[gsttt_scenario::scenario("distribution", "A missing binary stops the server with one line")]
#[test]
fn a_missing_binary_stops_the_server_with_one_line() {
    let nowhere = tempfile::tempdir().unwrap();

    let output = wrapper_command(nowhere.path(), nowhere.path())
        .arg("mcp")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "{}", printed(&output));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    let line = String::from_utf8(output.stderr).unwrap();
    assert_eq!(line.lines().count(), 1, "{line}");
    assert!(line.contains("install.sh"), "{line}");
}

#[gsttt_scenario::scenario("distribution", "The binary in the install directory is found")]
#[test]
fn a_binary_in_the_install_directory_is_found_when_path_lacks_it() {
    let installed = directory_with_gsttt();
    let nowhere = tempfile::tempdir().unwrap();

    let server = wrapper_command(nowhere.path(), installed.path())
        .arg("mcp")
        .output()
        .unwrap();
    let hook = run_hook(&session_start_command(), nowhere.path(), installed.path());

    assert_eq!(String::from_utf8_lossy(&server.stdout), "gsttt mcp\n");
    assert_eq!(
        String::from_utf8_lossy(&hook.stdout),
        "gsttt hook session-start\n"
    );
}

#[test]
fn the_install_directory_defaults_to_local_bin_under_home() {
    let home = tempfile::tempdir().unwrap();
    write_executable(
        home.path(),
        ".local/bin/gsttt",
        "#!/bin/sh\necho \"gsttt $*\"\n",
    );
    let nowhere = tempfile::tempdir().unwrap();

    let output = Command::new(wrapper())
        .env_clear()
        .env("PATH", nowhere.path())
        .env("HOME", home.path())
        .arg("mcp")
        .output()
        .unwrap();

    assert_eq!(String::from_utf8_lossy(&output.stdout), "gsttt mcp\n");
}

#[test]
fn a_binary_on_the_path_wins_over_the_install_directory() {
    let on_path = directory_with_script("echo on the path\n");
    let in_install_dir = directory_with_script("echo in the install directory\n");

    let output = wrapper_command(on_path.path(), in_install_dir.path())
        .arg("mcp")
        .output()
        .unwrap();

    assert_eq!(String::from_utf8_lossy(&output.stdout), "on the path\n");
}

#[test]
fn an_installed_binary_gets_the_hook_and_the_server_arguments() {
    let installed = directory_with_gsttt();
    let nowhere = tempfile::tempdir().unwrap();

    let hook = run_hook(&session_start_command(), installed.path(), nowhere.path());
    let server = wrapper_command(installed.path(), nowhere.path())
        .arg("mcp")
        .output()
        .unwrap();

    assert_eq!(
        String::from_utf8_lossy(&hook.stdout),
        "gsttt hook session-start\n"
    );
    assert_eq!(String::from_utf8_lossy(&server.stdout), "gsttt mcp\n");
}

#[gsttt_scenario::scenario("distribution", "A failing hook is silent")]
#[test]
fn a_hook_that_fails_does_not_break_the_session() {
    // An old gsttt that does not know the hook: clap prints the error and exits with 2.
    let old = directory_with_script("echo \"error: unrecognized subcommand 'hook'\" >&2\nexit 2\n");
    let nowhere = tempfile::tempdir().unwrap();

    let output = run_hook(&session_start_command(), old.path(), nowhere.path());

    assert!(output.status.success(), "{}", printed(&output));
    assert!(output.stdout.is_empty(), "{}", printed(&output));
    assert!(output.stderr.is_empty(), "{}", printed(&output));
}

#[test]
fn the_server_keeps_the_exit_code_and_the_complaint_of_a_failing_binary() {
    let failing = directory_with_script("echo 'cannot start' >&2\nexit 3\n");
    let nowhere = tempfile::tempdir().unwrap();

    let output = wrapper_command(failing.path(), nowhere.path())
        .arg("mcp")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(3));
    assert_eq!(String::from_utf8_lossy(&output.stderr), "cannot start\n");
}

#[gsttt_scenario::scenario("distribution", "A hook is cut off after five seconds")]
#[test]
fn the_session_start_hook_has_a_timeout() {
    assert_eq!(session_start_hook().get("timeout"), Some(&json!(5)));
}

#[gsttt_scenario::scenario("distribution", "The marketplace lists the plugin")]
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
