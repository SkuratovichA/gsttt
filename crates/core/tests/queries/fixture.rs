//! A tree written as a list of facts, turned into the `Index` the engine would build from it.

use std::collections::BTreeMap;

use gsttt_core::contract::{ContractKey, Role};
use gsttt_core::coverage::{ConnectorCoverage, Coverage, FailedFile};
use gsttt_core::fact::{
    FileTags, Import, ImportKind, ParseFailure, ParseStatus, Resolution, ResolvedImport,
    UnresolvedReason, UnresolvedSite,
};
use gsttt_core::graph::{ContractSite, Edge, GraphBuilder};
use gsttt_core::index::{FileRecord, Index, IndexInfo, ProjectInfo};
use gsttt_core::{FileId, ProjectId, RepoPath};

/// The connector that reads a file unless a test says otherwise.
const DEFAULT_CONNECTOR: &str = "ts";

/// The rule that found a site of `role`, as a connector names it: an OpenAPI document declares a route, a NestJS
/// controller serves it and a `fetch` calls it. Each role has its own rule, so that a test sees an answer that takes
/// the rule from another site.
fn via(role: Role) -> &'static str {
    match role {
        Role::Declares => "openapi",
        Role::Serves => "nestjs",
        Role::Calls => "fetch",
    }
}

/// Where an import of a test tree points.
#[derive(Clone, Debug)]
pub(crate) enum To {
    File(String),
    Files(Vec<String>),
    Package(String),
    Builtin,
    Missing(UnresolvedReason),
}

/// An import that resolved to the file at `path`.
pub(crate) fn file(path: &str) -> To {
    To::File(path.to_owned())
}

/// An import that resolved to a third-party package.
pub(crate) fn package(name: &str) -> To {
    To::Package(name.to_owned())
}

/// A tree where `src/a.ts` has `total` imports, the first `resolved` of them to a package and the others to nothing.
pub(crate) fn resolving(resolved: u32, total: u32) -> Tree {
    (0..total).fold(Tree::new(), |tree, n| {
        let to = if n < resolved {
            package("react")
        } else {
            To::Missing(UnresolvedReason::NotFound)
        };
        tree.import(
            "src/a.ts",
            Import::new(format!("./m{n}"), ImportKind::Static, n + 1),
            to,
        )
    })
}

impl To {
    fn paths(&self) -> Vec<&str> {
        match self {
            Self::File(path) => vec![path.as_str()],
            Self::Files(paths) => paths.iter().map(String::as_str).collect(),
            Self::Package(_) | Self::Builtin | Self::Missing(_) => Vec::new(),
        }
    }

    fn resolve(&self, ids: &BTreeMap<String, FileId>) -> Resolution {
        match self {
            Self::File(path) => Resolution::File { file: ids[path] },
            Self::Files(paths) => Resolution::Files {
                files: paths.iter().map(|path| ids[path]).collect(),
            },
            Self::Package(package) => Resolution::External {
                package: package.clone(),
            },
            Self::Builtin => Resolution::Builtin,
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

/// A tree of files, their imports and their contract sites, built the way the engine builds an index from the facts of
/// a connector: a file keeps its imports in the order they were given, which is the order of its facts and not a sorted
/// one, a file the engine puts aside is owned and counted apart, and a project is named `.` at the repository root.
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

    /// Adds files that `connector` read until it owns `total` of them, so that a test can put the few files it looks
    /// at in a tree of the size of a real one.
    pub(crate) fn padded(mut self, connector: &str, total: usize) -> Self {
        self.register(connector);
        let owned = self
            .files
            .values()
            .filter(|entry| entry.connector.as_deref() == Some(connector))
            .count();
        for number in owned..total {
            self.files.insert(
                format!("padding/{connector}/{number}"),
                Entry::new(Some(connector)),
            );
        }
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

    /// A file its connector could not read at all. A file over the size limit, not UTF-8 or unreadable is the engine's
    /// to put aside: it is owned, counted in the files of its connector and listed as skipped, and it is not a failure
    /// of the connector.
    pub(crate) fn failed(mut self, path: &str, failure: ParseFailure) -> Self {
        self.entry(path).status = ParseStatus::Failed(failure);
        self
    }

    /// A file its connector read past `errors` syntax errors, the first on `line`.
    pub(crate) fn recovered(mut self, path: &str, errors: u32, line: u32) -> Self {
        self.entry(path).status = ParseStatus::Recovered {
            errors,
            first_error_line: line,
        };
        self
    }

    /// A site in `path` that its connector saw and could not read.
    pub(crate) fn blind(
        mut self,
        path: &str,
        line: u32,
        reason: UnresolvedReason,
        detail: &str,
    ) -> Self {
        self.entry(path).blind.push(UnresolvedSite {
            line,
            reason,
            detail: detail.to_owned(),
        });
        self
    }

    pub(crate) fn prefixes(mut self, prefixes: &[&str]) -> Self {
        self.prefixes = prefixes.iter().map(|prefix| (*prefix).to_owned()).collect();
        self
    }

    /// The commit the index was built at and how many files differed from it.
    pub(crate) fn commit(mut self, head: &str, dirty: u32) -> Self {
        self.head = Some(head.to_owned());
        self.dirty = dirty;
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
                        via: via(*role).to_owned(),
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
                .map(|connector| ConnectorCoverage {
                    projects: vec![".".to_owned()],
                    ..ConnectorCoverage::new(connector)
                })
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
                match &entry.status {
                    ParseStatus::Failed(failure) if failure.is_skipped() => {
                        counted.files += 1;
                        coverage.skipped.push(FailedFile {
                            file,
                            failure: failure.clone(),
                        });
                    }
                    status => counted.count_file(file, status),
                }
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
