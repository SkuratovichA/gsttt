//! The plugin marketplace of the repository and the manifests of the plugins it lists.

use std::fmt::Display;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use anyhow::Context as _;
use serde_json::Value;

/// The marketplace, from the repository root.
const MARKETPLACE: &str = ".claude-plugin/marketplace.json";

/// The manifest of a plugin, from the directory of the plugin.
const MANIFEST: &str = ".claude-plugin/plugin.json";

/// The manifest fields that name files or directories of the plugin.
const PATH_FIELDS: [&str; 7] = [
    "commands",
    "agents",
    "skills",
    "hooks",
    "mcpServers",
    "outputStyles",
    "lspServers",
];

/// How a manifest or a configuration file refers to the directory of its plugin.
const ROOT_VARIABLE: &str = "${CLAUDE_PLUGIN_ROOT}/";

/// The problems of the marketplace of the repository in `root` and of the plugins it lists from the repository.
pub(super) fn problems(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut check = Check {
        root,
        problems: Vec::new(),
    };
    check.marketplace()?;
    Ok(check.problems)
}

/// A check of the plugin files under `root`, collecting what it finds.
struct Check<'a> {
    root: &'a Path,
    problems: Vec<String>,
}

impl Check<'_> {
    /// Adds a problem with `file`, shown from the repository root.
    fn report(&mut self, file: &Path, message: impl Display) {
        let shown = file.strip_prefix(self.root).unwrap_or(file).display();
        self.problems.push(format!("{shown}: {message}"));
    }

    /// Parses `file`, or reports why it cannot be.
    fn read_json(&mut self, file: &Path) -> anyhow::Result<Option<Value>> {
        let text = match std::fs::read_to_string(file) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.report(file, "the file is missing");
                return Ok(None);
            }
            Err(error) => {
                return Err(error).with_context(|| format!("cannot read {}", file.display()));
            }
        };
        match serde_json::from_str(&text) {
            Ok(value) => Ok(Some(value)),
            Err(error) => {
                self.report(file, format!("is not valid JSON: {error}"));
                Ok(None)
            }
        }
    }

    fn marketplace(&mut self) -> anyhow::Result<()> {
        let file = self.root.join(MARKETPLACE);
        let Some(marketplace) = self.read_json(&file)? else {
            return Ok(());
        };
        let Some(listed) = marketplace.get("plugins").and_then(Value::as_array) else {
            self.report(&file, "`plugins` is missing or is not a list");
            return Ok(());
        };
        for plugin in listed {
            self.listed_plugin(&file, plugin)?;
        }
        Ok(())
    }

    /// Checks a plugin of the marketplace that lives in this repository. A plugin fetched from elsewhere names its source as an object and has no files here.
    fn listed_plugin(&mut self, marketplace: &Path, plugin: &Value) -> anyhow::Result<()> {
        let Some(source) = plugin.get("source").and_then(Value::as_str) else {
            return Ok(());
        };
        let name = plugin.get("name").and_then(Value::as_str).unwrap_or("?");
        let directory = self.root.join(source.strip_prefix("./").unwrap_or(source));
        if !directory.is_dir() {
            self.report(
                marketplace,
                format!("plugin `{name}` has the source `{source}`, which is not a directory"),
            );
            return Ok(());
        }
        let manifest_file = directory.join(MANIFEST);
        if let Some(manifest) = self.read_json(&manifest_file)? {
            self.manifest(&directory, &manifest_file, &manifest)?;
        }
        Ok(())
    }

    fn manifest(&mut self, plugin: &Path, file: &Path, manifest: &Value) -> anyhow::Result<()> {
        for field in PATH_FIELDS {
            for path in strings(manifest.get(field)) {
                self.named_path(plugin, file, field, path)?;
            }
        }
        self.root_variables(plugin, file, manifest)
    }

    /// Checks that `path`, which `field` of the manifest `file` names, starts with `./` and exists in `plugin`. A JSON file named there holds hooks or servers, so its own references are checked too.
    ///
    /// A skill must be a directory that holds `SKILL.md`, and a path must start with `./`: `claude plugin validate` (2.1.312) rejects a file and any other path.
    fn named_path(
        &mut self,
        plugin: &Path,
        file: &Path,
        field: &str,
        path: &str,
    ) -> anyhow::Result<()> {
        let Some(relative) = path.strip_prefix("./") else {
            self.report(
                file,
                format!(
                    "`{field}` names `{path}`, but a path of the manifest must start with `./`"
                ),
            );
            return Ok(());
        };
        let full = plugin.join(relative);
        if !full.exists() {
            self.report(
                file,
                format!("`{field}` names `{path}`, which does not exist"),
            );
        } else if field == "skills" && full.is_dir() && !full.join("SKILL.md").is_file() {
            self.report(file, format!("the skill `{path}` has no SKILL.md"));
        } else if full
            .extension()
            .is_some_and(|extension| extension == "json")
            && let Some(configuration) = self.read_json(&full)?
        {
            self.root_variables(plugin, &full, &configuration)?;
        }
        Ok(())
    }

    /// Checks that every `${CLAUDE_PLUGIN_ROOT}/<path>` anywhere in `value` names something that exists in `plugin`, and that every program a `command` runs from there is executable: git keeps the mode, and a script that lost it breaks the hook or the server without a word.
    fn root_variables(&mut self, plugin: &Path, file: &Path, value: &Value) -> anyhow::Result<()> {
        let mut paths = Vec::new();
        collect_root_paths(value, &mut paths);
        for path in paths {
            if !plugin.join(&path).exists() {
                self.report(
                    file,
                    format!("a command names `{ROOT_VARIABLE}{path}`, which does not exist"),
                );
            }
        }
        let mut programs = Vec::new();
        collect_programs(value, &mut programs);
        for program in programs {
            let full = plugin.join(&program);
            if full.is_file() && !is_executable(&full)? {
                self.report(
                    file,
                    format!("a command runs `{ROOT_VARIABLE}{program}`, which is not executable: set the mode with `chmod +x` and commit it"),
                );
            }
        }
        Ok(())
    }
}

