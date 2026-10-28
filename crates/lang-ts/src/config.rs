//! tsconfig files: what each one says, and what a chain of `extends` adds up to.

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use gsttt_core::fact::Line;
use gsttt_core::path::RepoPath;

use crate::json::Json;
use crate::pathing::join_file;

/// The options of one tsconfig file as written, before `extends` is followed.
#[derive(Debug, Clone, Default)]
pub(crate) struct ConfigFile {
    pub extends: Vec<String>,
    pub base_url: Option<String>,
    pub out_dir: Option<String>,
    pub root_dir: Option<String>,
    pub paths: Option<Vec<(String, Vec<String>)>>,
    /// The `path` of every project reference, as written.
    pub references: Vec<String>,
    /// The lists `files`, `include` and `exclude`, as written.
    pub files: Option<Vec<String>>,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
}

impl ConfigFile {
    pub(crate) fn from_json(json: &Json) -> Self {
        let options = json.get("compilerOptions");
        let text = |key: &str| {
            options
                .and_then(|options| options.get(key))
                .and_then(Json::as_str)
                .map(str::to_owned)
        };
        Self {
            extends: extends_of(json).into_iter().map(|(spec, _)| spec).collect(),
            references: references_of(json),
            files: strings_at(json, "files"),
            include: strings_at(json, "include"),
            exclude: strings_at(json, "exclude"),
            base_url: text("baseUrl"),
            out_dir: text("outDir"),
            root_dir: text("rootDir"),
            paths: options
                .and_then(|options| options.get("paths"))
                .map(|paths| {
                    paths
                        .entries()
                        .iter()
                        .map(|entry| {
                            (
                                entry.key.clone(),
                                entry
                                    .value
                                    .strings()
                                    .into_iter()
                                    .map(str::to_owned)
                                    .collect(),
                            )
                        })
                        .collect()
                }),
        }
    }
}

/// The configs a tsconfig extends, as written, with the line of the `extends` key.
pub(crate) fn extends_of(json: &Json) -> Vec<(String, Line)> {
    json.entry("extends")
        .map(|entry| {
            entry
                .value
                .strings()
                .into_iter()
                .map(|spec| (spec.to_owned(), entry.line))
                .collect()
        })
        .unwrap_or_default()
}

/// The `path` of every project reference of a tsconfig, as written.
fn references_of(json: &Json) -> Vec<String> {
    match json.get("references") {
        Some(Json::Array(items)) => items
            .iter()
            .filter_map(|item| item.get("path").and_then(Json::as_str))
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

/// The strings of the list at `key`, when the config has one.
fn strings_at(json: &Json, key: &str) -> Option<Vec<String>> {
    json.get(key)
        .map(|value| value.strings().into_iter().map(str::to_owned).collect())
}

/// One `paths` entry, remembering the directory of the config that declares it: a target is read from `baseUrl` when one is
/// set, and from that directory when not.
#[derive(Debug, Clone)]
pub(crate) struct PathMapping {
    pub pattern: String,
    pub targets: Vec<String>,
    pub dir: String,
}

/// The options a config ends up with once its `extends` chain is folded in. Directories are repository paths.
#[derive(Debug, Clone, Default)]
pub(crate) struct Effective {
    pub base_url: Option<String>,
    pub out_dir: Option<String>,
    pub root_dir: Option<String>,
    pub paths: Option<Vec<PathMapping>>,
    /// The files, and the patterns of the files included and left out, as repository paths. Each is inherited alone.
    pub files: Option<Vec<String>>,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
}

impl Effective {
    /// Lets what `parent` sets win over what is already here.
    pub(crate) fn overlay(&mut self, parent: &Self) {
        if parent.base_url.is_some() {
            self.base_url.clone_from(&parent.base_url);
        }
        if parent.out_dir.is_some() {
            self.out_dir.clone_from(&parent.out_dir);
        }
        if parent.root_dir.is_some() {
            self.root_dir.clone_from(&parent.root_dir);
        }
        if parent.paths.is_some() {
            self.paths.clone_from(&parent.paths);
        }
        if parent.files.is_some() {
            self.files.clone_from(&parent.files);
        }
        if parent.include.is_some() {
            self.include.clone_from(&parent.include);
        }
        if parent.exclude.is_some() {
            self.exclude.clone_from(&parent.exclude);
        }
    }

    /// Lets the options `config` sets, read from the directory `dir` it lies in, win over what is already here.
    pub(crate) fn apply(&mut self, config: &ConfigFile, dir: &str) {
        let resolve = |value: &Option<String>| {
            value
                .as_deref()
                .and_then(|value| RepoPath::join_dir(dir, value))
        };
        if let Some(base_url) = resolve(&config.base_url) {
            self.base_url = Some(base_url);
        }
        if let Some(out_dir) = resolve(&config.out_dir) {
            self.out_dir = Some(out_dir);
        }
        if let Some(root_dir) = resolve(&config.root_dir) {
            self.root_dir = Some(root_dir);
        }
        let rooted = |list: &Option<Vec<String>>| {
            list.as_ref().map(|items| {
                items
                    .iter()
                    .filter_map(|item| RepoPath::join_dir(dir, item))
                    .collect::<Vec<_>>()
            })
        };
        if let Some(files) = rooted(&config.files) {
            self.files = Some(files);
        }
        if let Some(include) = rooted(&config.include) {
            self.include = Some(include);
        }
        if let Some(exclude) = rooted(&config.exclude) {
            self.exclude = Some(exclude);
        }
        if let Some(paths) = &config.paths {
            self.paths = Some(
                paths
                    .iter()
                    .map(|(pattern, targets)| PathMapping {
                        pattern: pattern.clone(),
                        targets: targets.clone(),
                        dir: dir.to_owned(),
                    })
                    .collect(),
            );
        }
    }
}

/// The files a config lists, compiled once: what `files` names, and what `include` matches less what `exclude` matches.
#[derive(Debug)]
pub(crate) struct Cover {
    files: Vec<String>,
    include: Option<GlobSet>,
    exclude: GlobSet,
}

impl Cover {
    /// The cover of `effective`, a config that lies in `dir`. A config with neither `include` nor `files` lists
    /// everything below it; one with `files` alone lists only those.
    pub(crate) fn of(dir: &str, effective: &Effective) -> Self {
        let include = match (&effective.include, &effective.files) {
            (Some(patterns), _) => Some(globs(patterns.iter().map(String::as_str))),
            (None, None) => Some(globs([join_file(dir, "**/*")].iter().map(String::as_str))),
            (None, Some(_)) => None,
        };
        Self {
            files: effective.files.clone().unwrap_or_default(),
            include,
            exclude: globs(effective.exclude.iter().flatten().map(String::as_str)),
        }
    }

    /// Whether the config lists the file at `path`.
    pub(crate) fn covers(&self, path: &str) -> bool {
        self.files.iter().any(|file| file == path)
            || (self
                .include
                .as_ref()
                .is_some_and(|include| include.is_match(path))
                && !self.exclude.is_match(path))
    }
}

/// The glob set of `patterns`; a pattern that is not valid lists nothing. `*` stays within a segment, as in tsc.
fn globs<'p>(patterns: impl Iterator<Item = &'p str>) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        if let Ok(glob) = GlobBuilder::new(&directory_form(pattern))
            .literal_separator(true)
            .build()
        {
            builder.add(glob);
        }
    }
    builder.build().unwrap_or_else(|_| GlobSet::empty())
}

