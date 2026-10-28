//! The commands, the version and the errors of a command line that cannot be run as given.

use clap::{CommandFactory, Parser};
use gsttt::Cli;
use gsttt_scenario::scenario;

use super::support::Sandbox;

/// The commands the specs list, each as it is typed: those of the cli capability, and `mcp`, which the mcp capability adds.
const COMMANDS: [&str; 17] = [
    "index",
    "impact",
    "dependents",
    "dependencies",
    "path",
    "contract",
    "coverage",
    "status",
    "mcp",
    "hook session-start",
    "init",
    "config check",
    "config show",
    "config trust",
    "config untrust",
    "schema external",
    "completions",
];

/// The commands clap knows, each as it is typed: the ones with no subcommands of their own.
fn defined_commands(command: &clap::Command, prefix: &str) -> Vec<String> {
    let mut names = Vec::new();
    for sub in command.get_subcommands() {
        let name = format!("{prefix}{}", sub.get_name());
        if sub.has_subcommands() {
            names.extend(defined_commands(sub, &format!("{name} ")));
        } else {
            names.push(name);
        }
    }
    names
}

#[test]
fn the_commands_are_those_of_the_spec() {
    assert_eq!(defined_commands(&Cli::command(), ""), COMMANDS);
}

#[scenario("cli", "No command prints the usage")]
#[test]
fn no_command_prints_the_usage() {
    let sandbox = Sandbox::new();
    for args in [&[][..], &["--json"][..]] {
        let ran = sandbox.run(args);
        assert_eq!(ran.code, Some(2), "{args:?}");
        assert_eq!(ran.stdout, "", "{args:?}");
        for command in COMMANDS {
            assert!(
                ran.stderr.contains(&format!("gsttt {command}")),
                "{args:?}: `{command}` is missing from the usage:\n{}",
                ran.stderr
            );
        }
    }
}

#[scenario("cli", "Completions are printed for a shell")]
#[test]
fn completions_are_printed_for_a_shell() {
    let sandbox = Sandbox::new();
    let ran = sandbox.run(&["completions", "zsh"]);
    assert_eq!(ran.code, Some(0));
    assert_eq!(ran.stderr, "");
    assert!(ran.stdout.starts_with("#compdef gsttt"), "{}", ran.stdout);
    assert!(ran.stdout.contains("_gsttt"));
    for command in ["impact", "dependents", "completions"] {
        assert!(ran.stdout.contains(command), "{command}");
    }
}

#[test]
fn completions_are_printed_for_every_shell_clap_lists() {
    let sandbox = Sandbox::new();
    for shell in ["bash", "elvish", "fish", "powershell", "zsh"] {
        let ran = sandbox.run(&["completions", shell]);
        assert_eq!(ran.code, Some(0), "{shell}");
        assert!(ran.stdout.contains("gsttt"), "{shell}");
    }
    let ran = sandbox.run(&["completions", "tcsh"]);
    assert_eq!(ran.code, Some(2));
    assert_eq!(ran.stdout, "");
    assert!(ran.stderr.contains("tcsh"));
}

#[scenario("cli", "A development build says so in its version")]
#[test]
fn a_development_build_says_so_in_its_version() {
    let sandbox = Sandbox::new();
    let expected = match option_env!("GSTTT_VERSION").filter(|version| !version.is_empty()) {
        Some(version) => version.to_owned(),
        None => format!("{}-dev", env!("CARGO_PKG_VERSION")),
    };
    let ran = sandbox.run(&["--version"]);
    assert_eq!(ran.code, Some(0));
    assert_eq!(ran.stdout, format!("gsttt {expected}\n"));
    assert_eq!(ran.stderr, "");
}

#[test]
fn global_flags_are_taken_before_and_after_the_command() {
    for args in [
        [
            "gsttt",
            "-C",
            "work",
            "--json",
            "--all",
            "--no-refresh",
            "coverage",
        ],
        [
            "gsttt",
            "coverage",
            "-C",
            "work",
            "--json",
            "--all",
            "--no-refresh",
        ],
    ] {
        let flags = Cli::try_parse_from(args).unwrap().flags;
        assert_eq!(
            flags.directory.as_ref().map(|dir| dir.as_str()),
            Some("work"),
            "{args:?}"
        );
        assert!(flags.json && flags.all && flags.no_refresh, "{args:?}");
    }
}

