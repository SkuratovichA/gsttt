//! `gsttt update`, run against a machine made of fakes: a `claude` that logs its calls, an installed `gsttt` that only prints its version, and a directory of packages that holds only the name of the newest release. The installer finds the installed version to be the newest one and downloads nothing, so no package has to be built. The download path of the installer is proven against the same file by the tests of `xtask`.

use std::path::{Path, PathBuf};
use std::process::Command;

use gsttt::cli::VERSION;
use gsttt_scenario::scenario;
use serde_json::{Value, json};

use super::support::{Ran, Sandbox, ran, write, write_executable};

/// A `claude` that appends `CLAUDE_CONFIG_DIR` (or `unset`), the physical current directory and the arguments of each call to `FAKE_CLAUDE_LOG`, and fails when `CLAUDE_CONFIG_DIR` is the value of `FAKE_CLAUDE_FAIL`.
const CLAUDE: &str = r#"#!/bin/sh
printf '%s|%s|%s\n' "${CLAUDE_CONFIG_DIR:-unset}" "$(pwd -P)" "$*" >> "$FAKE_CLAUDE_LOG"
echo "fake claude: $*"
if [ -n "${CLAUDE_CONFIG_DIR:-}" ] && [ "${CLAUDE_CONFIG_DIR}" = "${FAKE_CLAUDE_FAIL:-}" ]; then
  echo "fake claude: failing" >&2
  exit 1
fi
"#;

/// A `gsttt` that prints a version whatever it is asked.
fn installed_gsttt(version: &str) -> String {
    format!("#!/bin/sh\necho 'gsttt {version}'\n")
}

/// A call of `claude` as the fake logged it, the paths canonical because a temporary directory on macOS is reached through a link.
#[derive(Debug, PartialEq, Eq)]
struct Called {
    /// The `CLAUDE_CONFIG_DIR` of the call, `None` when it was unset.
    config: Option<PathBuf>,
    directory: PathBuf,
    arguments: String,
}

impl Called {
    fn new(config: Option<&Path>, directory: &Path, arguments: &str) -> Self {
        Self {
            config: config.map(|config| config.canonicalize().unwrap()),
            directory: directory.canonicalize().unwrap(),
            arguments: arguments.to_owned(),
        }
    }
}

/// A machine to run `gsttt update` on.
struct Machine {
    sandbox: Sandbox,
    /// The directory that holds the fake `claude`, and nothing else, in front of `/usr/bin:/bin`.
    tools: PathBuf,
    /// The install directory, holding the fake installed `gsttt`.
    install: PathBuf,
    /// The directory the installer takes its packages from, which holds `latest`.
    packages: PathBuf,
    log: PathBuf,
}

impl Machine {
    /// A machine with `claude`, and a `gsttt` of the newest release, 0.1.99, installed.
    fn new() -> Self {
        // The real `claude` must never run: it would update the plugin of whoever runs the tests.
        for directory in ["/usr/bin", "/bin"] {
            assert!(!Path::new(directory).join("claude").exists());
        }
        let sandbox = Sandbox::new();
        let work = sandbox.work.path().to_owned();
        let machine = Self {
            tools: work.join("tools"),
            install: work.join("bin"),
            packages: work.join("packages"),
            log: work.join("claude.log"),
            sandbox,
        };
        write_executable(&machine.tools, "claude", CLAUDE);
        write(&machine.packages, "latest", "0.1.99\n");
        machine.install_gsttt("0.1.99");
        machine
    }

    /// Replaces the installed `gsttt` by one that says it is `version`.
    fn install_gsttt(&self, version: &str) {
        write_executable(&self.install, "gsttt", &installed_gsttt(version));
    }

    fn home(&self) -> &Path {
        self.sandbox.home.path()
    }

    /// Makes a Claude Code config `name` in the home directory where gsttt is installed once for each of `installs`, and returns its directory.
    fn config(&self, name: &str, installs: &[(&str, Option<&Path>)]) -> PathBuf {
        let directory = self.home().join(name);
        write(
            &directory,
            "plugins/installed_plugins.json",
            &installed_plugins(installs),
        );
        directory
    }

