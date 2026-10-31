//! `gsttt update`, run against a machine made of fakes: a `claude` that logs its calls and can rewrite the version it installed, an installed `gsttt` that only prints its version, and a directory of packages that holds the name of the newest release. Unless a test makes a package, the installer finds the installed version to be the newest one and downloads nothing. The download path of the installer is proven against the same file by the tests of `xtask`.

use std::path::{Path, PathBuf};
use std::process::Command;

use gsttt::cli::VERSION;
use gsttt_scenario::scenario;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::support::{Ran, Sandbox, ran, write, write_executable};

/// A `claude` that appends `CLAUDE_CONFIG_DIR` (or `unset`), the physical current directory and the arguments of each call to `FAKE_CLAUDE_LOG`, and prints a line about the call. It removes the directory `FAKE_CLAUDE_REMOVE` names, as a person might between two calls. It fails, after six more lines on stdout and one on stderr, when `CLAUDE_CONFIG_DIR` is the value of `FAKE_CLAUDE_FAIL` and the arguments match the pattern `FAKE_CLAUDE_FAIL_ON`, which is any by default. When `FAKE_CLAUDE_SED` is set, a `plugin update` that works applies it as a `sed` script to `installed_plugins.json` of its config, as Claude Code rewrites the file for the install it updates.
const CLAUDE: &str = r#"#!/bin/sh
printf '%s|%s|%s\n' "${CLAUDE_CONFIG_DIR:-unset}" "$(pwd -P)" "$*" >> "$FAKE_CLAUDE_LOG"
echo "fake claude: $*"
if [ -n "${FAKE_CLAUDE_REMOVE:-}" ]; then rm -rf "$FAKE_CLAUDE_REMOVE"; fi
if [ -n "${CLAUDE_CONFIG_DIR:-}" ] && [ "${CLAUDE_CONFIG_DIR}" = "${FAKE_CLAUDE_FAIL:-}" ]; then
  case "$*" in
    ${FAKE_CLAUDE_FAIL_ON:-*})
      for n in 1 2 3 4 5 6; do echo "fake claude: noise $n"; done
      echo "fake claude: failing" >&2
      exit 1 ;;
  esac
fi
case "$*" in
  "plugin update "*)
    if [ -n "${FAKE_CLAUDE_SED:-}" ]; then
      file="${CLAUDE_CONFIG_DIR:-$HOME/.claude}/plugins/installed_plugins.json"
      sed "$FAKE_CLAUDE_SED" "$file" > "$file.new" && mv "$file.new" "$file"
    fi ;;
esac
"#;

