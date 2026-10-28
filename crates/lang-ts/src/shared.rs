//! What every project of one run shares: the manifests, the workspace packages and the tsconfig chains, all read once when the projects open.

use std::collections::BTreeMap;
use std::fmt;

use gsttt_core::tree::{self, Value};
use gsttt_core::{RepoPath, RepoView};

use crate::config::{ConfigFile, Cover, Effective};
use crate::manifest::Manifest;
use crate::options::TypeScriptOptions;
use crate::pathing::{
    ancestors, file_dir, has_segment, is_config_name, is_relative, join_file, split_package,
    strip_dir,
};
use crate::source::is_code;

/// How many `extends` links are followed before a chain is given up.
const MAX_EXTENDS: usize = 16;

/// The conditions of a package `exports` map that are always active. The form of an import adds `import` or `require`.
const CONDITIONS: [&str; 2] = ["types", "default"];

/// The names a config may have in a directory, in the order they are tried.
const CONFIG_NAMES: [&str; 2] = ["tsconfig.json", "jsconfig.json"];

/// The repository as the resolver sees it.
pub(crate) struct Shared {
    pub(crate) repo: RepoView,
    /// Every `package.json`, by the directory it lies in (`""` for the root).
    pub(crate) manifests: BTreeMap<String, Manifest>,
    /// Workspace packages: the name in the manifest and the directory.
    pub(crate) packages: BTreeMap<String, String>,
    /// The readable tsconfig and jsconfig files with their `extends` chains folded in, by path.
    pub(crate) configs: BTreeMap<String, Effective>,
    /// The roots of the projects, sorted.
    pub(crate) roots: Vec<String>,
    /// The conditions an `exports` or `imports` map is read with besides the form of the import, in no particular order.
    pub(crate) conditions: Vec<String>,
    /// The config used for files no `tsconfig.json` or `jsconfig.json` governs.
    fallback: Option<String>,
    /// For a solution-style config, one that has `references`: the configs it lists, in the order it lists them.
    solutions: BTreeMap<String, Vec<String>>,
    /// The files each listed config covers, by config path.
    covers: BTreeMap<String, Cover>,
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
        let mut paths: Vec<&RepoPath> = repo
            .files()
            .map(|(_, path)| path)
            .filter(|path| !has_segment(path.as_str(), "node_modules"))
            .collect();
        paths.sort_unstable();

        let mut manifests = BTreeMap::new();
        let mut raw = BTreeMap::new();
        let mut code_dirs = Vec::new();
        for path in paths {
            let name = path.file_name();
            if name == "package.json" {
                let manifest = read_json(repo, path.as_str())
                    .map(|json| Manifest::from_json(&json))
                    .unwrap_or_default();
                manifests.insert(path.parent().to_owned(), manifest);
            } else if is_config_name(name) {
                if let Some(json) = read_json(repo, path.as_str()) {
                    raw.insert(path.as_str().to_owned(), ConfigFile::from_json(&json));
                }
            } else if is_code(name) {
                code_dirs.push(path.parent());
            }
        }

        let mut packages = BTreeMap::new();
        for (dir, manifest) in &manifests {
            if let Some(name) = &manifest.name {
                packages.entry(name.clone()).or_insert_with(|| dir.clone());
            }
        }