    /// `gsttt update` with `arguments`, with the fakes in place and `GSTTT_INSTALL_DIR` set.
    fn update(&self, arguments: &[&str]) -> Command {
        let mut command = self.without_install_directory(arguments);
        command.env("GSTTT_INSTALL_DIR", &self.install);
        command
    }

    /// `gsttt update` with `arguments`, with the fakes in place and no `GSTTT_INSTALL_DIR`.
    fn without_install_directory(&self, arguments: &[&str]) -> Command {
        let mut command = self.sandbox.gsttt();
        command
            .env("PATH", format!("{}:/usr/bin:/bin", self.tools.display()))
            .env(
                "GSTTT_BASE_URL",
                format!("file://{}", self.packages.display()),
            )
            .env("FAKE_CLAUDE_LOG", &self.log)
            .arg("update")
            .args(arguments);
        command
    }

    /// Runs `gsttt update` with `arguments`.
    fn run(&self, arguments: &[&str]) -> Ran {
        ran(&mut self.update(arguments))
    }

    /// The calls of `claude` so far, in order.
    fn calls(&self) -> Vec<Called> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(|line| {
                let mut parts = line.splitn(3, '|');
                let config = parts.next().unwrap();
                let directory = parts.next().unwrap();
                let arguments = parts.next().unwrap();
                Called::new(
                    (config != "unset").then(|| Path::new(config)),
                    Path::new(directory),
                    arguments,
                )
            })
            .collect()
    }
}

/// What Claude Code writes to `plugins/installed_plugins.json` of a config where gsttt is installed once for each of `installs`: a scope, and the project of a scope that is not the user's. Another plugin is listed too, which the update ignores.
fn installed_plugins(installs: &[(&str, Option<&Path>)]) -> String {
    let entries: Vec<Value> = installs
        .iter()
        .map(|(scope, project)| {
            let mut entry = json!({
                "scope": scope,
                "installPath": "/claude/plugins/cache/gsttt/gsttt/9b6517d98ed1",
                "version": "9b6517d98ed1",
                "gitCommitSha": "9b6517d98ed1b8c917379dedc4fa592a6e07b605",
            });
            if let Some(project) = project {
                entry["projectPath"] = json!(project);
            }
            entry
        })
        .collect();
    json!({
        "version": 2,
        "plugins": {
            "gsttt@gsttt": entries,
            "other@market": [{"scope": "user", "installPath": "/x", "version": "1"}],
        },
    })
    .to_string()
}

const MARKETPLACE: &str = "plugin marketplace update gsttt";
const USER: &str = "plugin update gsttt@gsttt --scope user";
const LOCAL: &str = "plugin update gsttt@gsttt --scope local";
const RESTART: &str = "Restart the Claude Code sessions that are running";

#[scenario("cli", "An update with nothing new still updates the plugin")]
#[test]
fn an_update_with_nothing_new_still_updates_the_plugin() {
    let machine = Machine::new();
    machine.config(".claude", &[("user", None)]);

    let run = machine.run(&[]);

    assert_eq!(run.code, Some(0), "{run:?}");
    let installed = format!(
        "gsttt 0.1.99 is already installed at {}/gsttt",
        machine.install.display()
    );
    assert!(run.stdout.contains(&installed), "{run:?}");
    let line = format!(
        "Installing the newest release into {} (this is gsttt {VERSION})",
        machine.install.display()
    );
    assert!(run.stdout.contains(&line), "{run:?}");
    let home = machine.home();
    assert_eq!(
        machine.calls(),
        [
            Called::new(None, home, MARKETPLACE),
            Called::new(None, home, USER)
        ]
    );
    assert!(run.stdout.contains(RESTART), "{run:?}");
}

