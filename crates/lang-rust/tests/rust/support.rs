//! Running the engine over a fixture tree and reading what it found.

use gsttt_config::builtin_packs;
use gsttt_core::engine::build::build_index;
use gsttt_core::engine::pipeline::{self, FileRun, Module, Output};
use gsttt_core::fact::Symbols;
use gsttt_core::index::{Freshness, IndexInfo};
use gsttt_core::query::{Impact, ImpactRequest, impact};
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;
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
    /// The directory an inline tree was written to, kept until the test ends.
    dir: Option<tempfile::TempDir>,
}

/// A crate named `t` whose `src/lib.rs` declares `modules` and whose files are `files`, indexed like a fixture, for the programs of a few lines whose declarations a scenario reads.
pub(crate) fn crate_of(modules: &[&str], files: &[(&str, &str)]) -> Indexed {
    let dir = tempfile::tempdir().unwrap();
    let lib: String = modules
        .iter()
        .map(|name| ["mod ", name, ";\n"].concat())
        .collect();
    let manifest = "[package]\nname = \"t\"\nversion = \"0.0.0\"\nedition = \"2021\"\n";
    for (path, text) in [("Cargo.toml", manifest), ("src/lib.rs", lib.as_str())]
        .into_iter()
        .chain(files.iter().map(|(path, text)| (*path, *text)))
    {
        let file = dir.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let root = dir.path().to_str().unwrap().to_owned();
    let mut indexed = index_at_with(&root, builtin_packs(PACKS).unwrap());
    indexed.dir = Some(dir);
    indexed
}

/// Indexes `tests/fixtures/<name>/` with the built-in packs.
pub(crate) fn index(name: &str) -> Indexed {
    let root = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    index_at(&root)
}

/// Indexes the tree under `root` with the built-in packs.
pub(crate) fn index_at(root: &str) -> Indexed {
    index_at_with(root, builtin_packs(PACKS).unwrap())
}

/// Indexes `tests/fixtures/<name>/` with `packs` in place of the built-in ones.
pub(crate) fn index_with(name: &str, packs: Vec<Pack>) -> Indexed {
    let root = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    index_at_with(&root, packs)
}

fn index_at_with(root: &str, packs: Vec<Pack>) -> Indexed {
    let repo = RepoView::scan(root).unwrap();
    let modules = [Module {
        connector: &RUST,
        options: Value::Null,
        packs,
    }];
    let output = pipeline::run(&repo, &modules, 1 << 20).unwrap();
    Indexed {
        repo,
        output,
        dir: None,
    }
}

impl Indexed {
    /// The answer to `gsttt impact <path> --name <names>` over this tree, one hop unless `depth` says more.
    pub(crate) fn ask(&self, path: &str, names: &[&str], depth: Option<u32>) -> Impact {
        let index = build_index(
            &self.repo,
            &self.output,
            &["rust"],
            &[],
            &[],
            IndexInfo::default(),
        );
        let request = ImpactRequest {
            files: vec![path.to_owned()],
            names: names.iter().map(|&name| name.to_owned()).collect(),
            depth,
            ..ImpactRequest::default()
        };
        impact(&index, Freshness::Fresh, &request)
    }

    fn run_of(&self, path: &str) -> &FileRun {
        self.output
            .files
            .iter()
            .find(|file| {
                self.repo
                    .path(file.id)
                    .is_some_and(|at| at.as_str() == path)
            })
            .unwrap_or_else(|| panic!("`{path}` is not among the indexed files"))
    }

    /// The declarations and uses a file records, which a file with syntax errors does not.
    pub(crate) fn symbols(&self, path: &str) -> &Symbols {
        self.run_of(path)
            .facts
            .symbols
            .as_ref()
            .unwrap_or_else(|| panic!("`{path}` records no declarations"))
    }

    /// Whether the file records declarations.
    pub(crate) fn records(&self, path: &str) -> bool {
        self.run_of(path).facts.symbols.is_some()
    }

    /// The declarations of a file as `name L<start>-<end>`, in the order it records them.
    pub(crate) fn declared(&self, path: &str) -> Vec<String> {
        self.symbols(path)
            .declared
            .iter()
            .map(|declared| format!("{} L{}-{}", declared.name, declared.line, declared.end))
            .collect()
    }

    /// What the declaration `name` of a file uses, in byte order.
    pub(crate) fn uses(&self, path: &str, name: &str) -> Vec<&str> {
        let mut uses: Vec<&str> = self
            .symbols(path)
            .declared
            .iter()
            .find(|declared| declared.name == name)
            .unwrap_or_else(|| panic!("{path} does not declare {name}: {:?}", self.declared(path)))
            .uses
            .iter()
            .map(|used| used.name.as_str())
            .collect();
        uses.sort_unstable();
        uses
    }

    /// What the declaration `name` of a file uses, each with the line the code names it on, in byte order.
    pub(crate) fn use_lines(&self, path: &str, name: &str) -> Vec<(String, u32)> {
        let mut uses: Vec<(String, u32)> = self
            .symbols(path)
            .declared
            .iter()
            .find(|declared| declared.name == name)
            .unwrap_or_else(|| panic!("{path} does not declare {name}: {:?}", self.declared(path)))
            .uses
            .iter()
            .map(|used| (used.name.clone(), used.line))
            .collect();
        uses.sort_unstable();
        uses
    }

    /// The member names a file reads on a value of an unknown type, each with its line, in byte order.
    pub(crate) fn read_lines(&self, path: &str) -> Vec<(String, u32)> {
        let mut reads: Vec<(String, u32)> = self
            .symbols(path)
            .reads
            .iter()
            .map(|used| (used.name.clone(), used.line))
            .collect();
        reads.sort_unstable();
        reads
    }

    /// What `path` uses as it loads.
    pub(crate) fn on_load(&self, path: &str) -> Vec<&str> {
        self.symbols(path)
            .on_load
            .iter()
            .map(|used| used.name.as_str())
            .collect()
    }

    /// The member names a file reads on a value of a type it does not know, in byte order.
    pub(crate) fn reads(&self, path: &str) -> Vec<&str> {
        let mut reads: Vec<&str> = self
            .symbols(path)
            .reads
            .iter()
            .map(|used| used.name.as_str())
            .collect();
        reads.sort_unstable();
        reads
    }

    /// The heirs of a file as `name of`.
    pub(crate) fn heirs(&self, path: &str) -> Vec<String> {
        self.symbols(path)
            .heirs
            .iter()
            .map(|heir| format!("{} {}", heir.name, heir.of))
            .collect()
    }

    /// The members the file records as holding a value of a class, as `name of`.
    pub(crate) fn holds(&self, path: &str) -> Vec<String> {
        self.symbols(path)
            .holds
            .iter()
            .map(|heir| format!("{} {}", heir.name, heir.of))
            .collect()
    }

    /// What the file overrides, as `name of`.
    pub(crate) fn overrides(&self, path: &str) -> Vec<String> {
        self.symbols(path)
            .overrides
            .iter()
            .map(|heir| format!("{} {}", heir.name, heir.of))
            .collect()
    }

    /// The names an importer of the file can take by name, in byte order.
    pub(crate) fn exports(&self, path: &str) -> Vec<&str> {
        let mut exports: Vec<&str> = self
            .run_of(path)
            .facts
            .exports
            .as_ref()
            .unwrap_or_else(|| panic!("`{path}` records no exports"))
            .iter()
            .map(String::as_str)
            .collect();
        exports.sort_unstable();
        exports
    }

    /// The static imports of a file as `spec [names] [exported]`, in its order.
    pub(crate) fn imports(&self, path: &str) -> Vec<String> {
        self.run_of(path)
            .imports
            .iter()
            .map(|resolved| {
                let import = &resolved.import;
                format!(
                    "{} [{}] [{}]",
                    import.spec,
                    import.names.join(","),
                    import.exported.join(",")
                )
            })
            .collect()
    }

    /// The files a walk by name reaches as `<path>@<distance>`, in the order of the answer.
    pub(crate) fn reached(&self, path: &str, names: &[&str], depth: Option<u32>) -> Vec<String> {
        self.ask(path, names, depth)
            .affected
            .iter()
            .map(|entry| format!("{}@{}", entry.path, entry.distance))
            .collect()
    }

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