        // A `package.json` opens a project when it is at the repository root or declares a package of its own. A marker such as `{"type": "module"}` only sets the module type of the directory it lies in.
        let mut roots: Vec<String> = manifests
            .iter()
            .filter(|(dir, manifest)| dir.is_empty() || manifest.declares_project)
            .map(|(dir, _)| dir.clone())
            .collect();
        let uncovered = code_dirs
            .iter()
            .any(|dir| !roots.iter().any(|root| strip_dir(dir, root).is_some()));
        if uncovered {
            roots.push(String::new());
        }
        roots.sort();

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
                .and_then(|path| RepoPath::join_dir("", path)),
            solutions: BTreeMap::new(),
            covers: BTreeMap::new(),
        };
        // A config reached by `extends` or `references` may have any name, such as `base.json` in a package of shared configs, so those files are read although no config name matches them.
        let mut pending: Vec<String> = raw.keys().cloned().collect();
        while let Some(path) = pending.pop() {
            let targets: Vec<String> = raw
                .get(&path)
                .into_iter()
                .flat_map(|config| config.extends.iter().chain(&config.references))
                .filter_map(|spec| shared.extends_target(&path, spec))
                .collect();
            for target in targets {
                if raw.contains_key(&target) {
                    continue;
                }
                if let Some(json) = read_json(repo, &target) {
                    raw.insert(target.clone(), ConfigFile::from_json(&json));
                    pending.push(target);
                }
            }
        }
        let configs = raw
            .keys()
            .filter_map(|path| {
                shared
                    .fold(path, &raw, &mut Vec::new())
                    .map(|effective| (path.clone(), effective))
            })
            .collect();
        shared.configs = configs;

        let mut solutions = BTreeMap::new();
        let mut covers = BTreeMap::new();
        for (path, config) in &raw {
            let listed: Vec<String> = config
                .references
                .iter()
                .filter_map(|spec| shared.extends_target(path, spec))
                .filter(|target| shared.configs.contains_key(target))
                .collect();
            for target in &listed {
                if let Some(effective) = shared.configs.get(target) {
                    covers
                        .entry(target.clone())
                        .or_insert_with(|| Cover::of(file_dir(target), effective));
                }
            }
            if !listed.is_empty() {
                solutions.insert(path.clone(), listed);
            }
        }
        shared.solutions = solutions;
        shared.covers = covers;
        shared
    }

    /// The root of the project that owns `path`: the deepest one that contains it.
    pub(crate) fn owner(&self, path: &RepoPath) -> Option<&str> {
        self.roots
            .iter()
            .filter(|root| path.is_under(root))
            .max_by_key(|root| root.len())
            .map(String::as_str)
    }

    /// The manifests of the project rooted at `root` and of the projects around it, nearest first and the repository root last: the places a bare import and the `detect` list of a pack look for a dependency.
    pub(crate) fn manifests_above<'a>(
        &'a self,
        root: &'a str,
    ) -> impl Iterator<Item = &'a Manifest> + 'a {
        ancestors(root).filter_map(move |dir| {
            self.roots
                .binary_search_by(|candidate| candidate.as_str().cmp(dir))
                .ok()?;
            self.manifests.get(dir)
        })
    }

    /// The path of the config nearest above `dir`: in each directory `tsconfig.json`, else `jsconfig.json`; the configured one when there is none.
    fn nearest(&self, dir: &str) -> Option<&str> {
        ancestors(dir)
            .find_map(|dir| {
                CONFIG_NAMES.iter().find_map(|name| {
                    self.configs
                        .get_key_value(&join_file(dir, name))
                        .map(|(key, _)| key.as_str())
                })
            })
            .or_else(|| {
                let fallback = self.fallback.as_deref()?;
                self.configs
                    .get_key_value(fallback)
                    .map(|(key, _)| key.as_str())
            })
    }

    /// The options that govern files in `dir`: those of the nearest readable config upwards, else the configured one.
    pub(crate) fn config_for(&self, dir: &str) -> Option<&Effective> {
        self.nearest(dir).and_then(|key| self.configs.get(key))
    }

    /// The options that govern the file at `path`. A nearest config that only refers to others (a solution) hands the file to the first config it lists, in listed order, that covers the file; otherwise the nearest config governs.
    pub(crate) fn config_of(&self, path: &str) -> Option<&Effective> {
        let key = self.nearest(file_dir(path))?;
        let listed = self
            .solutions
            .get(key)
            .into_iter()
            .flatten()
            .find(|config| {
                self.covers
                    .get(*config)
                    .is_some_and(|cover| cover.covers(path))
            });
        self.configs.get(listed.map_or(key, String::as_str))
    }

    /// The tsconfig an `extends` (or a reference) written in `from` stands for, when it is in the repository.
    pub(crate) fn extends_target(&self, from: &str, spec: &str) -> Option<String> {
        let base = if is_relative(spec) {
            RepoPath::join_dir(file_dir(from), spec)?
        } else {
            let (name, subpath) = split_package(spec)?;
            let package = self.packages.get(name)?;
            if subpath.is_empty() {
                package.clone()
            } else {
                RepoPath::join_dir(package, subpath)?
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

    /// The options of the config at `path` with everything it extends folded in; `None` for a config that did not read or a chain that loops.
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

/// A JSON file of the repository, or `None` when it cannot be read or is not JSON; the file reports its own failure when it is parsed.
fn read_json(repo: &RepoView, path: &str) -> Option<Value> {
    let text = match repo.read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            tracing::debug!(path, %error, "file is unreadable");
            return None;
        }
    };
    match tree::parse_json(&text) {
        Ok(json) => Some(json),
        Err(error) => {
            tracing::debug!(path, line = error.line, message = %error.message, "file is not valid JSON");
            None
        }
    }
}
