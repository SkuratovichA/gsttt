//! The Claude Code half of `gsttt update`: which configs have the gsttt plugin, and the `claude` calls that bring it up to date.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde::Deserialize;
use serde_json::Value;

use super::RESTART;
use crate::error::CliError;
use crate::streams::{Io, Outcome};

/// The name of the plugin, which Claude Code files under `gsttt@<marketplace>`.
const PLUGIN: &str = "gsttt";

/// The variable that moves the config of Claude Code away from `~/.claude`.
const CONFIG_VARIABLE: &str = "CLAUDE_CONFIG_DIR";

/// What a person whose Claude Code has no gsttt plugin needs to add it.
const HOW_TO_ADD: &str = "No Claude Code config on this machine has the gsttt plugin. To add it, run these in Claude Code:\n  /plugin marketplace add SkuratovichA/gsttt\n  /plugin install gsttt@gsttt\n";

/// How many of the last lines `claude` printed a note quotes when a call failed.
const QUOTED_LINES: usize = 5;

/// One place the plugin is installed, as `installed_plugins.json` records it. Fields it does not name are ignored.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Install {
    /// `user`, or the scope of a project: `local`, `project`.
    scope: String,
    /// The project of an install that is not the user's.
    project_path: Option<PathBuf>,
    /// The version of the plugin as the file writes it: the start of the commit of the marketplace.
    version: Option<String>,
    /// The commit of the marketplace that the install was made from.
    git_commit_sha: Option<String>,
}

impl Install {
    /// Whether `other` is this install, the same scope of the same project, whatever version each records.
    fn is_where(&self, other: &Self) -> bool {
        self.scope == other.scope && self.project_path == other.project_path
    }
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

/// What `dir` records now for `install` of the plugin from `marketplace`, with its version: `None` when the file has no such install any more, or it has no version.
fn recorded_install(
    dir: &Path,
    marketplace: &str,
    install: &Install,
) -> Result<Option<(Install, String)>, ConfigError> {
    let Some(config) = read_config(dir)? else {
        return Ok(None);
    };
    Ok(config
        .installs
        .get(marketplace)
        .and_then(|installs| installs.iter().find(|found| found.is_where(install)))
        .and_then(|found| {
            found
                .version
                .clone()
                .map(|version| (found.clone(), version))
        }))
}

/// What a call of `claude` is for.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// Refreshes what the marketplace has, so that the plugin is updated to its newest commit.
    Marketplace(String),
    /// Updates `install`, as the file recorded it before the run, of the plugin from `marketplace`.
    Plugin {
        marketplace: String,
        install: Install,
    },
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
    step: Step,
}

impl Call {
    /// The directory the call runs in: the project, or else the home directory, or else, when the machine has no home or it is not a directory, the config directory.
    fn directory<'a>(&'a self, home: Option<&'a Path>) -> &'a Path {
        self.project
            .as_deref()
            .or(home.filter(|home| home.is_dir()))
            .unwrap_or(&self.config)
    }

    /// The call, ready to run: in `directory`, and with `CLAUDE_CONFIG_DIR` set to the config, or removed when the config is `~/.claude`, which is where Claude Code looks without it.
    fn command(&self, home: Option<&Path>) -> Command {
        let mut command = Command::new("claude");
        command.args(&self.args).current_dir(self.directory(home));
        if is_default(&self.config, home) {
            command.env_remove(CONFIG_VARIABLE);
        } else {
            command.env(CONFIG_VARIABLE, &self.config);
        }
        command
    }

    /// The call as a shell line, for a person to paste where `claude` is. It names the config either way: a shell that exports `CLAUDE_CONFIG_DIR` would send the call for `~/.claude` to another config if the line only left the variable out, so that line takes it away. The line of a project runs in a subshell, so that pasting several leaves the shell where it was.
    fn line(&self, home: Option<&Path>) -> String {
        let mut words = Vec::new();
        if is_default(&self.config, home) {
            words.push(format!("env -u {CONFIG_VARIABLE}"));
        } else {
            let config = quoted(&self.config.to_string_lossy()).into_owned();
            words.push(format!("{CONFIG_VARIABLE}={config}"));
        }
        words.push("claude".to_owned());
        words.extend(self.args.iter().map(|arg| quoted(arg).into_owned()));
        let line = words.join(" ");
        match &self.project {
            Some(project) => format!("(cd {} && {line})", quoted(&project.to_string_lossy())),
            None => line,
        }
    }

