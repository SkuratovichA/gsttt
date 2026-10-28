//! What resolution and pack activation need from a `package.json`.

use std::collections::BTreeSet;

use gsttt_core::tree::Value;

/// The sections of a manifest that declare packages.
const DEPENDENCY_SECTIONS: [&str; 4] = [
    "dependencies",
    "devDependencies",
    "peerDependencies",
    "optionalDependencies",
];

/// A package manifest, reduced to the fields the connector reads.
#[derive(Debug, Clone, Default)]
pub(crate) struct Manifest {
    pub name: Option<String>,
    /// Whether the file opens a project of its own: it names a package, declares dependencies or lists workspaces. A file with none of those, such as `{"type": "module"}`, only marks the directory it lies in.
    pub declares_project: bool,
    /// Every package named in any dependency section.
    pub dependencies: BTreeSet<String>,
    pub exports: Option<Value>,
    pub imports: Option<Value>,
    pub types: Option<String>,
    pub module: Option<String>,
    pub main: Option<String>,
}

impl Manifest {
    pub(crate) fn from_json(json: &Value) -> Self {
        let text = |key: &str| json.get(key).and_then(Value::as_str).map(str::to_owned);
        Self {
            name: text("name"),
            declares_project: text("name").is_some()
                || DEPENDENCY_SECTIONS
                    .iter()
                    .copied()
                    .chain(["workspaces"])
                    .any(|section| json.get(section).is_some()),
            dependencies: DEPENDENCY_SECTIONS
                .iter()
                .filter_map(|section| json.get(section))
                .flat_map(|section| section.entries().iter().map(|entry| entry.key.clone()))
                .collect(),
            exports: json.get("exports").cloned(),
            imports: json.get("imports").cloned(),
            types: text("types").or_else(|| text("typings")),
            module: text("module"),
            main: text("main"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gsttt_core::tree::parse_json;

    #[test]
    fn a_marker_file_declares_no_project() {
        let declares =
            |text: &str| Manifest::from_json(&parse_json(text).unwrap()).declares_project;
        assert!(!declares(r#"{"type": "module"}"#));
        assert!(declares(r#"{"name": "app"}"#));
        assert!(declares(r#"{"dependencies": {}}"#));
        assert!(declares(r#"{"workspaces": ["packages/*"]}"#));
    }

    #[test]
    fn every_dependency_section_counts() {
        let json = parse_json(r#"{"name": "web", "dependencies": {"react": "1"}, "devDependencies": {"vitest": "1"}, "peerDependencies": {"zod": "1"}}"#).unwrap();
        let manifest = Manifest::from_json(&json);
        assert_eq!(manifest.name.as_deref(), Some("web"));
        assert_eq!(
            manifest
                .dependencies
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["react", "vitest", "zod"]
        );
    }
}
