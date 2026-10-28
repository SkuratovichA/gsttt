//! What every project of one run shares: the manifests, the workspace packages and the tsconfig chains, all read once
//! when the projects open.

use std::collections::BTreeMap;
use std::fmt;

use gsttt_core::RepoView;

use crate::config::{ConfigFile, Effective};
use crate::json::{self, Json};
use crate::manifest::Manifest;
use crate::options::TypeScriptOptions;
use crate::pathing::{
    ancestors, file_dir, file_name, has_segment, is_config_name, is_relative, join_dir, join_file,
    split_package, strip_dir,
};
use crate::source::is_code;

/// How many `extends` links are followed before a chain is given up.
const MAX_EXTENDS: usize = 16;

/// The conditions of a package `exports` map that are always active.
const CONDITIONS: [&str; 6] = ["types", "import", "module", "node", "default", "require"];

/// The repository as the resolver sees it.
pub(crate) struct Shared {
    pub(crate) repo: RepoView,
    /// Every `package.json`, by the directory it lies in (`""` for the root).
    pub(crate) manifests: BTreeMap<String, Manifest>,
    /// Workspace packages: the name in the manifest and the directory.
    pub(crate) packages: BTreeMap<String, String>,
    /// The readable tsconfig files with their `extends` chains folded in, by path.
    pub(crate) configs: BTreeMap<String, Effective>,
    /// The roots of the projects, in order.
    pub(crate) roots: Vec<String>,
    /// The conditions an `exports` or `imports` map is read with, in no particular order.
    pub(crate) conditions: Vec<String>,
    /// The config used for files no `tsconfig.json` governs.
    fallback: Option<String>,
}

impl fmt::Debug for Shared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shared")
            .field("roots", &self.roots)
            .field("packages", &self.packages)
            .finish_non_exhaustive()
    }
}

impl Shared {
    pub(crate) fn build(repo: &RepoView, options: &TypeScriptOptions) -> Self {
        let mut paths: Vec<&str> = repo
            .files()
            .map(|(_, path)| path.as_str())
            .filter(|path| !has_segment(path, "node_modules"))
            .collect();
        paths.sort_unstable();

        let mut manifests = BTreeMap::new();
        let mut raw = BTreeMap::new();
        let mut code_dirs = Vec::new();
        for path in paths {
            let name = file_name(path);
            if name == "package.json" {
                let manifest = read_json(repo, path)
                    .map(|json| Manifest::from_json(&json))
                    .unwrap_or_default();
                manifests.insert(file_dir(path).to_owned(), manifest);
            } else if is_config_name(name) {
                if let Some(json) = read_json(repo, path) {
                    raw.insert(path.to_owned(), ConfigFile::from_json(&json));
                }
            } else if is_code(name) {
                code_dirs.push(file_dir(path));
            }
        }

        let mut packages = BTreeMap::new();
        for (dir, manifest) in &manifests {
            if let Some(name) = &manifest.name {
                packages.entry(name.clone()).or_insert_with(|| dir.clone());
            }
        }

        let mut roots: Vec<String> = manifests.keys().cloned().collect();
        let uncovered = code_dirs
            .iter()
            .any(|dir| !roots.iter().any(|root| strip_dir(dir, root).is_some()));
        if uncovered {
            roots.push(String::new());
            roots.sort();
        }

        let mut conditions: Vec<String> =
            CONDITIONS.iter().map(|name| (*name).to_owned()).collect();
        for condition in &options.conditions {
            if !conditions.contains(condition) {
                conditions.push(condition.clone());
            }
        }

        let mut shared = Self {
            repo: repo.clone(),
            manifests,
            packages,
            configs: BTreeMap::new(),
            roots,
            conditions,
            fallback: options
                .tsconfig
                .as_deref()
                .and_then(|path| join_dir("", path)),
        };
        let configs = raw
            .keys()
            .filter_map(|path| {
                shared
                    .fold(path, &raw, &mut Vec::new())
                    .map(|effective| (path.clone(), effective))
            })
            .collect();
        shared.configs = configs;
        shared
    }

    /// The root of the project that owns `path`: the deepest one that contains it.
    pub(crate) fn owner(&self, path: &str) -> Option<&str> {
        self.roots
            .iter()
            .filter(|root| strip_dir(path, root).is_some())
            .max_by_key(|root| root.len())
            .map(String::as_str)
    }

    /// The options that govern files in `dir`: the nearest readable `tsconfig.json` upwards, else the configured one.
    pub(crate) fn config_for(&self, dir: &str) -> Option<&Effective> {
        ancestors(dir)
            .find_map(|dir| self.configs.get(&join_file(dir, "tsconfig.json")))
            .or_else(|| {
                self.fallback
                    .as_ref()
                    .and_then(|path| self.configs.get(path))
            })
    }

    /// The tsconfig an `extends` (or a reference) written in `from` stands for, when it is in the repository.
    pub(crate) fn extends_target(&self, from: &str, spec: &str) -> Option<String> {
        let base = if is_relative(spec) {
            join_dir(file_dir(from), spec)?
        } else {
            let (name, subpath) = split_package(spec)?;
            let package = self.packages.get(name)?;
            if subpath.is_empty() {
                package.clone()
            } else {
                join_dir(package, subpath)?
            }
        };
        [
            base.clone(),
            format!("{base}.json"),
            join_file(&base, "tsconfig.json"),
        ]
        .into_iter()
        .find(|candidate| self.repo.lookup(candidate).is_some())
    }

    /// The options of the config at `path` with everything it extends folded in; `None` for a config that did not read
    /// or a chain that loops.
    fn fold(
        &self,
        path: &str,
        raw: &BTreeMap<String, ConfigFile>,
        stack: &mut Vec<String>,
    ) -> Option<Effective> {
        let config = raw.get(path)?;
        if stack.len() >= MAX_EXTENDS || stack.iter().any(|seen| seen == path) {
            return None;
        }
        stack.push(path.to_owned());
        let mut effective = Effective::default();
        for spec in &config.extends {
            let parent = self
                .extends_target(path, spec)
                .and_then(|parent| self.fold(&parent, raw, stack));
            if let Some(parent) = parent {
                effective.overlay(&parent);
            }
        }
        stack.pop();
        effective.apply(config, file_dir(path));
        Some(effective)
    }
}

/// A JSON file of the repository, or `None` when it cannot be read or is not JSON; the file reports its own failure when
/// it is parsed.
fn read_json(repo: &RepoView, path: &str) -> Option<Json> {
    let text = match repo.read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            tracing::debug!(path, %error, "file is unreadable");
            return None;
        }
    };
    match json::parse(&text) {
        Ok(json) => Some(json),
        Err(error) => {
            tracing::debug!(path, line = error.line, message = %error.message, "file is not valid JSON");
            None
        }
    }
}