/// A pattern whose last segment has no wildcard and no extension names a directory: tsc reads `src` as `src/**/*`.
fn directory_form(pattern: &str) -> String {
    let last = pattern.rsplit('/').next().unwrap_or(pattern);
    if pattern.is_empty() {
        "**".to_owned()
    } else if last.contains(['*', '?', '.']) {
        pattern.to_owned()
    } else {
        format!("{pattern}/**")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse;

    #[test]
    fn a_child_overrides_what_its_parent_sets_and_keeps_the_rest() {
        let base = ConfigFile::from_json(&parse(r#"{"compilerOptions": {"baseUrl": ".", "outDir": "dist", "paths": {"@a/*": ["a/*"]}}}"#).unwrap());
        let child =
            ConfigFile::from_json(&parse(r#"{"compilerOptions": {"outDir": "build"}}"#).unwrap());
        let mut parent = Effective::default();
        parent.apply(&base, "");
        let mut effective = Effective::default();
        effective.overlay(&parent);
        effective.apply(&child, "apps/web");
        assert_eq!(effective.base_url.as_deref(), Some(""));
        assert_eq!(effective.out_dir.as_deref(), Some("apps/web/build"));
        assert_eq!(effective.paths.as_ref().map(Vec::len), Some(1));
    }

    #[test]
    fn a_solution_config_lists_references_and_no_files() {
        let solution = ConfigFile::from_json(
            &parse(r#"{"files": [], "references": [{"path": "./tsconfig.app.json"}, {"path": "./node"}]}"#).unwrap(),
        );
        assert_eq!(solution.references, ["./tsconfig.app.json", "./node"]);
        assert_eq!(solution.files, Some(Vec::new()));
        let mut effective = Effective::default();
        effective.apply(&solution, "");
        assert!(!Cover::of("", &effective).covers("src/main.ts"));
    }

    #[test]
    fn include_and_exclude_choose_the_files_a_config_covers() {
        let config = ConfigFile::from_json(
            &parse(r#"{"include": ["src", "vite.config.ts"], "exclude": ["src/legacy"]}"#).unwrap(),
        );
        let mut effective = Effective::default();
        effective.apply(&config, "app");
        let cover = Cover::of("app", &effective);
        assert!(cover.covers("app/src/deep/a.ts"));
        assert!(cover.covers("app/vite.config.ts"));
        assert!(!cover.covers("app/src/legacy/old.ts"));
        assert!(!cover.covers("app/other.ts"));
        assert!(!cover.covers("src/a.ts"));
    }

    #[test]
    fn a_config_without_a_list_covers_everything_below_it() {
        let mut effective = Effective::default();
        effective.apply(&ConfigFile::default(), "app");
        let cover = Cover::of("app", &effective);
        assert!(
            cover.covers("app/a.ts") && cover.covers("app/b/c.ts") && !cover.covers("lib/a.ts")
        );
    }

    #[test]
    fn extends_may_be_one_config_or_several() {
        assert_eq!(
            extends_of(&parse(r#"{"extends": "./a.json"}"#).unwrap()),
            [("./a.json".to_owned(), 1)]
        );
        assert_eq!(
            extends_of(&parse("{\n\"extends\": [\"./a\", \"./b\"]}").unwrap()).len(),
            2
        );
    }
}
