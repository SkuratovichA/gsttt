//! The Claude Code half of `gsttt update`: which configs have the gsttt plugin, and the `claude` calls that bring it up to date.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use serde_json::Value;

use crate::error::CliError;
use crate::streams::{Io, Outcome};

/// The name of the plugin, which Claude Code files under `gsttt@<marketplace>`.
const PLUGIN: &str = "gsttt";

/// The variable that moves the config of Claude Code away from `~/.claude`.
const CONFIG_VARIABLE: &str = "CLAUDE_CONFIG_DIR";

/// What a person whose Claude Code has no gsttt plugin needs to add it.
const HOW_TO_ADD: &str = "No Claude Code config on this machine has the gsttt plugin. To add it, run these in Claude Code:\n  /plugin marketplace add SkuratovichA/gsttt\n  /plugin install gsttt@gsttt\n";

/// The last line after an update: a session that is running keeps the plugin it loaded when it started.
const RESTART: &str =
    "Restart the Claude Code sessions that are running to load the updated plugin.\n";

/// One place the plugin is installed, as `installed_plugins.json` records it. Fields it does not name are ignored.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Install {
    /// `user`, or the scope of a project: `local`, `project`.
    scope: String,
    /// The project of an install that is not the user's.
    project_path: Option<PathBuf>,
}

/// A Claude Code config directory that has the gsttt plugin.
#[derive(Debug, PartialEq, Eq)]
struct Config {
    dir: PathBuf,
    /// The installs of the plugin by the marketplace they came from, each list in the order of the file.
    installs: BTreeMap<String, Vec<Install>>,
}

/// The part of `plugins/installed_plugins.json` that this command reads: the installs by `<plugin>@<marketplace>`. The installs of other plugins are not looked into, so a change in how Claude Code writes them cannot break the update of this one.
#[derive(Debug, Deserialize)]
struct InstalledPlugins {
    #[serde(default)]
    plugins: BTreeMap<String, Value>,
}

/// Why a config cannot be told to have the plugin or not.
#[derive(Debug, thiserror::Error)]
enum ConfigError {
    #[error("cannot read {}: {source}", .path.display())]
    Read { path: PathBuf, source: io::Error },

    #[error("{} is not the JSON that Claude Code writes: {source}", .path.display())]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("the installs of `{key}` in {} are not what Claude Code writes: {source}", .path.display())]
    Installs {
        path: PathBuf,
        key: String,
        source: serde_json::Error,
    },
}

/// Reads the installs of the gsttt plugin in the config `dir`. A file that is not there, one without a `plugins` object and one without the plugin all mean there is none, and are not told.
fn read_config(dir: &Path) -> Result<Option<Config>, ConfigError> {
    let path = dir.join("plugins/installed_plugins.json");
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(ConfigError::Read { path, source }),
    };
    let file: InstalledPlugins =
        serde_json::from_slice(&bytes).map_err(|source| ConfigError::Json {
            path: path.clone(),
            source,
        })?;
    let mut installs = BTreeMap::new();
    for (key, value) in file.plugins {
        let Some(marketplace) = key
            .strip_prefix(PLUGIN)
            .and_then(|rest| rest.strip_prefix('@'))
        else {
            continue;
        };
        let list = serde_json::from_value(value).map_err(|source| ConfigError::Installs {
            path: path.clone(),
            key: key.clone(),
            source,
        })?;
        installs.insert(marketplace.to_owned(), list);
    }
    Ok((!installs.is_empty()).then(|| Config {
        dir: dir.to_owned(),
        installs,
    }))
}

/// The configs that have the plugin, in the order of their paths, and what stopped a config from being read.
///
/// The candidates are `variable`, the value of `CLAUDE_CONFIG_DIR`, and every directory in `home` whose name starts with `.claude`. Two candidates whose `plugins` directories are one directory on disk are one config, as a link from `.claude-backup/plugins` to `.claude/plugins` makes them, and the one whose path sorts first is the one kept.
fn find_configs(
    variable: Option<OsString>,
    home: Option<&Path>,
) -> (Vec<Config>, Vec<ConfigError>) {
    let mut problems = Vec::new();
    let mut candidates = Vec::new();
    if let Some(directory) = variable.filter(|value| !value.is_empty()) {
        // The calls run in other directories, where a relative path would name something else.
        let directory = PathBuf::from(directory);
        candidates.push(std::path::absolute(&directory).unwrap_or(directory));
    }
    if let Some(home) = home {
        match fs::read_dir(home) {
            Ok(entries) => candidates.extend(
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_name().as_bytes().starts_with(b".claude"))
                    .map(|entry| entry.path())
                    .filter(|path| path.is_dir()),
            ),
            Err(source) => problems.push(ConfigError::Read {
                path: home.to_owned(),
                source,
            }),
        }
    }
    candidates.sort_by(|a, b| a.as_os_str().as_bytes().cmp(b.as_os_str().as_bytes()));
    candidates.dedup();

    let mut seen = BTreeSet::new();
    let mut configs = Vec::new();
    for dir in candidates {
        // A config with no `plugins` directory has no plugin.
        let Ok(plugins) = fs::canonicalize(dir.join("plugins")) else {
            continue;
        };
        if !seen.insert(plugins) {
            continue;
        }
        match read_config(&dir) {
            Ok(Some(config)) => configs.push(config),
            Ok(None) => {}
            Err(problem) => problems.push(problem),
        }
    }
    (configs, problems)
}