/// The strings of a manifest field that holds a string or a list.
fn strings(field: Option<&Value>) -> Vec<&str> {
    match field {
        Some(Value::String(text)) => vec![text.as_str()],
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

/// The path at the start of `rest`, up to a space or a quote, or `None` when there is none.
fn root_path(rest: &str) -> Option<&str> {
    rest.split(|character: char| character.is_whitespace() || matches!(character, '"' | '\''))
        .next()
        .filter(|path| !path.is_empty())
}

/// Adds the paths that follow `${CLAUDE_PLUGIN_ROOT}/` in the strings of `value`.
fn collect_root_paths(value: &Value, paths: &mut Vec<String>) {
    match value {
        Value::String(text) => paths.extend(
            text.split(ROOT_VARIABLE)
                .skip(1)
                .filter_map(root_path)
                .map(str::to_owned),
        ),
        Value::Array(items) => items
            .iter()
            .for_each(|item| collect_root_paths(item, paths)),
        Value::Object(map) => map
            .values()
            .for_each(|item| collect_root_paths(item, paths)),
        _ => {}
    }
}

/// The path of the program that `command` runs, when it starts with `${CLAUDE_PLUGIN_ROOT}/`, quoted or not. A path later in the command is an argument or the script of an interpreter, and needs no executable bit.
fn program(command: &str) -> Option<&str> {
    let command = command.trim_start().trim_start_matches(['"', '\'']);
    root_path(command.strip_prefix(ROOT_VARIABLE)?)
}

/// Adds the programs that the `command` fields in `value` run from the plugin directory.
fn collect_programs(value: &Value, programs: &mut Vec<String>) {
    match value {
        Value::Array(items) => items
            .iter()
            .for_each(|item| collect_programs(item, programs)),
        Value::Object(map) => {
            for (key, item) in map {
                match item {
                    Value::String(command) if key == "command" => {
                        programs.extend(program(command).map(str::to_owned));
                    }
                    _ => collect_programs(item, programs),
                }
            }
        }
        _ => {}
    }
}

/// Whether any of the executable bits of the file at `path` is set.
fn is_executable(path: &Path) -> anyhow::Result<bool> {
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("cannot read the mode of {}", path.display()))?;
    Ok(metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{collect_programs, collect_root_paths, strings};

    #[test]
    fn finds_the_paths_after_the_plugin_root() {
        let value = json!({
            "command": "${CLAUDE_PLUGIN_ROOT}/scripts/run.sh hook start",
            "nested": [{ "args": ["--config", "\"${CLAUDE_PLUGIN_ROOT}/config.json\"", "plain"] }],
        });
        let mut paths = Vec::new();
        collect_root_paths(&value, &mut paths);
        paths.sort();
        assert_eq!(paths, ["config.json", "scripts/run.sh"]);
    }

    #[test]
    fn finds_the_programs_a_command_runs_from_the_plugin_root() {
        let value = json!({
            "hooks": [{ "command": "\"${CLAUDE_PLUGIN_ROOT}/scripts/start.sh\" hook start" }],
            "server": {
                "command": "${CLAUDE_PLUGIN_ROOT}/bin/server",
                "args": ["--config", "${CLAUDE_PLUGIN_ROOT}/config.json"],
            },
            "interpreted": { "command": "node ${CLAUDE_PLUGIN_ROOT}/server.js" },
            "elsewhere": { "command": "gsttt mcp" },
            "not_a_command": "${CLAUDE_PLUGIN_ROOT}/notes.md",
        });
        let mut programs = Vec::new();
        collect_programs(&value, &mut programs);
        programs.sort();
        assert_eq!(programs, ["bin/server", "scripts/start.sh"]);
    }

    #[test]
    fn reads_a_field_that_is_a_string_or_a_list() {
        assert_eq!(strings(Some(&json!("./a"))), ["./a"]);
        assert_eq!(strings(Some(&json!(["./a", 3, "./b"]))), ["./a", "./b"]);
        let nothing: [&str; 0] = [];
        assert_eq!(strings(Some(&json!({ "inline": true }))), nothing);
        assert_eq!(strings(None), nothing);
    }
}
