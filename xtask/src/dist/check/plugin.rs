//! The plugin marketplace of the repository and the manifests of the plugins it lists.

use std::fmt::Display;
use std::io::ErrorKind;
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
        self.root_variables(plugin, file, manifest);
        Ok(())
    }

    /// Checks that `path`, which `field` of the manifest `file` names, exists in `plugin`. A JSON file named there holds hooks or servers, so its own references are checked too.
    fn named_path(
        &mut self,
        plugin: &Path,
        file: &Path,
        field: &str,
        path: &str,
    ) -> anyhow::Result<()> {
        let full = plugin.join(path.strip_prefix("./").unwrap_or(path));
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
            self.root_variables(plugin, &full, &configuration);
        }
        Ok(())
    }

    /// Checks that every `${CLAUDE_PLUGIN_ROOT}/<path>` anywhere in `value` names something that exists in `plugin`.
    fn root_variables(&mut self, plugin: &Path, file: &Path, value: &Value) {
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

/// Adds the paths that follow `${CLAUDE_PLUGIN_ROOT}/` in the strings of `value`: up to a space or a quote.
fn collect_root_paths(value: &Value, paths: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            for rest in text.split(ROOT_VARIABLE).skip(1) {
                let path = rest
                    .split(|character: char| {
                        character.is_whitespace() || matches!(character, '"' | '\'')
                    })
                    .next();
                paths.extend(path.filter(|path| !path.is_empty()).map(str::to_owned));
            }
        }
        Value::Array(items) => items
            .iter()
            .for_each(|item| collect_root_paths(item, paths)),
        Value::Object(map) => map
            .values()
            .for_each(|item| collect_root_paths(item, paths)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{collect_root_paths, strings};

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
    fn reads_a_field_that_is_a_string_or_a_list() {
        assert_eq!(strings(Some(&json!("./a"))), ["./a"]);
        assert_eq!(strings(Some(&json!(["./a", 3, "./b"]))), ["./a", "./b"]);
        let nothing: [&str; 0] = [];
        assert_eq!(strings(Some(&json!({ "inline": true }))), nothing);
        assert_eq!(strings(None), nothing);
    }
}