/// One call of the `claude` command.
#[derive(Debug, PartialEq, Eq)]
struct Call {
    /// The config the call is for.
    config: PathBuf,
    /// The project an install that is not the user's belongs to, which the call has to run in. Any other call runs in the home directory.
    project: Option<PathBuf>,
    /// The arguments after `claude`.
    args: Vec<String>,
    /// What the line before the call says it does.
    what: String,
    /// Whether the call updates the plugin, and not only the marketplace's list of what is new.
    updates_plugin: bool,
}

impl Call {
    /// The call, ready to run: in the project or else the home directory (the config directory when the machine has no home), and with `CLAUDE_CONFIG_DIR` set to the config, or removed when the config is `~/.claude`, which is where Claude Code looks without it.
    fn command(&self, home: Option<&Path>) -> Command {
        let mut command = Command::new("claude");
        command
            .args(&self.args)
            .current_dir(self.project.as_deref().or(home).unwrap_or(&self.config));
        if is_default(&self.config, home) {
            command.env_remove(CONFIG_VARIABLE);
        } else {
            command.env(CONFIG_VARIABLE, &self.config);
        }
        command
    }

    /// The call as a shell line, for a person to run where `claude` is.
    fn line(&self, home: Option<&Path>) -> String {
        let mut words = Vec::new();
        if let Some(project) = &self.project {
            words.push(format!("cd {} &&", quoted(&project.to_string_lossy())));
        }
        if !is_default(&self.config, home) {
            let config = quoted(&self.config.to_string_lossy()).into_owned();
            words.push(format!("{CONFIG_VARIABLE}={config}"));
        }
        words.push("claude".to_owned());
        words.extend(self.args.iter().map(|arg| quoted(arg).into_owned()));
        words.join(" ")
    }
}

/// Whether `config` is `~/.claude`, the config Claude Code uses when `CLAUDE_CONFIG_DIR` is unset.
fn is_default(config: &Path, home: Option<&Path>) -> bool {
    home.is_some_and(|home| config == home.join(".claude"))
}

/// `text` as one word of a shell line.
fn quoted(text: &str) -> Cow<'_, str> {
    shlex::try_quote(text).unwrap_or(Cow::Borrowed(text))
}

/// The calls that bring the plugin of `configs` up to date, and a note for each install that is left out. For each marketplace of a config the marketplace is updated first, so that the plugin is updated to its newest commit, and then each install is, in the scope it was made in. An install of a project that is not on disk any more is left out, as one that names no project is.
fn plan(configs: &[Config]) -> (Vec<Call>, Vec<String>) {
    let mut calls = Vec::new();
    let mut left_out = Vec::new();
    for config in configs {
        for (marketplace, installs) in &config.installs {
            calls.push(Call {
                config: config.dir.clone(),
                project: None,
                args: ["plugin", "marketplace", "update", marketplace]
                    .map(str::to_owned)
                    .to_vec(),
                what: format!("the marketplace {marketplace}"),
                updates_plugin: false,
            });
            let id = format!("{PLUGIN}@{marketplace}");
            for install in installs {
                let scope = &install.scope;
                let project = if scope == "user" {
                    None
                } else {
                    match install.project_path.as_deref() {
                        Some(path) if path.is_dir() => Some(path.to_owned()),
                        other => {
                            let why = other.map_or_else(
                                || "it names no project".to_owned(),
                                |path| format!("its project {} is not a directory", path.display()),
                            );
                            left_out.push(format!(
                                "left out the {scope} install of {id} in {}: {why}",
                                config.dir.display()
                            ));
                            continue;
                        }
                    }
                };
                let what = match &project {
                    Some(project) => {
                        format!("{id}, {scope} scope, project {}", project.display())
                    }
                    None => format!("{id}, {scope} scope"),
                };
                calls.push(Call {
                    config: config.dir.clone(),
                    project,
                    args: ["plugin", "update", &id, "--scope", scope]
                        .map(str::to_owned)
                        .to_vec(),
                    what,
                    updates_plugin: true,
                });
            }
        }
    }
    (calls, left_out)
}