/// The targets that have a package, whichever of them the installer picks for this machine.
const TARGETS: [&str; 4] = [
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
];

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
        machine.write_claude();
        write(&machine.packages, "latest", "0.1.99\n");
        machine.install_gsttt("0.1.99");
        machine
    }

    /// Puts the fake `claude` where the `PATH` of the fakes finds it.
    fn write_claude(&self) {
        write_executable(&self.tools, "claude", CLAUDE);
    }

    /// Replaces the installed `gsttt` by one that says it is `version`.
    fn install_gsttt(&self, version: &str) {
        write_executable(&self.install, "gsttt", &installed_gsttt(version));
    }

    /// Makes `version` the newest release, with a package for every target whose `gsttt` says it is `version`, so that the installer has something to download.
    fn release(&self, version: &str) {
        let staging = tempfile::tempdir().unwrap();
        write_executable(staging.path(), "gsttt", &installed_gsttt(version));
        for target in TARGETS {
            let archive = format!("gsttt-{version}-{target}.tar.gz");
            let path = self.packages.join(&archive);
            let status = Command::new("tar")
                .arg("-czf")
                .arg(&path)
                .arg("-C")
                .arg(staging.path())
                .arg("gsttt")
                .status()
                .unwrap();
            assert!(status.success(), "tar: {status}");
            let digest = faster_hex::hex_string(&Sha256::digest(std::fs::read(&path).unwrap()));
            write(
                &self.packages,
                &format!("{archive}.sha256"),
                &format!("{digest}  {archive}\n"),
            );
        }
        write(&self.packages, "latest", &format!("{version}\n"));
    }

    fn home(&self) -> &Path {
        self.sandbox.home.path()
    }

    /// Makes a Claude Code config `name` in the home directory where gsttt is installed once for each of `installs`, at the version `OLD`, and returns its directory.
    fn config(&self, name: &str, installs: &[(&str, Option<&Path>)]) -> PathBuf {
        let versions: Vec<_> = installs
            .iter()
            .map(|(scope, project)| (*scope, *project, OLD))
            .collect();
        self.config_at(name, &versions)
    }

    /// Like [`Machine::config`], with the version of each install given.
    fn config_at(&self, name: &str, installs: &[(&str, Option<&Path>, &str)]) -> PathBuf {
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

    /// Runs `gsttt update` with `arguments`, the fake `claude` moving the installs of version `OLD` to `NEW`.
    fn run_bumping(&self, arguments: &[&str]) -> Ran {
        ran(self.update(arguments).env("FAKE_CLAUDE_SED", bump()))
    }

    /// Runs `script` in a POSIX shell that starts in the work directory, with the fake `claude` on its `PATH`, as a person would paste lines there, `exported` being the `CLAUDE_CONFIG_DIR` of that shell. It ends by printing the directory it is in.
    fn paste(&self, script: &str, exported: Option<&Path>) -> Ran {
        let mut command = Command::new("sh");
        command
            .env_clear()
            .env("HOME", self.home())
            .env("PATH", format!("{}:/usr/bin:/bin", self.tools.display()))
            .env("FAKE_CLAUDE_LOG", &self.log)
            .current_dir(self.sandbox.work.path())
            .arg("-c")
            .arg(format!("{script}\npwd -P"));
        if let Some(config) = exported {
            command.env("CLAUDE_CONFIG_DIR", config);
        }
        ran(&mut command)
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

/// The script with which the fake `claude` moves the installs at `OLD` to `NEW`.
fn bump() -> String {
    format!(r#"s/"version":"{OLD}"/"version":"{NEW}"/"#)
}

/// What Claude Code writes to `plugins/installed_plugins.json` of a config where gsttt is installed once for each of `installs`: a scope, the project of a scope that is not the user's, and a version. Another plugin is listed too, which the update ignores.
fn installed_plugins(installs: &[(&str, Option<&Path>, &str)]) -> String {
    let entries: Vec<Value> = installs
        .iter()
        .map(|(scope, project, version)| {
            let mut entry = json!({
                "scope": scope,
                "installPath": format!("/claude/plugins/cache/gsttt/gsttt/{version}"),
                "version": version,
                "gitCommitSha": format!("{version}b8c917379dedc4fa592a6e07b605"),
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
const RESTART: &str = "Restart running Claude Code sessions to load the plugin.\n";

/// The version the plugins are installed at, and the one the fake `claude` moves them to.
const OLD: &str = "9b6517d98ed1";
const NEW: &str = "4c1f2aa3b5c6";

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
    assert!(run.stderr.contains(&installed), "{run:?}");
    let line = format!(
        "Installing the newest release into {} (this is gsttt {VERSION})",
        machine.install.display()
    );
    assert!(run.stderr.contains(&line), "{run:?}");
    let home = machine.home();
    assert_eq!(
        machine.calls(),
        [
            Called::new(None, home, MARKETPLACE),
            Called::new(None, home, USER)
        ]
    );
    let newest = format!("The plugin in ~/.claude (user) is already the newest: {OLD}\n");
    assert!(run.stderr.contains(&newest), "{run:?}");
    assert!(!run.stderr.contains(RESTART), "{run:?}");
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
    // This one is on disk when the update plans its calls, and goes while the first call of `claude` runs.
    let vanishing = machine.sandbox.work.path().join("vanishing");
    std::fs::create_dir(&vanishing).unwrap();
    machine.config(
        ".claude",
        &[
            ("user", None),
            ("local", Some(&gone)),
            ("project", Some(&vanishing)),
        ],
    );
    let mut command = machine.update(&[]);
    command.env("FAKE_CLAUDE_REMOVE", &vanishing);

    let run = ran(&mut command);

    assert_eq!(run.code, Some(0), "{run:?}");
    assert!(
        run.stderr.contains(&format!(
            "note: left out the local install of gsttt@gsttt in ~/.claude: its project {} is not a directory\n",
            gone.display()
        )),
        "{run:?}"
    );
    assert!(
        run.stderr.contains(&format!(
            "note: left out the project install of gsttt@gsttt in ~/.claude: its project {} is not a directory\n",
            vanishing.display()
        )),
        "{run:?}"
    );
    // A directory that is gone is not a `claude` that is gone.
    assert!(!run.stderr.contains("not on PATH"), "{run:?}");
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
    let home = machine.home();
    // Names that a shell splits at the space and would take the quote of for the start of a string.
    let project = home.join("my app's");
    std::fs::create_dir(&project).unwrap();
    let failing = machine.config(".claude it's", &[("local", Some(&project))]);
    let working = machine.config(".claude-two", &[("user", None)]);
    let mut command = machine.update(&[]);
    // The marketplace is refreshed, and only the update of the plugin fails.
    command
        .env("FAKE_CLAUDE_FAIL", &failing)
        .env("FAKE_CLAUDE_FAIL_ON", "plugin update *")
        .env("FAKE_CLAUDE_SED", bump());

    let run = ran(&mut command);

    assert_eq!(run.code, Some(1), "{run:?}");
    let home_text = home.display();
    let rerun = format!(
        "(cd \"{home_text}/my app's\" && CLAUDE_CONFIG_DIR=\"{home_text}/.claude it's\" claude plugin update gsttt@gsttt --scope local)"
    );
    let failed = format!(
        "note: the plugin in ~/.claude it's (local, ~/my app's) was not updated: `{rerun}` ended with exit status: 1\n  claude printed:\n    fake claude: noise 3\n    fake claude: noise 4\n    fake claude: noise 5\n    fake claude: noise 6\n    fake claude: failing\n"
    );
    assert!(run.stderr.contains(&failed), "{run:?}");
    // Only the last five lines are quoted, and what `claude` printed in a call that worked is not.
    assert!(!run.stderr.contains("noise 2"), "{run:?}");
    assert!(!run.stderr.contains("fake claude: plugin"), "{run:?}");
    assert!(
        run.stderr.contains(&format!(
            "Updated the plugin in ~/.claude-two (user): {OLD} → {NEW}\n"
        )),
        "{run:?}"
    );
    assert!(
        run.stderr.contains("not up to date in every config"),
        "{run:?}"
    );
    assert_eq!(
        machine.calls(),
        [
            Called::new(Some(&failing), home, MARKETPLACE),
            Called::new(Some(&failing), &project, LOCAL),
            Called::new(Some(&working), home, MARKETPLACE),
            Called::new(Some(&working), home, USER),
        ]
    );
    assert!(run.stderr.contains(RESTART), "{run:?}");
}

#[scenario("cli", "A failed marketplace refresh leaves its plugins alone")]
#[test]
fn a_failed_marketplace_refresh_leaves_its_plugins_alone() {
    let machine = Machine::new();
    let project = machine.home().join("app");
    std::fs::create_dir(&project).unwrap();
    let stale = machine.config(".claude-one", &[("user", None), ("local", Some(&project))]);
    let fresh = machine.config(".claude-two", &[("user", None)]);
    let mut command = machine.update(&[]);
    command
        .env("FAKE_CLAUDE_FAIL", &stale)
        .env("FAKE_CLAUDE_FAIL_ON", "plugin marketplace *")
        .env("FAKE_CLAUDE_SED", bump());

    let run = ran(&mut command);

    assert_eq!(run.code, Some(1), "{run:?}");
    let refresh = format!(
        "note: the marketplace gsttt in ~/.claude-one was not refreshed: `CLAUDE_CONFIG_DIR={} claude plugin marketplace update gsttt` ended with exit status: 1\n",
        stale.display()
    );
    assert!(run.stderr.contains(&refresh), "{run:?}");
    for place in ["~/.claude-one (user)", "~/.claude-one (local, ~/app)"] {
        let skipped = format!(
            "note: the plugin in {place} was not updated, because the marketplace gsttt was not refreshed\n"
        );
        assert!(run.stderr.contains(&skipped), "{place}: {run:?}");
    }
    // The other config does not depend on it.
    assert!(
        run.stderr.contains(&format!(
            "Updated the plugin in ~/.claude-two (user): {OLD} → {NEW}\n"
        )),
        "{run:?}"
    );
    let home = machine.home();
    assert_eq!(
        machine.calls(),
        [
            Called::new(Some(&stale), home, MARKETPLACE),
            Called::new(Some(&fresh), home, MARKETPLACE),
            Called::new(Some(&fresh), home, USER),
        ]
    );
}

#[scenario(
    "cli",
    "A machine without claude is told which configs were not updated"
)]
#[test]
fn without_claude_on_the_path_the_configs_that_were_not_updated_are_named() {
    let machine = Machine::new();
    std::fs::remove_file(machine.tools.join("claude")).unwrap();
    machine.config(".claude", &[("user", None)]);
    let second = machine.config(".claude-work", &[("user", None)]);

    let run = machine.run(&[]);

    assert_eq!(run.code, Some(1), "{run:?}");
    // The config that is `~/.claude` is named by taking the variable away, the other by setting it.
    let second = second.display();
    let note = format!(
        "note: `claude` is not on PATH, so the plugin was left as it was in 2 configs. Where `claude` is, run:\n\
         \x20 ~/.claude\n\
         \x20   env -u CLAUDE_CONFIG_DIR claude plugin marketplace update gsttt\n\
         \x20   env -u CLAUDE_CONFIG_DIR claude plugin update gsttt@gsttt --scope user\n\
         \x20 ~/.claude-work\n\
         \x20   CLAUDE_CONFIG_DIR={second} claude plugin marketplace update gsttt\n\
         \x20   CLAUDE_CONFIG_DIR={second} claude plugin update gsttt@gsttt --scope user\n"
    );
    assert!(run.stderr.contains(&note), "{run:?}");
    assert!(!run.stderr.contains(RESTART), "{run:?}");
    assert_eq!(machine.calls(), []);
}

#[scenario("cli", "The lines to run by hand are safe to paste")]
#[test]
fn the_lines_to_run_by_hand_name_their_config_and_leave_the_shell_where_it_was() {
    let machine = Machine::new();
    std::fs::remove_file(machine.tools.join("claude")).unwrap();
    let home = machine.home();
    let project = home.join("my app's");
    std::fs::create_dir(&project).unwrap();
    machine.config(".claude", &[("user", None)]);
    let quoted = machine.config(".claude it's", &[("local", Some(&project))]);
    let run = machine.run(&[]);
    assert_eq!(run.code, Some(1), "{run:?}");
    let lines: Vec<&str> = run
        .stderr
        .lines()
        .filter_map(|line| line.strip_prefix("    "))
        .collect();
    assert_eq!(lines.len(), 4, "{run:?}");

    // The person has another config exported, and is in a directory of their own.
    machine.write_claude();
    let elsewhere = home.join("elsewhere");
    let pasted = machine.paste(&lines.join("\n"), Some(&elsewhere));

    assert_eq!(pasted.code, Some(0), "{pasted:?}");
    let work = machine.sandbox.work.path();
    assert_eq!(
        machine.calls(),
        [
            Called::new(None, work, MARKETPLACE),
            Called::new(None, work, USER),
            Called::new(Some(&quoted), work, MARKETPLACE),
            Called::new(Some(&quoted), &project, LOCAL),
        ]
    );
    let after = pasted.stdout.lines().last().unwrap_or_default();
    assert_eq!(Path::new(after), work.canonicalize().unwrap());
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
        run.stderr
            .contains("/plugin marketplace add SkuratovichA/gsttt\n"),
        "{run:?}"
    );
    assert!(
        run.stderr.contains("/plugin install gsttt@gsttt\n"),
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
        run.stderr.contains("gsttt 0.1.98 is already installed"),
        "{run:?}"
    );
    assert!(
        run.stderr
            .contains("The plugin follows the newest release, so it is left as it is."),
        "{run:?}"
    );
    assert!(!run.stderr.contains(RESTART), "{run:?}");
    assert_eq!(machine.calls(), []);
}

#[test]
fn a_version_in_the_environment_of_the_installer_does_not_name_the_release_to_install() {
    let machine = Machine::new();
    machine.config(".claude", &[("user", None)]);
    // The installer reads `GSTTT_VERSION` too. An update that names no version wants the newest release, 0.1.99, which is installed, and there is no package of 0.1.50 to download.
    let mut command = machine.update(&[]);
    command.env("GSTTT_VERSION", "0.1.50");

    let run = ran(&mut command);

    assert_eq!(run.code, Some(0), "{run:?}");
    assert!(
        run.stderr.contains("gsttt 0.1.99 is already installed at"),
        "{run:?}"
    );
    assert!(!run.stderr.contains("0.1.50"), "{run:?}");
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

#[scenario("cli", "The plugin lines say what changed")]
#[test]
fn the_plugin_lines_say_what_changed_and_not_what_claude_printed() {
    let machine = Machine::new();
    let home = machine.home();
    let project = home.join("app");
    std::fs::create_dir(&project).unwrap();
    // Only the install at `OLD` is moved to `NEW`: the one of the project is at a version that is the newest already.
    let newest = "7e0d1c9a4b20";
    machine.config_at(
        ".claude",
        &[("user", None, OLD), ("local", Some(&project), newest)],
    );
    // An install with no version in the file, which `claude` gives one, and one that stays at its version and moves to another commit of it.
    write(
        home,
        ".claude-bare/plugins/installed_plugins.json",
        &json!({"version": 2, "plugins": {"gsttt@gsttt": [
            {"scope": "user", "installPath": "/claude/plugins/cache/gsttt/gsttt/bare"}
        ]}})
        .to_string(),
    );
    let pinned = "3c91f0aa27d4";
    machine.config_at(".claude-commit", &[("user", None, pinned)]);
    let sed = format!(
        "{};s|\"installPath\":\"/claude/plugins/cache/gsttt/gsttt/bare\"|&,\"version\":\"1.0.0\"|;s/\"gitCommitSha\":\"{pinned}b8c917379dedc4fa592a6e07b605\"/\"gitCommitSha\":\"e5b2a6d8c7f3aaaaaaaaaaaaaaaaaaaaaaaaaaaa\"/",
        bump()
    );
    let mut command = machine.update(&[]);
    command.env("FAKE_CLAUDE_SED", sed);

    let run = ran(&mut command);

    assert_eq!(run.code, Some(0), "{run:?}");
    assert_eq!(run.stdout, "", "{run:?}");
    let install = machine.install.display();
    let expected = format!(
        "Installing the newest release into {install} (this is gsttt {VERSION})\n\
         gsttt 0.1.99 is already installed at {install}/gsttt\n\
         {install} is not in your PATH. To run gsttt by name, add this line to your shell profile:\n  \
         export PATH=\"{install}:$PATH\"\n\
         Refreshing the marketplace gsttt in ~/.claude\n\
         Updated the plugin in ~/.claude (user): {OLD} → {NEW}\n\
         The plugin in ~/.claude (local, ~/app) is already the newest: {newest}\n\
         Refreshing the marketplace gsttt in ~/.claude-bare\n\
         Updated the plugin in ~/.claude-bare (user) to 1.0.0\n\
         Refreshing the marketplace gsttt in ~/.claude-commit\n\
         Updated the plugin in ~/.claude-commit (user): {pinned}, commit {pinned} → e5b2a6d8c7f3\n\
         {RESTART}"
    );
    assert_eq!(run.stderr, expected);
}

#[scenario("cli", "An install that is gone after the update is a failure")]
#[test]
fn an_install_that_is_gone_after_the_update_is_a_failure() {
    let machine = Machine::new();
    machine.config(".claude", &[("user", None)]);
    let mut command = machine.update(&[]);
    // The install is not in the file any more, as after a `claude` that dropped it.
    command.env("FAKE_CLAUDE_SED", r#"s/"scope":"user"/"scope":"gone"/"#);

    let run = ran(&mut command);

    assert_eq!(run.code, Some(1), "{run:?}");
    assert!(
        run.stderr.contains(
            "note: the plugin in ~/.claude (user) has no version in plugins/installed_plugins.json after the update"
        ),
        "{run:?}"
    );
    assert!(!run.stderr.contains("Updated the plugin"), "{run:?}");
    assert!(!run.stderr.contains(RESTART), "{run:?}");
}

#[scenario("cli", "Update writes nothing to stdout")]
#[test]
fn update_writes_nothing_to_stdout_whether_it_works_or_fails() {
    let worked = Machine::new();
    worked.config(".claude", &[("user", None)]);
    let run = worked.run_bumping(&[]);
    assert_eq!(run.code, Some(0), "{run:?}");
    assert_eq!(run.stdout, "", "{run:?}");
    // The installer's lines are ours now, so they are on stderr.
    assert!(run.stderr.contains("is already installed at"), "{run:?}");

    let failed = Machine::new();
    let failing = failed.config(".claude-one", &[("user", None)]);
    let mut command = failed.update(&[]);
    command.env("FAKE_CLAUDE_FAIL", &failing);
    let run = ran(&mut command);
    assert_eq!(run.code, Some(1), "{run:?}");
    assert_eq!(run.stdout, "", "{run:?}");

    let named = Machine::new();
    named.config(".claude", &[("user", None)]);
    let run = named.run(&["0.1.99"]);
    assert_eq!(run.code, Some(0), "{run:?}");
    assert_eq!(run.stdout, "", "{run:?}");

    let bare = Machine::new();
    let run = bare.run(&[]);
    assert_eq!(run.code, Some(0), "{run:?}");
    assert_eq!(run.stdout, "", "{run:?}");
    assert!(
        run.stderr.contains("/plugin install gsttt@gsttt"),
        "{run:?}"
    );

    // The installer fails here: the newest release is not installed, and there is no package of it.
    let broken = Machine::new();
    broken.install_gsttt("0.1.98");
    let run = broken.run(&[]);
    assert_eq!(run.code, Some(1), "{run:?}");
    assert_eq!(run.stdout, "", "{run:?}");
}

#[scenario("cli", "The restart is asked for only after a change")]
#[test]
fn the_restart_is_asked_for_only_after_a_binary_was_installed_or_a_plugin_changed() {
    let machine = Machine::new();
    machine.config(".claude", &[("user", None)]);

    // Nothing is new: the binary is the newest, and so is the plugin.
    let run = machine.run(&[]);
    assert_eq!(run.code, Some(0), "{run:?}");
    assert!(!run.stderr.contains(RESTART), "{run:?}");

    // A binary is installed, and the plugin is the newest already.
    machine.install_gsttt("0.1.98");
    machine.release("0.1.99");
    let run = machine.run(&[]);
    assert_eq!(run.code, Some(0), "{run:?}");
    assert!(run.stderr.contains("Installed gsttt 0.1.99 to"), "{run:?}");
    assert!(run.stderr.contains("is already the newest"), "{run:?}");
    assert!(run.stderr.ends_with(RESTART), "{run:?}");

    // A binary is installed by name, and the plugin is left alone.
    machine.install_gsttt("0.1.98");
    let run = machine.run(&["0.1.99"]);
    assert_eq!(run.code, Some(0), "{run:?}");
    assert!(run.stderr.contains("Installed gsttt 0.1.99 to"), "{run:?}");
    assert!(run.stderr.ends_with(RESTART), "{run:?}");

    // A plugin changes, and the binary does not.
    let run = machine.run_bumping(&[]);
    assert_eq!(run.code, Some(0), "{run:?}");
    assert!(run.stderr.contains(&format!("{OLD} → {NEW}")), "{run:?}");
    assert!(run.stderr.ends_with(RESTART), "{run:?}");

    // A binary is installed where no config has the plugin, which has nothing to load.
    let bare = Machine::new();
    write(
        bare.home(),
        ".claude/plugins/installed_plugins.json",
        r#"{"version":2,"plugins":{"other@market":[]}}"#,
    );
    bare.install_gsttt("0.1.98");
    bare.release("0.1.99");
    let run = bare.run(&[]);
    assert_eq!(run.code, Some(0), "{run:?}");
    assert!(run.stderr.contains("Installed gsttt 0.1.99 to"), "{run:?}");
    assert!(!run.stderr.contains(RESTART), "{run:?}");
    bare.install_gsttt("0.1.98");
    let run = bare.run(&["0.1.99"]);
    assert_eq!(run.code, Some(0), "{run:?}");
    assert!(run.stderr.contains("Installed gsttt 0.1.99 to"), "{run:?}");
    assert!(!run.stderr.contains(RESTART), "{run:?}");
}