#[scenario("cli", "An unknown flag exits 2")]
#[test]
fn an_unknown_flag_exits_2() {
    let sandbox = Sandbox::new();
    let ran = sandbox.run(&["impact", "--frobnicate", "src/a.ts"]);
    assert_eq!(ran.code, Some(2));
    assert_eq!(ran.stdout, "");
    assert!(ran.stderr.contains("--frobnicate"), "{}", ran.stderr);
}

#[test]
fn an_unknown_command_exits_2() {
    let sandbox = Sandbox::new();
    let ran = sandbox.run(&["frobnicate"]);
    assert_eq!(ran.code, Some(2));
    assert_eq!(ran.stdout, "");
    assert!(ran.stderr.contains("frobnicate"), "{}", ran.stderr);
}

#[scenario("cli", "An empty contract query exits 2")]
#[test]
fn an_empty_contract_query_exits_2() {
    let sandbox = Sandbox::new();
    for args in [&["contract", ""][..], &["contract", "  ", ""][..]] {
        let ran = sandbox.run(args);
        assert_eq!(ran.code, Some(2), "{args:?}");
        assert_eq!(ran.stdout, "", "{args:?}");
        assert!(ran.stderr.contains("query is empty"), "{}", ran.stderr);
    }
}

#[scenario("cli", "--base without --changed exits 2")]
#[test]
fn base_without_changed_exits_2() {
    let sandbox = Sandbox::new();
    let ran = sandbox.run(&["impact", "--base", "main", "lib/util.ts"]);
    assert_eq!(ran.code, Some(2));
    assert_eq!(ran.stdout, "");
    assert!(
        ran.stderr.contains("`--base` needs `--changed`"),
        "{}",
        ran.stderr
    );
}

#[test]
fn impact_takes_paths_or_changed_and_not_both() {
    let sandbox = Sandbox::new();
    for args in [&["impact"][..], &["impact", "--changed", "lib/util.ts"][..]] {
        let ran = sandbox.run(args);
        assert_eq!(ran.code, Some(2), "{args:?}");
        assert_eq!(ran.stdout, "", "{args:?}");
        assert!(ran.stderr.contains("--changed"), "{args:?}: {}", ran.stderr);
    }
}

#[test]
fn index_cannot_run_without_a_refresh() {
    let sandbox = Sandbox::new();
    let ran = sandbox.run(&["--no-refresh", "index"]);
    assert_eq!(ran.code, Some(2));
    assert_eq!(ran.stdout, "");
    assert!(ran.stderr.contains("--no-refresh"), "{}", ran.stderr);
}

/// The commands of the third run are not built yet: `init`, `config` and `schema`. They find their root first, as
/// the built ones do, so a refused root is told as such.
#[test]
fn a_command_that_is_not_built_yet_says_so_and_exits_1() {
    let sandbox = Sandbox::new();
    for args in [
        &["init"][..],
        &["config", "check"][..],
        &["config", "show"][..],
        &["config", "trust"][..],
        &["config", "untrust"][..],
        &["--json", "schema", "external"][..],
    ] {
        let ran = sandbox.run(args);
        assert_eq!(ran.code, Some(1), "{args:?}");
        assert_eq!(ran.stdout, "", "{args:?}");
        assert!(
            ran.stderr.contains("is not built yet"),
            "{args:?}: {}",
            ran.stderr
        );
    }
}

#[test]
fn a_command_that_is_not_built_yet_still_refuses_the_home_directory() {
    let sandbox = Sandbox::new();
    let mut command = sandbox.gsttt();
    command
        .current_dir(sandbox.home.path())
        .args(["config", "check"]);
    let ran = super::support::ran(&mut command);
    assert_eq!(ran.code, Some(2), "{ran:?}");
    assert!(
        ran.stderr.contains("does not index the home directory"),
        "{ran:?}"
    );
}