#[scenario("cli", "Every config that has the plugin is updated")]
#[test]
fn every_config_that_has_the_plugin_is_updated_from_the_right_place() {
    let machine = Machine::new();
    let home = machine.home();
    let project = machine.sandbox.work.path().join("app");
    std::fs::create_dir(&project).unwrap();
    let main = machine.config(".claude", &[("user", None), ("local", Some(&project))]);
    let work = machine.config(".claude-work", &[("user", None)]);
    std::fs::create_dir(home.join(".claude-backup")).unwrap();
    std::os::unix::fs::symlink(main.join("plugins"), home.join(".claude-backup/plugins")).unwrap();
    write(
        home,
        ".claude-none/plugins/installed_plugins.json",
        r#"{"version":2,"plugins":{"other@market":[]}}"#,
    );
    // The one `CLAUDE_CONFIG_DIR` names is found whatever its name and place; this one sorts after the others.
    let elsewhere = machine.config("elsewhere/claude", &[("user", None)]);
    let mut command = machine.update(&[]);
    command.env("CLAUDE_CONFIG_DIR", &elsewhere);

    let run = ran(&mut command);

    assert_eq!(run.code, Some(0), "{run:?}");
    assert_eq!(
        machine.calls(),
        [
            Called::new(None, home, MARKETPLACE),
            Called::new(None, home, USER),
            Called::new(None, &project, LOCAL),
            Called::new(Some(&work), home, MARKETPLACE),
            Called::new(Some(&work), home, USER),
            Called::new(Some(&elsewhere), home, MARKETPLACE),
            Called::new(Some(&elsewhere), home, USER),
        ]
    );
}

#[scenario("cli", "A project that no longer exists is skipped")]
#[test]
fn a_project_that_no_longer_exists_is_skipped_with_a_note() {
    let machine = Machine::new();
    let gone = machine.sandbox.work.path().join("gone");
    machine.config(".claude", &[("user", None), ("local", Some(&gone))]);

    let run = machine.run(&[]);

    assert_eq!(run.code, Some(0), "{run:?}");
    assert!(
        run.stderr.contains("note: left out the local install"),
        "{run:?}"
    );
    assert!(run.stderr.contains(&gone.display().to_string()), "{run:?}");
    assert_eq!(
        machine.calls(),
        [
            Called::new(None, machine.home(), MARKETPLACE),
            Called::new(None, machine.home(), USER)
        ]
    );
}

#[scenario("cli", "A failing plugin update fails the command")]
#[test]
fn a_failing_plugin_update_fails_the_command_after_the_other_configs_were_updated() {
    let machine = Machine::new();
    let failing = machine.config(".claude-one", &[("user", None)]);
    let working = machine.config(".claude-two", &[("user", None)]);
    let mut command = machine.update(&[]);
    command.env("FAKE_CLAUDE_FAIL", &failing);

    let run = ran(&mut command);

    assert_eq!(run.code, Some(1), "{run:?}");
    assert!(run.stderr.contains("note:"), "{run:?}");
    assert!(
        run.stderr
            .contains("claude plugin update gsttt@gsttt --scope user"),
        "{run:?}"
    );
    assert!(
        run.stderr.contains("not up to date in every config"),
        "{run:?}"
    );
    let home = machine.home();
    assert_eq!(
        machine.calls(),
        [
            Called::new(Some(&failing), home, MARKETPLACE),
            Called::new(Some(&failing), home, USER),
            Called::new(Some(&working), home, MARKETPLACE),
            Called::new(Some(&working), home, USER),
        ]
    );
    assert!(run.stdout.contains(RESTART), "{run:?}");
}

#[scenario(
    "cli",
    "A machine without claude is told which configs were not updated"
)]
#[test]
fn without_claude_on_the_path_the_configs_that_were_not_updated_are_named() {
    let machine = Machine::new();
    std::fs::remove_file(machine.tools.join("claude")).unwrap();
    let first = machine.config(".claude", &[("user", None)]);
    let second = machine.config(".claude-work", &[("user", None)]);

    let run = machine.run(&[]);

    assert_eq!(run.code, Some(1), "{run:?}");
    assert!(
        run.stderr.contains(&format!("  {}\n", first.display())),
        "{run:?}"
    );
    assert!(
        run.stderr.contains(&format!("  {}\n", second.display())),
        "{run:?}"
    );
    assert!(
        run.stderr
            .contains("claude plugin update gsttt@gsttt --scope user\n"),
        "{run:?}"
    );
    assert!(
        run.stderr.contains(&format!(
            "CLAUDE_CONFIG_DIR={} claude plugin marketplace update gsttt",
            second.display()
        )),
        "{run:?}"
    );
    assert!(!run.stdout.contains(RESTART), "{run:?}");
    assert_eq!(machine.calls(), []);
}

