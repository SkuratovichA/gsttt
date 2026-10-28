//! Running the engine over a fixture tree and reading what it found.

use gsttt_config::builtin_packs;
use gsttt_core::engine::pipeline::{self, Module, Output};
use gsttt_core::repo::RepoView;
use gsttt_lang_rust::{PACKS, RUST};
use serde::Serialize;
use serde_json::{Value, json};

/// A value the way the engine prints it: flags and tags are numbers there.
pub(crate) fn printed(value: &impl Serialize) -> Value {
    serde_json::to_value(value).unwrap()
}

/// The engine's print of an import that points nowhere, for the reason `reason` (`not-found`, `generated`, …).
pub(crate) fn unresolved(reason: &str) -> Value {
    json!({ "unresolved": { "reason": reason } })
}

/// Contract sites as `<line> <role> <key>`.
fn described(sites: &[Value]) -> Vec<String> {
    sites
        .iter()
        .map(|site| {
            format!(
                "{} {} {}",
                site["line"],
                site["role"].as_str().unwrap(),
                site["key"].as_str().unwrap()
            )
        })
        .collect()
}

/// One run of the Rust connector over a fixture tree.
pub(crate) struct Indexed {
    pub(crate) repo: RepoView,
    pub(crate) output: Output,
}

/// Indexes `tests/fixtures/<name>/` with the built-in packs.
pub(crate) fn index(name: &str) -> Indexed {
    let root = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    index_at(&root)
}

/// Indexes the tree under `root` with the built-in packs.
pub(crate) fn index_at(root: &str) -> Indexed {
    let repo = RepoView::scan(root).unwrap();
    let packs = builtin_packs(PACKS).unwrap();
    let modules = [Module {
        connector: &RUST,
        options: Value::Null,
        packs,
    }];
    let output = pipeline::run(&repo, &modules, 1 << 20).unwrap();
    Indexed { repo, output }
}

impl Indexed {
    /// The whole run as JSON, the shape the snapshots hold.
    pub(crate) fn json(&self) -> Value {
        self.output.to_json(&self.repo)
    }

    /// The entry of one file: its facts, its resolved imports and the project that took it.
    pub(crate) fn file(&self, path: &str) -> Value {
        let json = self.json();
        json["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["path"] == path)
            .unwrap_or_else(|| panic!("`{path}` is not among the indexed files"))
            .clone()
    }

    /// What the sites of one file say: its contracts, its embedded documents and its unread sites.
    pub(crate) fn sites(&self, path: &str) -> Value {
        let file = self.file(path);
        json!({
            "contracts": file["contracts"],
            "embedded": file["embedded"],
            "unresolved": file["unresolved"],
        })
    }

    /// The contract sites of one file as `<line> <role> <key>`, in the order the engine prints them.
    pub(crate) fn contracts(&self, path: &str) -> Vec<String> {
        described(self.file(path)["contracts"].as_array().unwrap())
    }

    /// The sites that linking found in one file, written the way [`Indexed::contracts`] writes its own.
    pub(crate) fn linked(&self, path: &str) -> Vec<String> {
        let linked: Vec<Value> = self.json()["linked"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|site| site["file"] == path)
            .cloned()
            .collect();
        described(&linked)
    }

    /// Where the first import of `spec` in `path` points, as the engine prints it.
    pub(crate) fn to(&self, path: &str, spec: &str) -> Value {
        self.file(path)["imports"]
            .as_array()
            .unwrap()
            .iter()
            .find(|import| import["spec"] == spec)
            .unwrap_or_else(|| panic!("`{path}` has no import of `{spec}`"))["to"]
            .clone()
    }

    /// The roots of the projects that opened, in the order they were returned.
    pub(crate) fn roots(&self) -> Vec<String> {
        self.json()["projects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|project| project["root"].as_str().unwrap().to_owned())
            .collect()
    }
}
