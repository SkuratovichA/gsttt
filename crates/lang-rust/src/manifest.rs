//! Cargo manifests: a package with its crate targets and dependencies, and what a workspace root shares.

use std::collections::{BTreeMap, HashSet};

use gsttt_core::path::{PathError, RepoPath};
use gsttt_core::repo::RepoView;
use toml::{Table, Value};

/// The kinds of crate a package builds, in the order a file that belongs to several of them is attributed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TargetKind {
    Lib,
    Bin,
    Example,
    Test,
    Bench,
    Build,
}

/// One crate target: the file its module tree starts from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) kind: TargetKind,
    pub(crate) name: String,
    pub(crate) root: String,
}

/// Which targets of a package see a dependency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Section {
    /// `[dependencies]` and `[dev-dependencies]`: every target but the build script. A dev-dependency is visible to
    /// all of them because `#[cfg(test)]` code in `src/` uses it and `cfg` is not evaluated.
    Normal,
    /// `[build-dependencies]`: the build script, and nothing else.
    Build,
}

/// One dependency of a package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Dependency {
    /// The key in the manifest: the name `use` paths start with, once `-` becomes `_`, unless `lib_name` is set.
    pub(crate) key: String,
    /// The package the key stands for: the key itself unless the dependency renames it.
    pub(crate) package: String,
    /// The repository directory of a dependency declared with `path`.
    pub(crate) path: Option<String>,
    /// The `[lib] name` in the manifest of a path dependency that is not renamed: code writes it instead of the key.
    pub(crate) lib_name: Option<String>,
    pub(crate) section: Section,
}

impl Dependency {
    /// The name code writes for the dependency: its key, or the `[lib] name` of its own manifest when it sets one.
    fn code_name(&self) -> &str {
        self.lib_name.as_deref().unwrap_or(&self.key)
    }

    /// Whether `use` paths reach this dependency as `name`: its code name, with `_` where that has `-`.
    pub(crate) fn is_named(&self, name: &str) -> bool {
        let code = self.code_name();
        code.len() == name.len()
            && code
                .chars()
                .zip(name.chars())
                .all(|(code, name)| code == name || (code == '-' && name == '_'))
    }

    /// Whether code of a `kind` target can name this dependency: a build script sees the build dependencies and
    /// nothing else, and no other target sees those.
    pub(crate) fn visible_to(&self, kind: TargetKind) -> bool {
        (self.section == Section::Build) == (kind == TargetKind::Build)
    }
}

/// A manifest with a `[package]` table, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Package {
    pub(crate) name: String,
    pub(crate) edition: Option<String>,
    /// The name other crates use for the library: `[lib] name`, else the package name with `_` for `-`.
    pub(crate) lib_name: String,
    pub(crate) targets: Vec<Target>,
    pub(crate) dependencies: Vec<Dependency>,
}

/// Every `Cargo.toml` of the repository, by directory. A manifest that does not read is kept with the reason.
#[derive(Debug, Default)]
pub(crate) struct Manifests {
    tables: BTreeMap<String, Result<Table, String>>,
}

impl Manifests {
    /// Reads every `Cargo.toml` of the view from the working tree.
    pub(crate) fn read(repo: &RepoView) -> Self {
        let tables = repo
            .files()
            .filter(|(_, path)| path.file_name() == "Cargo.toml")
            .map(|(_, path)| {
                let table = repo
                    .read_to_string(path.as_str())
                    .map_err(|error| format!("{path} cannot be read: {error}"))
                    .and_then(|text| {
                        text.parse::<Table>().map_err(|error| {
                            format!("{path} is not valid TOML: {}", error.message())
                        })
                    });
                if let Err(message) = &table {
                    tracing::warn!(manifest = %path, %message, "manifest left unread");
                }
                (path.parent().to_owned(), table)
            })
            .collect();
        Self { tables }
    }

    /// The directories that hold a manifest, in path order.
    pub(crate) fn dirs(&self) -> impl Iterator<Item = &str> {
        self.tables.keys().map(String::as_str)
    }

    /// The package of the manifest in `dir`: `None` for a virtual workspace manifest, an error when the manifest
    /// does not read.
    pub(crate) fn package(&self, repo: &RepoView, dir: &str) -> Result<Option<Package>, String> {
        let table = match self.tables.get(dir) {
            Some(Ok(table)) => table,
            Some(Err(message)) => return Err(message.clone()),
            None => return Ok(None),
        };
        let Some(package) = table.get("package").and_then(Value::as_table) else {
            return Ok(None);
        };
        let Some(name) = package.get("name").and_then(Value::as_str) else {
            return Err(format!("the [package] of `{dir}/Cargo.toml` has no name"));
        };
        let workspace = self.workspace_of(dir, package);
        let edition = match package.get("edition") {
            Some(Value::String(edition)) => Some(edition.clone()),
            Some(Value::Table(_)) => workspace
                .and_then(|(_, ws)| ws.get("workspace")?.as_table()?.get("package")?.as_table())
                .and_then(|shared| shared.get("edition")?.as_str())
                .map(str::to_owned),
            _ => None,
        };
        let lib_name =
            explicit_lib_name(table).map_or_else(|| name.replace('-', "_"), str::to_owned);
        Ok(Some(Package {
            name: name.to_owned(),
            edition,
            targets: targets(repo, dir, table, package, name, &lib_name),
            dependencies: self.dependencies(table, dir, workspace),
            lib_name,
        }))
    }