    /// Where the call acts, for the lines about it: the config, and for the plugin its scope and, when it has one, its project.
    fn place(&self, home: Option<&Path>) -> String {
        let config = tilde(&self.config, home);
        match (&self.step, &self.project) {
            (Step::Marketplace(_), _) => config,
            (Step::Plugin { install, .. }, Some(project)) => {
                format!("{config} ({}, {})", install.scope, tilde(project, home))
            }
            (Step::Plugin { install, .. }, None) => format!("{config} ({})", install.scope),
        }
    }

    /// What was left undone, for the note about a call that failed.
    fn undone(&self, home: Option<&Path>) -> String {
        let place = self.place(home);
        match &self.step {
            Step::Marketplace(name) => {
                format!("the marketplace {name} in {place} was not refreshed")
            }
            Step::Plugin { .. } => format!("the plugin in {place} was not updated"),
        }
    }
}

/// Whether `config` is `~/.claude`, the config Claude Code uses when `CLAUDE_CONFIG_DIR` is unset.
fn is_default(config: &Path, home: Option<&Path>) -> bool {
    home.is_some_and(|home| config == home.join(".claude"))
}

/// `path` for a person to read, the home directory written `~`. A line a shell runs keeps the whole path, because the shell does not expand a `~` it was given quoted.
fn tilde(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// `text` as one word of a shell line.
fn quoted(text: &str) -> Cow<'_, str> {
    shlex::try_quote(text).unwrap_or(Cow::Borrowed(text))
}

/// The calls that bring the plugin of `configs` up to date, and a note for each install that is left out. For each marketplace of a config the marketplace is updated first, so that the plugin is updated to its newest commit, and then each install is, in the scope it was made in. An install of a project that is not on disk any more is left out, as one that names no project is.
fn plan(configs: &[Config], home: Option<&Path>) -> (Vec<Call>, Vec<String>) {
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
                step: Step::Marketplace(marketplace.clone()),
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
                            left_out.push(left_out_note(
                                scope,
                                marketplace,
                                &tilde(&config.dir, home),
                                &project_problem(other),
                            ));
                            continue;
                        }
                    }
                };
                calls.push(Call {
                    config: config.dir.clone(),
                    project,
                    args: ["plugin", "update", &id, "--scope", scope]
                        .map(str::to_owned)
                        .to_vec(),
                    step: Step::Plugin {
                        marketplace: marketplace.clone(),
                        install: install.clone(),
                    },
                });
            }
        }
    }
    (calls, left_out)
}

/// Why an install cannot be updated from `project`: it names none, or the one it names is not on disk.
fn project_problem(project: Option<&Path>) -> String {
    project.map_or_else(
        || "it names no project".to_owned(),
        |path| format!("its project {} is not a directory", path.display()),
    )
}