#[scenario("cli", "Without the plugin the commands to add it are printed")]
#[test]
fn without_the_plugin_anywhere_the_commands_to_add_it_are_printed() {
    let machine = Machine::new();
    write(
        machine.home(),
        ".claude/plugins/installed_plugins.json",
        r#"{"version":2,"plugins":{"other@market":[]}}"#,
    );

    let run = machine.run(&[]);

    assert_eq!(run.code, Some(0), "{run:?}");
    assert!(
        run.stdout
            .contains("/plugin marketplace add SkuratovichA/gsttt\n"),
        "{run:?}"
    );
    assert!(
        run.stdout.contains("/plugin install gsttt@gsttt\n"),
        "{run:?}"
    );
    assert_eq!(machine.calls(), []);
}

#[scenario("cli", "A named version leaves the plugin alone")]
#[test]
fn a_named_version_leaves_the_plugin_alone() {
    let machine = Machine::new();
    machine.install_gsttt("0.1.98");
    machine.config(".claude", &[("user", None)]);

    let run = machine.run(&["0.1.98"]);

    assert_eq!(run.code, Some(0), "{run:?}");
    assert!(
        run.stdout.contains("gsttt 0.1.98 is already installed"),
        "{run:?}"
    );
    assert!(
        run.stdout
            .contains("The plugin follows the newest release, so it is left as it is."),
        "{run:?}"
    );
    assert_eq!(machine.calls(), []);
}

#[scenario("cli", "A failed install leaves the plugin alone")]
#[test]
fn a_failed_install_leaves_the_plugin_alone() {
    let machine = Machine::new();
    // The newest release is 0.1.99, which is not installed, and there is no package to download.
    machine.install_gsttt("0.1.98");
    machine.config(".claude", &[("user", None)]);

    let run = machine.run(&[]);

    assert_eq!(run.code, Some(1), "{run:?}");
    assert!(run.stderr.contains("install.sh:"), "{run:?}");
    assert!(
        run.stderr
            .contains("the Claude Code plugin was left as it was"),
        "{run:?}"
    );
    assert_eq!(machine.calls(), []);
}

#[scenario("cli", "A gsttt built from source is not replaced")]
#[test]
fn a_gsttt_built_from_source_is_not_replaced() {
    // A binary built with `GSTTT_VERSION` set is not a development build, and `update` would replace the one in `target`.
    assert!(VERSION.ends_with("-dev"), "{VERSION}");
    let machine = Machine::new();
    machine.config(".claude", &[("user", None)]);

    let run = ran(&mut machine.without_install_directory(&[]));

    assert_eq!(run.code, Some(1), "{run:?}");
    assert!(run.stderr.contains("GSTTT_INSTALL_DIR"), "{run:?}");
    assert_eq!(run.stdout, "", "{run:?}");
    assert_eq!(machine.calls(), []);
}

#[scenario("cli", "The update takes no repository flags")]
#[test]
fn update_refuses_the_flags_of_a_repository() {
    let machine = Machine::new();
    machine.config(".claude", &[("user", None)]);

    for flag in [&["-C", "."][..], &["--json"], &["--all"], &["--no-refresh"]] {
        let run = machine.run(flag);

        assert_eq!(run.code, Some(2), "{flag:?}: {run:?}");
        assert_eq!(run.stdout, "", "{flag:?}: {run:?}");
        let named = flag.first().unwrap();
        assert!(
            run.stderr.contains(&format!("`{named}` was given")),
            "{flag:?}: {run:?}"
        );
    }
    assert_eq!(machine.calls(), []);
}

#[test]
fn an_empty_version_is_refused() {
    let machine = Machine::new();

    let run = machine.run(&[""]);

    assert_eq!(run.code, Some(2), "{run:?}");
    assert_eq!(run.stdout, "", "{run:?}");
}