    /// The workspace root a package inherits from: the manifest `workspace = "<dir>"` names, else the nearest
    /// manifest at or above the package that has a `[workspace]` table.
    fn workspace_of(&self, dir: &str, package: &Table) -> Option<(&str, &Table)> {
        let explicit = package
            .get("workspace")
            .and_then(Value::as_str)
            .and_then(|relative| join_dir(dir, relative));
        let candidates = explicit.map_or_else(|| ancestors(dir), |named| vec![named]);
        candidates.iter().find_map(|candidate| {
            match self.tables.get_key_value(candidate.as_str()) {
                Some((key, Ok(table))) if table.contains_key("workspace") => {
                    Some((key.as_str(), table))
                }
                _ => None,
            }
        })
    }
}

/// `dir` and each of its parents up to the repository root.
fn ancestors(dir: &str) -> Vec<String> {
    let mut found = vec![dir.to_owned()];
    let mut current = dir;
    while !current.is_empty() {
        current = current.rsplit_once('/').map_or("", |(parent, _)| parent);
        found.push(current.to_owned());
    }
    found
}

/// `relative` from `dir`, as a repository path. `..` climbs; a path that leaves the repository is `None`.
pub(crate) fn join_dir(dir: &str, relative: &str) -> Option<String> {
    match RepoPath::join(dir, relative) {
        Ok(path) => Some(String::from(path)),
        // The repository root is `""` wherever a directory is a string, and `path = ".."` from `fuzz/` reaches it.
        Err(PathError::Empty) => Some(String::new()),
        Err(_) => None,
    }
}

/// `rest` inside the directory `dir`, where `""` is the repository root.
pub(crate) fn at(dir: &str, rest: &str) -> String {
    if dir.is_empty() {
        rest.to_owned()
    } else {
        format!("{dir}/{rest}")
    }
}

/// The dependency tables of a manifest: the three plain ones and the same three under each `[target.*]`.
fn dependency_tables(table: &Table) -> Vec<(Section, &Table)> {
    let mut found = sections(table);
    if let Some(targets) = table.get("target").and_then(Value::as_table) {
        for target in targets.values().filter_map(Value::as_table) {
            found.extend(sections(target));
        }
    }
    found
}

/// The dependency tables that `owner`, a manifest or a `[target.*]` table, holds directly.
fn sections(owner: &Table) -> Vec<(Section, &Table)> {
    const NAMES: [(&str, Section); 3] = [
        ("dependencies", Section::Normal),
        ("dev-dependencies", Section::Normal),
        ("build-dependencies", Section::Build),
    ];
    NAMES
        .iter()
        .filter_map(|(name, section)| Some((*section, owner.get(*name)?.as_table()?)))
        .collect()
}

/// The `[lib] name` of a manifest, when it sets one.
fn explicit_lib_name(table: &Table) -> Option<&str> {
    table.get("lib")?.as_table()?.get("name")?.as_str()
}

impl Manifests {
    fn dependencies(
        &self,
        table: &Table,
        dir: &str,
        workspace: Option<(&str, &Table)>,
    ) -> Vec<Dependency> {
        let shared = workspace.and_then(|(ws_dir, ws)| {
            let deps = ws
                .get("workspace")?
                .as_table()?
                .get("dependencies")?
                .as_table()?;
            Some((ws_dir, deps))
        });
        // A key in two sections is two dependencies: the build script has one and the rest of the package the other.
        let mut found: BTreeMap<(String, Section), Dependency> = BTreeMap::new();
        for (section, deps) in dependency_tables(table) {
            for (key, value) in deps {
                found
                    .entry((key.clone(), section))
                    .or_insert_with(|| self.dependency(key, value, section, dir, shared));
            }
        }
        found.into_values().collect()
    }

