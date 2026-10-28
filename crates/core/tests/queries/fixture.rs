//! A tree written as a list of facts, turned into the `Index` the engine would build from it.

use std::collections::BTreeMap;

use gsttt_core::contract::{ContractKey, Role};
use gsttt_core::coverage::{ConnectorCoverage, Coverage};
use gsttt_core::fact::{
    FileTags, Import, ImportKind, ParseStatus, Resolution, ResolvedImport, UnresolvedReason,
    UnresolvedSite,
};
use gsttt_core::graph::{ContractSite, Edge, GraphBuilder};
use gsttt_core::index::{FileRecord, Index, IndexInfo, ProjectInfo};
use gsttt_core::{FileId, ProjectId, RepoPath};

/// The connector that reads a file unless a test says otherwise.
const DEFAULT_CONNECTOR: &str = "ts";

/// Where an import of a test tree points.
#[derive(Clone, Debug)]
pub(crate) enum To {
    File(String),
    Missing(UnresolvedReason),
}

/// An import that resolved to the file at `path`.
pub(crate) fn file(path: &str) -> To {
    To::File(path.to_owned())
}

impl To {
    fn paths(&self) -> Vec<&str> {
        match self {
            Self::File(path) => vec![path.as_str()],
            Self::Missing(_) => Vec::new(),
        }
    }

    fn resolve(&self, ids: &BTreeMap<String, FileId>) -> Resolution {
        match self {
            Self::File(path) => Resolution::File { file: ids[path] },
            Self::Missing(reason) => Resolution::Unresolved(reason.clone()),
        }
    }
}

#[derive(Debug)]
struct Entry {
    /// The connector that read the file, `None` when no connector did.
    connector: Option<String>,
    tags: FileTags,
    status: ParseStatus,
    imports: Vec<(Import, To)>,
    sites: Vec<(ContractKey, Role, u32)>,
    blind: Vec<UnresolvedSite>,
}

impl Entry {
    fn new(connector: Option<&str>) -> Self {
        Self {
            connector: connector.map(str::to_owned),
            tags: FileTags::NONE,
            status: ParseStatus::Ok,
            imports: Vec::new(),
            sites: Vec::new(),
            blind: Vec::new(),
        }
    }
}

/// A tree of files, their imports and their contract sites.
#[derive(Debug, Default)]
pub(crate) struct Tree {
    files: BTreeMap<String, Entry>,
    /// Connector names in the order they were first used, which stands for the registry order.
    connectors: Vec<String>,
    prefixes: Vec<String>,
    head: Option<String>,
    dirty: u32,
}

impl Tree {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn register(&mut self, connector: &str) {
        if !self.connectors.iter().any(|known| known == connector) {
            self.connectors.push(connector.to_owned());
        }
    }

    /// The entry of `path`, created as a file the default connector read.
    fn entry(&mut self, path: &str) -> &mut Entry {
        if !self.files.contains_key(path) {
            self.register(DEFAULT_CONNECTOR);
            self.files
                .insert(path.to_owned(), Entry::new(Some(DEFAULT_CONNECTOR)));
        }
        self.files.get_mut(path).unwrap()
    }

    /// A file that `connector` read.
    pub(crate) fn read_by(mut self, connector: &str, path: &str) -> Self {
        self.register(connector);
        self.entry(path).connector = Some(connector.to_owned());
        self
    }

    /// A file of the tree that no connector read.
    pub(crate) fn unread(mut self, path: &str) -> Self {
        self.entry(path).connector = None;
        self
    }

    pub(crate) fn tag(mut self, path: &str, tags: FileTags) -> Self {
        self.entry(path).tags = tags;
        self
    }

    /// `from` imports the file `to` with a static import on line 1.
    pub(crate) fn imports(self, from: &str, to: &str) -> Self {
        self.import(from, Import::new(to, ImportKind::Static, 1), file(to))
    }

