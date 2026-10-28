//! Running the engine over a fixture tree and reading what it found.

use gsttt_config::builtin_packs;
use gsttt_core::engine::build::build_index;
use gsttt_core::engine::pipeline::{self, Module, Output};
use gsttt_core::index::IndexInfo;
use gsttt_core::repo::RepoView;
use gsttt_lang_rust::{PACKS, RUST};
use serde::Serialize;
use serde_json::{Value, json};

/// A value the way the engine prints it: flags and tags are numbers there.
pub(crate) fn printed(value: &impl Serialize) -> Value {
    let mut json = serde_json::to_value(value).unwrap();
    json.sort_all_objects();
    json
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
    /// The whole run as JSON, the shape the snapshots hold, with the keys of every object in byte order.
    pub(crate) fn json(&self) -> Value {
        // Sorted here, so that no snapshot depends on whether serde_json keeps insertion order: its `preserve_order` feature, which another crate of the workspace may switch on or off.
        let mut json = self.output.to_json(&self.repo);
        json.sort_all_objects();
        json
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

    /// The contract sites of one file in the graph the engine builds from the output, as `<line> <role> <key>` in byte order. Unlike [`Indexed::contracts`], the graph has applied the rule that a linked fact replaces the own fact it names, so it is what a query answers from.
    pub(crate) fn graph(&self, path: &str) -> Vec<String> {
        let index = build_index(
            &self.repo,
            &self.output,
            &["rust"],
            &[],
            IndexInfo::default(),
        );
        let graph = &index.graph;
        let mut sites: Vec<String> = graph
            .contracts()
            .flat_map(|(id, key)| {
                graph
                    .sites(id)
                    .iter()
                    .filter(|site| graph.path(site.file).is_some_and(|at| at.as_str() == path))
                    .map(move |site| format!("{} {} {key}", site.line, site.role))
            })
            .collect();
        sites.sort();
        sites
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

    /// What the project rooted at `root` (`.` for the repository root) reports it provides, as the engine prints it.
    pub(crate) fn provides(&self, root: &str) -> Value {
        self.json()["projects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|project| project["root"] == root)
            .unwrap_or_else(|| panic!("no project is rooted at `{root}`"))["provides"]
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