    fn dependency(
        &self,
        key: &str,
        value: &Value,
        section: Section,
        dir: &str,
        shared: Option<(&str, &Table)>,
    ) -> Dependency {
        let inherits = value
            .as_table()
            .and_then(|table| table.get("workspace"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !inherits {
            return self.parse_spec(key, value, section, dir);
        }
        shared
            .and_then(|(ws_dir, deps)| Some(self.parse_spec(key, deps.get(key)?, section, ws_dir)))
            .unwrap_or_else(|| Dependency {
                key: key.to_owned(),
                package: key.to_owned(),
                path: None,
                lib_name: None,
                section,
            })
    }

    /// A dependency from its `Cargo.toml` value, with `path` taken relative to `dir`.
    fn parse_spec(&self, key: &str, value: &Value, section: Section, dir: &str) -> Dependency {
        let table = value.as_table();
        let package = table
            .and_then(|table| table.get("package"))
            .and_then(Value::as_str)
            .unwrap_or(key);
        let path = table
            .and_then(|table| table.get("path"))
            .and_then(Value::as_str)
            .and_then(|path| join_dir(dir, path));
        // Code names a dependency that is not renamed by the `[lib] name` of its own manifest, which only a path shows.
        let lib_name = path
            .as_deref()
            .filter(|_| package == key)
            .and_then(|found| self.tables.get(found)?.as_ref().ok())
            .and_then(explicit_lib_name)
            .map(str::to_owned);
        Dependency {
            key: key.to_owned(),
            package: package.to_owned(),
            path,
            lib_name,
            section,
        }
    }
}

/// The `[[bin]]`-style entries of a manifest: a name and the optional `path`.
fn explicit(table: &Table, key: &str) -> Vec<(String, Option<String>)> {
    table
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_table)
        .filter_map(|entry| {
            let path = entry.get("path").and_then(Value::as_str).map(str::to_owned);
            let name = entry.get("name").and_then(Value::as_str);
            let name = name.map(str::to_owned).or_else(|| {
                let stem = path.as_deref()?.rsplit('/').next()?;
                Some(stem.strip_suffix(".rs").unwrap_or(stem).to_owned())
            })?;
            Some((name, path))
        })
        .collect()
}

/// The crate targets of a package: the ones Cargo finds by convention and the ones the manifest names with a
/// `path`, each once, library first.
fn targets(
    repo: &RepoView,
    dir: &str,
    table: &Table,
    package: &Table,
    name: &str,
    lib_name: &str,
) -> Vec<Target> {
    let exists = |path: &str| repo.lookup(path).is_some();
    let auto = |key: &str| package.get(key).and_then(Value::as_bool).unwrap_or(true);
    let mut found = Vec::new();

    let lib_root = table
        .get("lib")
        .and_then(Value::as_table)
        .and_then(|lib| lib.get("path"))
        .and_then(Value::as_str)
        .and_then(|path| join_dir(dir, path))
        .unwrap_or_else(|| at(dir, "src/lib.rs"));
    if exists(&lib_root) {
        found.push(Target {
            kind: TargetKind::Lib,
            name: lib_name.to_owned(),
            root: lib_root,
        });
    }

    let main = at(dir, "src/main.rs");
    if auto("autobins") && exists(&main) {
        found.push(Target {
            kind: TargetKind::Bin,
            name: name.to_owned(),
            root: main,
        });
    }

    let groups = [
        (TargetKind::Bin, "bin", "src/bin", auto("autobins")),
        (
            TargetKind::Example,
            "example",
            "examples",
            auto("autoexamples"),
        ),
        (TargetKind::Test, "test", "tests", auto("autotests")),
        (TargetKind::Bench, "bench", "benches", auto("autobenches")),
    ];
    for (kind, key, folder, discover) in groups {
        let folder = at(dir, folder);
        for (entry, path) in explicit(table, key) {
            let root = match path {
                Some(path) => join_dir(dir, &path),
                None => [
                    format!("{folder}/{entry}.rs"),
                    format!("{folder}/{entry}/main.rs"),
                ]
                .into_iter()
                .find(|candidate| exists(candidate)),
            };
            if let Some(root) = root.filter(|root| exists(root)) {
                found.push(Target {
                    kind,
                    name: entry,
                    root,
                });
            }
        }
        if discover {
            found.extend(discovered(repo, &folder, kind));
        }
    }

    let build = match package.get("build") {
        Some(Value::String(path)) => join_dir(dir, path),
        Some(Value::Boolean(false)) => None,
        _ => Some(at(dir, "build.rs")),
    };
    if let Some(root) = build.filter(|root| exists(root)) {
        found.push(Target {
            kind: TargetKind::Build,
            name: "build-script".to_owned(),
            root,
        });
    }

    found.sort_by(|a, b| (a.kind, &a.root).cmp(&(b.kind, &b.root)));
    let mut seen = HashSet::new();
    found.retain(|target| seen.insert(target.root.clone()));
    found
}

/// Targets Cargo finds in `folder`: `<name>.rs` and `<name>/main.rs`.
fn discovered(repo: &RepoView, folder: &str, kind: TargetKind) -> Vec<Target> {
    let prefix = format!("{folder}/");
    repo.under(folder)
        .filter_map(|(_, path)| {
            let rest = path.as_str().strip_prefix(prefix.as_str())?;
            let name = match rest.split_once('/') {
                None => rest.strip_suffix(".rs")?,
                Some((name, "main.rs")) => name,
                Some(_) => return None,
            };
            Some(Target {
                kind,
                name: name.to_owned(),
                root: path.as_str().to_owned(),
            })
        })
        .collect()
}