    /// `from` has `import`, which points at `to`.
    pub(crate) fn import(mut self, from: &str, import: Import, to: To) -> Self {
        for target in to.paths() {
            self.entry(target);
        }
        self.entry(from).imports.push((import, to));
        self
    }

    pub(crate) fn site(mut self, path: &str, key: &str, role: Role, line: u32) -> Self {
        let key = ContractKey::parse(key).unwrap();
        self.entry(path).sites.push((key, role, line));
        self
    }

    pub(crate) fn declares(self, path: &str, key: &str) -> Self {
        self.site(path, key, Role::Declares, 1)
    }

    pub(crate) fn serves(self, path: &str, key: &str) -> Self {
        self.site(path, key, Role::Serves, 1)
    }

    pub(crate) fn calls(self, path: &str, key: &str) -> Self {
        self.site(path, key, Role::Calls, 1)
    }

    pub(crate) fn prefixes(mut self, prefixes: &[&str]) -> Self {
        self.prefixes = prefixes.iter().map(|prefix| (*prefix).to_owned()).collect();
        self
    }

    pub(crate) fn build(self) -> Index {
        let ids: BTreeMap<String, FileId> = self
            .files
            .keys()
            .enumerate()
            .map(|(position, path)| {
                (
                    path.clone(),
                    FileId::from_raw(u32::try_from(position).unwrap()),
                )
            })
            .collect();
        let paths: Vec<RepoPath> = self
            .files
            .keys()
            .map(|path| RepoPath::normalize(path).unwrap())
            .collect();

        let mut graph = GraphBuilder::new(paths.clone());
        for (path, entry) in &self.files {
            let from = ids[path];
            graph.tag(from, entry.tags);
            for (import, to) in &entry.imports {
                for target in to.paths() {
                    graph.edge(from, ids[target], Edge::new(import.kind, import.flags));
                }
            }
            for (key, role, line) in &entry.sites {
                graph.site(
                    key.clone(),
                    ContractSite {
                        file: from,
                        line: *line,
                        role: *role,
                        via: "test".to_owned(),
                    },
                );
            }
        }

        let projects: Vec<ProjectInfo> = self
            .connectors
            .iter()
            .map(|connector| ProjectInfo {
                connector: connector.clone(),
                root: String::new(),
            })
            .collect();
        let mut coverage = Coverage {
            connectors: self
                .connectors
                .iter()
                .map(|connector| ConnectorCoverage::new(connector))
                .collect(),
            ..Coverage::default()
        };
        let mut records = Vec::new();
        for (position, (path, entry)) in self.files.iter().enumerate() {
            let file = ids[path];
            let imports: Vec<ResolvedImport> = entry
                .imports
                .iter()
                .map(|(import, to)| ResolvedImport {
                    import: import.clone(),
                    to: to.resolve(&ids),
                })
                .collect();
            let project = entry.connector.as_ref().map(|connector| {
                let project = self
                    .connectors
                    .iter()
                    .position(|known| known == connector)
                    .unwrap();
                let counted = &mut coverage.connectors[project];
                counted.count_file(file, &entry.status);
                for import in &imports {
                    counted.count_import(file, import);
                }
                for site in &entry.blind {
                    counted.count_blind(file, site);
                }
                ProjectId::from_raw(u32::try_from(project).unwrap())
            });
            if project.is_none() {
                coverage.count_unclaimed(&paths[position]);
            }
            records.push(FileRecord {
                project,
                status: entry.status.clone(),
                imports,
                unresolved: entry.blind.clone(),
            });
        }
        coverage.finish();

        Index {
            graph: graph.build(),
            projects,
            records,
            coverage,
            info: IndexInfo {
                built_by: "0.0.0".to_owned(),
                head: self.head,
                dirty: self.dirty,
                ..IndexInfo::default()
            },
            http_prefixes: self.prefixes,
        }
    }
}
