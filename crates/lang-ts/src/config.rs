//! tsconfig files: what each one says, and what a chain of `extends` adds up to.

use gsttt_core::fact::Line;

use crate::json::Json;
use crate::pathing::join_dir;

/// The options of one tsconfig file as written, before `extends` is followed.
#[derive(Debug, Clone, Default)]
pub(crate) struct ConfigFile {
    pub extends: Vec<String>,
    pub base_url: Option<String>,
    pub out_dir: Option<String>,
    pub root_dir: Option<String>,
    pub paths: Option<Vec<(String, Vec<String>)>>,
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
    }

    /// Lets the options `config` sets, read from the directory `dir` it lies in, win over what is already here.
    pub(crate) fn apply(&mut self, config: &ConfigFile, dir: &str) {
        let resolve =
            |value: &Option<String>| value.as_deref().and_then(|value| join_dir(dir, value));
        if let Some(base_url) = resolve(&config.base_url) {
            self.base_url = Some(base_url);
        }
        if let Some(out_dir) = resolve(&config.out_dir) {
            self.out_dir = Some(out_dir);
        }
        if let Some(root_dir) = resolve(&config.root_dir) {
            self.root_dir = Some(root_dir);
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