/// The note for the install in `scope` of the plugin from `marketplace` in `config` that is not updated, for the reason `why`.
fn left_out_note(scope: &str, marketplace: &str, config: &str, why: &str) -> String {
    format!("left out the {scope} install of {PLUGIN}@{marketplace} in {config}: {why}")
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
            lines.push(format!("  {}", tilde(&call.config, home)));
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

/// The last `QUOTED_LINES` lines that `output` holds, the stdout and then the stderr, each indented under a line that says whose they are. Empty when `claude` printed nothing.
fn printed(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect();
    let last = lines.get(lines.len().saturating_sub(QUOTED_LINES)..);
    let mut text = String::new();
    for line in last.unwrap_or_default() {
        text.push_str("\n    ");
        text.push_str(line);
    }
    if text.is_empty() {
        text
    } else {
        format!("\n  claude printed:{text}")
    }
}

/// What came of a call.
enum Effect {
    /// It worked and changed nothing.
    Nothing,
    /// It worked and changed the version of the plugin.
    Changed,
    /// It failed, and was told.
    Failed,
    /// `claude` was not found.
    NoClaude,
}

/// The start of a commit hash, as far as a person tells commits apart.
fn short(commit: Option<&str>) -> &str {
    commit.map_or("none", |commit| commit.get(..12).unwrap_or(commit))
}

/// The line that says the plugin in `place` was updated, `before` being the install as the file had it before the call and `now` as it has it after, at `version`. `None` when neither its version nor its commit changed. An install that had no version is told by where it is now, since there is nothing to say it moved from.
fn updated_line(place: &str, before: &Install, now: &Install, version: &str) -> Option<String> {
    match before.version.as_deref() {
        None => Some(format!("Updated the plugin in {place} to {version}")),
        Some(old) if old != version => {
            Some(format!("Updated the plugin in {place}: {old} → {version}"))
        }
        Some(_) if before.git_commit_sha != now.git_commit_sha => Some(format!(
            "Updated the plugin in {place}: {version}, commit {} → {}",
            short(before.git_commit_sha.as_deref()),
            short(now.git_commit_sha.as_deref())
        )),
        Some(_) => None,
    }
}

/// Runs `call`, captures what `claude` prints, and tells what the call did: for the plugin the version it was and the version it is, as `installed_plugins.json` records them, and for a call that failed what `claude` printed last.
fn perform(call: &Call, home: Option<&Path>, io: &mut Io<'_>) -> Effect {
    if let Step::Marketplace(name) = &call.step {
        io.complain(
            format!(
                "Refreshing the marketplace {name} in {}\n",
                call.place(home)
            )
            .as_bytes(),
        );
    }
    // A project can go between the plan and its call. `spawn` says `NotFound` for a directory that is gone as it does for a program that is, so the directory is looked at first, and an install whose project is gone is left out as the plan leaves it out.
    match (&call.step, call.project.as_deref()) {
        (
            Step::Plugin {
                marketplace,
                install,
            },
            Some(project),
        ) if !project.is_dir() => {
            let config = tilde(&call.config, home);
            let why = project_problem(Some(project));
            note(
                io,
                &left_out_note(&install.scope, marketplace, &config, &why),
            );
            return Effect::Nothing;
        }
        _ => {}
    }
    let output = match call.command(home).output() {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Effect::NoClaude,
        Err(error) => {
            note(io, &format!("cannot run `{}`: {error}", call.line(home)));
            return Effect::Failed;
        }
    };
    if !output.status.success() {
        note(
            io,
            &format!(
                "{}: `{}` ended with {}{}",
                call.undone(home),
                call.line(home),
                output.status,
                printed(&output)
            ),
        );
        return Effect::Failed;
    }
    let Step::Plugin {
        marketplace,
        install,
    } = &call.step
    else {
        return Effect::Nothing;
    };

    let place = call.place(home);
    let (now, version) = match recorded_install(&call.config, marketplace, install) {
        Ok(Some(found)) => found,
        Ok(None) => {
            note(
                io,
                &format!(
                    "the plugin in {place} has no version in plugins/installed_plugins.json after the update"
                ),
            );
            return Effect::Failed;
        }
        Err(problem) => {
            note(io, &problem.to_string());
            return Effect::Failed;
        }
    };
    if let Some(line) = updated_line(&place, install, &now, &version) {
        io.complain(format!("{line}\n").as_bytes());
        Effect::Changed
    } else {
        io.complain(format!("The plugin in {place} is already the newest: {version}\n").as_bytes());
        Effect::Nothing
    }
}

/// Whether any Claude Code config on this machine has the plugin, which tells whether a new binary has a plugin to be loaded with.
pub(super) fn is_installed() -> bool {
    let home = std::env::home_dir();
    !find_configs(std::env::var_os(CONFIG_VARIABLE), home.as_deref())
        .0
        .is_empty()
}

/// Updates the plugin in every Claude Code config that has it, by running `claude`, and says so when no config has it. `installed` is whether the binary was replaced by another version, which running sessions have to be restarted for as much as for a plugin that changed, when there is a plugin for them to load.
///
/// A call that fails is told, and the calls after it still run, except the plugin calls of a marketplace that did not refresh, which would find the plugin newest in a stale copy of it. The run then ends with exit 1, as it does when `claude` is not found, which leaves the plugin out of step with the binary.
pub(super) fn update(io: &mut Io<'_>, installed: bool) -> Result<Outcome, CliError> {
    let home = std::env::home_dir();
    let (configs, problems) = find_configs(std::env::var_os(CONFIG_VARIABLE), home.as_deref());
    let mut failed = !problems.is_empty();
    for problem in &problems {
        note(io, &problem.to_string());
    }
    if configs.is_empty() && !failed {
        io.complain(HOW_TO_ADD.as_bytes());
    }

    let (calls, left_out) = plan(&configs, home.as_deref());
    for text in &left_out {
        note(io, text);
    }
    let mut changed = installed && !configs.is_empty();
    // The marketplaces that did not refresh, by the config they are in.
    let mut stale = BTreeSet::new();
    for (index, call) in calls.iter().enumerate() {
        let effect = match &call.step {
            Step::Plugin { marketplace, .. }
                if stale.contains(&(call.config.as_path(), marketplace.as_str())) =>
            {
                let place = call.place(home.as_deref());
                note(
                    io,
                    &format!(
                        "the plugin in {place} was not updated, because the marketplace {marketplace} was not refreshed"
                    ),
                );
                Effect::Failed
            }
            _ => perform(call, home.as_deref(), io),
        };
        match effect {
            Effect::Nothing => {}
            Effect::Changed => changed = true,
            Effect::Failed => {
                failed = true;
                if let Step::Marketplace(name) = &call.step {
                    stale.insert((call.config.as_path(), name.as_str()));
                }
            }
            Effect::NoClaude => {
                note(
                    io,
                    &left_as_it_was(calls.get(index..).unwrap_or_default(), home.as_deref()),
                );
                failed = true;
                break;
            }
        }
    }
    if changed {
        io.complain(RESTART.as_bytes());
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

    /// An install as the file records it, `version` being what it says.
    fn install(scope: &str, project: Option<&str>, version: Option<&str>) -> Install {
        Install {
            scope: scope.to_owned(),
            project_path: project.map(PathBuf::from),
            version: version.map(str::to_owned),
            git_commit_sha: None,
        }
    }

    /// `install` at the commit `sha`.
    fn at(sha: &str, install: Install) -> Install {
        Install {
            git_commit_sha: Some(sha.to_owned()),
            ..install
        }
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
                    at(
                        "9b6517d98ed1b8c917379dedc4fa592a6e07b605",
                        install("user", None, Some("9b6517d98ed1"))
                    ),
                    install("local", Some("/work/app"), Some("9b6517d98ed1")),
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
        fs::create_dir(home.path().join(".claude-backup")).unwrap();
        std::os::unix::fs::symlink(
            home.path().join(".claude/plugins"),
            home.path().join(".claude-backup/plugins"),
        )
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
                        project_path: Some(project.path().join("gone")),
                        ..install("local", None, None)
                    },
                    install("user", None, None),
                    Install {
                        project_path: Some(project.path().to_owned()),
                        ..install("local", None, None)
                    },
                    install("project", None, None),
                ],
            )]),
        };

        let (calls, left_out) = plan(&[config], Some(Path::new("/home/ann")));

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
                    "(cd {} && CLAUDE_CONFIG_DIR=/home/ann/.claude-work claude plugin update gsttt@gsttt --scope local)",
                    quoted(&project.path().to_string_lossy())
                ),
            ]
        );
        assert_eq!(left_out.len(), 2, "{left_out:?}");
        assert!(left_out.first().is_some_and(|text| text.contains("gone")));
        assert!(
            left_out
                .iter()
                .all(|text| text.contains("in ~/.claude-work:")),
            "{left_out:?}"
        );
        assert!(
            left_out
                .last()
                .is_some_and(|text| text.contains("names no project"))
        );
    }

    #[test]
    fn the_default_config_is_called_without_the_variable_and_its_line_takes_the_variable_away() {
        let call = Call {
            config: "/home/ann/.claude".into(),
            project: None,
            args: vec!["plugin".to_owned()],
            step: Step::Marketplace("gsttt".to_owned()),
        };
        let home = Some(Path::new("/home/ann"));

        assert_eq!(call.line(home), "env -u CLAUDE_CONFIG_DIR claude plugin");
        assert_eq!(
            call.line(None),
            "CLAUDE_CONFIG_DIR=/home/ann/.claude claude plugin"
        );
    }

    #[test]
    fn the_home_directory_is_written_as_a_tilde_where_a_person_reads_it() {
        let home = Some(Path::new("/home/ann"));
        for (path, shown) in [
            ("/home/ann", "~"),
            ("/home/ann/.claude", "~/.claude"),
            ("/home/ann/git/app", "~/git/app"),
            ("/home/annabel/.claude", "/home/annabel/.claude"),
            ("/srv/app", "/srv/app"),
        ] {
            assert_eq!(tilde(Path::new(path), home), shown, "{path}");
        }
        assert_eq!(
            tilde(Path::new("/home/ann/.claude"), None),
            "/home/ann/.claude"
        );
    }

    #[test]
    fn a_call_says_where_it_acts_and_what_was_left_undone_when_it_fails() {
        let home = Some(Path::new("/home/ann"));
        let marketplace = Call {
            config: "/home/ann/.claude-work".into(),
            project: None,
            args: Vec::new(),
            step: Step::Marketplace("gsttt".to_owned()),
        };
        let plugin = |scope: &str, project: Option<&str>| Call {
            config: "/home/ann/.claude".into(),
            project: project.map(PathBuf::from),
            args: Vec::new(),
            step: Step::Plugin {
                marketplace: "gsttt".to_owned(),
                install: install(scope, project, None),
            },
        };

        assert_eq!(marketplace.place(home), "~/.claude-work");
        assert_eq!(
            marketplace.undone(home),
            "the marketplace gsttt in ~/.claude-work was not refreshed"
        );
        assert_eq!(plugin("user", None).place(home), "~/.claude (user)");
        assert_eq!(
            plugin("local", Some("/home/ann/git/app")).place(home),
            "~/.claude (local, ~/git/app)"
        );
        assert_eq!(
            plugin("user", None).undone(home),
            "the plugin in ~/.claude (user) was not updated"
        );
    }

    #[test]
    fn only_the_last_five_lines_that_claude_printed_are_quoted() {
        let output = |stdout: &str, stderr: &str| Output {
            status: std::process::ExitStatus::default(),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        };

        assert_eq!(printed(&output("", "")), "");
        assert_eq!(printed(&output("\n  \n", "")), "");
        assert_eq!(
            printed(&output("one\ntwo\n", "")),
            "\n  claude printed:\n    one\n    two"
        );
        assert_eq!(
            printed(&output("1\n2\n3\n4\n", "5\n\n6\n7  \n")),
            "\n  claude printed:\n    3\n    4\n    5\n    6\n    7"
        );
    }

    #[test]
    fn an_install_is_found_again_by_its_scope_and_project_whatever_version_it_has_become() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "plugins/installed_plugins.json",
            r#"{"plugins":{"gsttt@gsttt":[
                {"scope":"user","version":"new"},
                {"scope":"local","projectPath":"/work/a","version":"a"},
                {"scope":"local","projectPath":"/work/b"}
            ]}}"#,
        );
        let version = |install: &Install| {
            recorded_install(dir.path(), "gsttt", install)
                .unwrap()
                .map(|(_, version)| version)
        };

        assert_eq!(
            version(&install("user", None, Some("old"))),
            Some("new".to_owned())
        );
        assert_eq!(
            version(&install("local", Some("/work/a"), None)),
            Some("a".to_owned())
        );
        // An install without a version, one that is not there and a marketplace that is not there are all no version.
        assert_eq!(version(&install("local", Some("/work/b"), None)), None);
        assert_eq!(version(&install("local", Some("/work/c"), None)), None);
        assert_eq!(
            recorded_install(dir.path(), "other", &install("user", None, None)).unwrap(),
            None
        );
    }

    #[test]
    fn a_line_with_a_project_runs_in_a_subshell_and_quotes_what_a_shell_would_split() {
        let call = Call {
            config: "/h/.claude it's".into(),
            project: Some("/h/my app's".into()),
            args: vec!["plugin".to_owned(), "update".to_owned()],
            step: Step::Marketplace("gsttt".to_owned()),
        };

        assert_eq!(
            call.line(Some(Path::new("/h"))),
            r#"(cd "/h/my app's" && CLAUDE_CONFIG_DIR="/h/.claude it's" claude plugin update)"#
        );
        let default = Call {
            config: "/h/.claude".into(),
            project: Some("/h/app".into()),
            ..call
        };
        assert_eq!(
            default.line(Some(Path::new("/h"))),
            "(cd /h/app && env -u CLAUDE_CONFIG_DIR claude plugin update)"
        );
    }

    #[test]
    fn a_call_runs_in_its_project_else_in_the_home_directory_else_in_its_config() {
        let home = tempfile::tempdir().unwrap();
        let call = |project: Option<&Path>| Call {
            config: "/c/config".into(),
            project: project.map(Path::to_path_buf),
            args: Vec::new(),
            step: Step::Marketplace("gsttt".to_owned()),
        };
        let file = home.path().join("file");
        fs::write(&file, "").unwrap();

        assert_eq!(
            call(Some(Path::new("/p/app"))).directory(Some(home.path())),
            Path::new("/p/app")
        );
        assert_eq!(call(None).directory(Some(home.path())), home.path());
        // No home, a home that is gone and a home that is a file all leave the config directory.
        assert_eq!(call(None).directory(None), Path::new("/c/config"));
        assert_eq!(
            call(None).directory(Some(&home.path().join("gone"))),
            Path::new("/c/config")
        );
        assert_eq!(call(None).directory(Some(&file)), Path::new("/c/config"));
    }

    #[test]
    fn an_install_is_told_updated_when_its_version_or_its_commit_changed_and_not_otherwise() {
        let sha = |sha: &str| {
            at(
                &format!("{sha}0123456789abcdef"),
                install("user", None, Some("0.1.50")),
            )
        };
        let versionless = install("user", None, None);
        let place = "~/.claude (user)";

        assert_eq!(
            updated_line(place, &sha("aaaaaaaaaaaa"), &sha("aaaaaaaaaaaa"), "0.1.50"),
            None
        );
        assert_eq!(
            updated_line(place, &sha("aaaaaaaaaaaa"), &sha("bbbbbbbbbbbb"), "0.1.51"),
            Some("Updated the plugin in ~/.claude (user): 0.1.50 → 0.1.51".to_owned())
        );
        assert_eq!(
            updated_line(place, &sha("aaaaaaaaaaaa"), &sha("bbbbbbbbbbbb"), "0.1.50"),
            Some(
                "Updated the plugin in ~/.claude (user): 0.1.50, commit aaaaaaaaaaaa → bbbbbbbbbbbb"
                    .to_owned()
            )
        );
        assert_eq!(
            updated_line(place, &versionless, &sha("bbbbbbbbbbbb"), "0.1.50"),
            Some("Updated the plugin in ~/.claude (user) to 0.1.50".to_owned())
        );
        // A commit that was not recorded before is a change too, and is told as "none".
        assert_eq!(
            updated_line(
                place,
                &install("user", None, Some("0.1.50")),
                &sha("bbbbbbbbbbbb"),
                "0.1.50"
            ),
            Some(
                "Updated the plugin in ~/.claude (user): 0.1.50, commit none → bbbbbbbbbbbb"
                    .to_owned()
            )
        );
    }

    #[test]
    fn a_project_that_cannot_be_used_is_told_by_what_is_wrong_with_it() {
        assert_eq!(project_problem(None), "it names no project");
        assert_eq!(
            project_problem(Some(Path::new("/work/gone"))),
            "its project /work/gone is not a directory"
        );
        assert_eq!(
            left_out_note("local", "gsttt", "~/.claude", "it names no project"),
            "left out the local install of gsttt@gsttt in ~/.claude: it names no project"
        );
    }
}