/// Tells something that went wrong, or was left out, without ending the run.
fn note(io: &mut Io<'_>, text: &str) {
    io.complain(format!("note: {text}\n").as_bytes());
}

/// Names the configs `calls` are for, each with the lines that do by hand what the calls would have done.
fn left_as_it_was(calls: &[Call], home: Option<&Path>) -> String {
    let mut lines = Vec::new();
    let mut configs = 0;
    let mut previous = None;
    for call in calls {
        if previous != Some(&call.config) {
            configs += 1;
            lines.push(format!("  {}", call.config.display()));
            previous = Some(&call.config);
        }
        lines.push(format!("    {}", call.line(home)));
    }
    let many = if configs == 1 { "config" } else { "configs" };
    format!(
        "`claude` is not on PATH, so the plugin was left as it was in {configs} {many}. Where `claude` is, run:\n{}",
        lines.join("\n")
    )
}

/// Updates the plugin in every Claude Code config that has it, by running `claude`, and says so when no config has it.
///
/// A call that fails is told, and the calls after it still run; the run then ends with exit 1, as it does when `claude` is not found, which leaves the plugin out of step with the binary.
pub(super) fn update(io: &mut Io<'_>) -> Result<Outcome, CliError> {
    let home = std::env::home_dir();
    let (configs, problems) = find_configs(std::env::var_os(CONFIG_VARIABLE), home.as_deref());
    let mut failed = !problems.is_empty();
    for problem in &problems {
        note(io, &problem.to_string());
    }
    if configs.is_empty() {
        if failed {
            return Err(CliError::UpdatePlugin);
        }
        io.answer(HOW_TO_ADD.as_bytes())?;
        return Ok(Outcome::Answered);
    }

    let (calls, left_out) = plan(&configs);
    for text in &left_out {
        note(io, text);
    }
    let mut updated = 0;
    for (index, call) in calls.iter().enumerate() {
        io.answer(format!("{}: updating {}\n", call.config.display(), call.what).as_bytes())?;
        match call.command(home.as_deref()).status() {
            Ok(status) if status.success() => updated += usize::from(call.updates_plugin),
            Ok(status) => {
                note(
                    io,
                    &format!("`{}` ended with {status}", call.line(home.as_deref())),
                );
                failed = true;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                note(
                    io,
                    &left_as_it_was(calls.get(index..).unwrap_or_default(), home.as_deref()),
                );
                failed = true;
                break;
            }
            Err(error) => {
                note(
                    io,
                    &format!("cannot run `{}`: {error}", call.line(home.as_deref())),
                );
                failed = true;
            }
        }
    }
    if updated > 0 {
        io.answer(RESTART.as_bytes())?;
    }
    if failed {
        Err(CliError::UpdatePlugin)
    } else {
        Ok(Outcome::Answered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, path: &str, text: &str) {
        let file = dir.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }

    const INSTALLED: &str = r#"{"version":2,"plugins":{
        "other@market":"not even a list",
        "gsttt@gsttt":[
            {"scope":"user","installPath":"/x","version":"9b6517d98ed1","gitCommitSha":"9b6517d98ed1b8c917379dedc4fa592a6e07b605"},
            {"scope":"local","projectPath":"/work/app","version":"9b6517d98ed1"}
        ]}}"#;

    #[test]
    fn the_installs_of_the_plugin_are_read_and_the_rest_of_the_file_is_not_looked_into() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "plugins/installed_plugins.json", INSTALLED);

        let config = read_config(dir.path()).unwrap().unwrap();

        assert_eq!(
            config.installs,
            BTreeMap::from([(
                "gsttt".to_owned(),
                vec![
                    Install {
                        scope: "user".to_owned(),
                        project_path: None
                    },
                    Install {
                        scope: "local".to_owned(),
                        project_path: Some("/work/app".into())
                    },
                ]
            )])
        );
    }

    #[test]
    fn a_config_without_the_plugin_is_none_and_says_nothing() {
        for (name, text) in [
            ("no file", None),
            ("no plugins object", Some(r#"{"version":2}"#)),
            ("other plugins", Some(r#"{"plugins":{"other@gsttt":[]}}"#)),
        ] {
            let dir = tempfile::tempdir().unwrap();
            if let Some(text) = text {
                write(dir.path(), "plugins/installed_plugins.json", text);
            }
            assert_eq!(read_config(dir.path()).unwrap(), None, "{name}");
        }
    }

    #[test]
    fn a_file_that_is_not_json_is_told_by_its_name() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "plugins/installed_plugins.json", "{ not json");

        let error = read_config(dir.path()).unwrap_err();

        assert!(matches!(error, ConfigError::Json { .. }), "{error:?}");
        let file = dir.path().join("plugins/installed_plugins.json");
        assert!(
            error.to_string().contains(&file.display().to_string()),
            "{error}"
        );
    }

    #[test]
    fn installs_that_are_not_a_list_are_told_by_the_key() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "plugins/installed_plugins.json",
            r#"{"plugins":{"gsttt@gsttt":{"scope":"user"}}}"#,
        );

        let error = read_config(dir.path()).unwrap_err();

        assert!(error.to_string().contains("`gsttt@gsttt`"), "{error}");
    }

    #[test]
    fn a_config_that_shares_its_plugins_with_an_earlier_one_is_left_out() {
        let home = tempfile::tempdir().unwrap();
        write(
            home.path(),
            ".claude/plugins/installed_plugins.json",
            INSTALLED,
        );
        std::os::unix::fs::symlink(home.path().join(".claude/plugins"), {
            fs::create_dir(home.path().join(".claude-backup")).unwrap();
            home.path().join(".claude-backup/plugins")
        })
        .unwrap();
        write(
            home.path(),
            ".claude-work/plugins/installed_plugins.json",
            INSTALLED,
        );
        write(
            home.path(),
            ".claude-none/plugins/installed_plugins.json",
            "{}",
        );
        write(home.path(), ".claude.json", "{}");
        write(
            home.path(),
            "other/plugins/installed_plugins.json",
            INSTALLED,
        );

        let (configs, problems) = find_configs(None, Some(home.path()));

        assert!(problems.is_empty(), "{problems:?}");
        let found: Vec<_> = configs.iter().map(|config| config.dir.clone()).collect();
        assert_eq!(
            found,
            [
                home.path().join(".claude"),
                home.path().join(".claude-work")
            ]
        );
    }

    #[test]
    fn a_stale_project_is_left_out_and_the_marketplace_is_updated_first() {
        let project = tempfile::tempdir().unwrap();
        let config = Config {
            dir: "/home/ann/.claude-work".into(),
            installs: BTreeMap::from([(
                "gsttt".to_owned(),
                vec![
                    Install {
                        scope: "local".to_owned(),
                        project_path: Some(project.path().join("gone")),
                    },
                    Install {
                        scope: "user".to_owned(),
                        project_path: None,
                    },
                    Install {
                        scope: "local".to_owned(),
                        project_path: Some(project.path().to_owned()),
                    },
                    Install {
                        scope: "project".to_owned(),
                        project_path: None,
                    },
                ],
            )]),
        };

        let (calls, left_out) = plan(&[config]);

        let lines: Vec<_> = calls
            .iter()
            .map(|call| call.line(Some(Path::new("/home/ann"))))
            .collect();
        assert_eq!(
            lines,
            [
                "CLAUDE_CONFIG_DIR=/home/ann/.claude-work claude plugin marketplace update gsttt"
                    .to_owned(),
                "CLAUDE_CONFIG_DIR=/home/ann/.claude-work claude plugin update gsttt@gsttt --scope user"
                    .to_owned(),
                format!(
                    "cd {} && CLAUDE_CONFIG_DIR=/home/ann/.claude-work claude plugin update gsttt@gsttt --scope local",
                    quoted(&project.path().to_string_lossy())
                ),
            ]
        );
        assert_eq!(left_out.len(), 2, "{left_out:?}");
        assert!(left_out.first().is_some_and(|text| text.contains("gone")));
        assert!(
            left_out
                .last()
                .is_some_and(|text| text.contains("names no project"))
        );
    }

    #[test]
    fn the_default_config_is_called_without_the_variable() {
        let call = Call {
            config: "/home/ann/.claude".into(),
            project: None,
            args: vec!["plugin".to_owned()],
            what: String::new(),
            updates_plugin: false,
        };
        let home = Some(Path::new("/home/ann"));

        assert_eq!(call.line(home), "claude plugin");
        assert_eq!(
            call.line(None),
            "CLAUDE_CONFIG_DIR=/home/ann/.claude claude plugin"
        );
    }
}
