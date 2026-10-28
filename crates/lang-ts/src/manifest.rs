//! What resolution and pack activation need from a `package.json`.

use std::collections::BTreeSet;

use crate::json::Json;

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
    /// Every package named in any dependency section.
    pub dependencies: BTreeSet<String>,
    pub exports: Option<Json>,
    pub imports: Option<Json>,
    pub types: Option<String>,
    pub module: Option<String>,
    pub main: Option<String>,
}

impl Manifest {
    pub(crate) fn from_json(json: &Json) -> Self {
        let text = |key: &str| json.get(key).and_then(Json::as_str).map(str::to_owned);
        Self {
            name: text("name"),
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
    use crate::json::parse;

    #[test]
    fn every_dependency_section_counts() {
        let json = parse(r#"{"name": "web", "dependencies": {"react": "1"}, "devDependencies": {"vitest": "1"}, "peerDependencies": {"zod": "1"}}"#).unwrap();
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
