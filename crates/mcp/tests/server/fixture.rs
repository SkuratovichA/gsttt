//! A tree of files that import each other, turned into the `Index` the engine would build from it.
//!
//! The query tests of the core crate hold a fuller builder in `crates/core/tests/queries/fixture.rs`. It is private
//! to that test crate, so this one builds only what the server's answers need: files, their imports and one
//! connector that read them all.

use std::collections::BTreeMap;

use gsttt_core::coverage::{ConnectorCoverage, Coverage};
use gsttt_core::fact::{Import, ImportKind, ParseStatus, Resolution, ResolvedImport};
use gsttt_core::graph::{Edge, GraphBuilder};
use gsttt_core::index::{FileRecord, Index, IndexInfo, ProjectInfo};
use gsttt_core::{FileId, ProjectId, RepoPath};

/// The connector that reads every file of a test tree.
const CONNECTOR: &str = "ts";

/// Files and the files each one imports, by path.
#[derive(Debug, Default)]
pub(crate) struct Tree {
    files: BTreeMap<String, Vec<String>>,
}

impl Tree {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// `from` imports `to`; both files are in the tree.
    pub(crate) fn imports(mut self, from: &str, to: &str) -> Self {
        self.files.entry(to.to_owned()).or_default();
        self.files
            .entry(from.to_owned())
            .or_default()
            .push(to.to_owned());
        self
    }

    pub(crate) fn build(self) -> Index {
        let ids: BTreeMap<&str, FileId> = self
            .files
            .keys()
            .enumerate()
            .map(|(position, path)| {
                (
                    path.as_str(),
                    FileId::from_raw(u32::try_from(position).unwrap()),
                )
            })
            .collect();
        let paths: Vec<RepoPath> = self
            .files
            .keys()
            .map(|path| RepoPath::normalize(path).unwrap())
            .collect();

        let mut graph = GraphBuilder::new(paths);
        let mut coverage = Coverage {
            connectors: vec![ConnectorCoverage {
                projects: vec![".".to_owned()],
                ..ConnectorCoverage::new(CONNECTOR)
            }],
            ..Coverage::default()
        };
        let mut records = Vec::new();
        for (path, targets) in &self.files {
            let from = ids[path.as_str()];
            let imports: Vec<ResolvedImport> = targets
                .iter()
                .map(|target| {
                    let to = ids[target.as_str()];
                    let import = Import::new(format!("./{target}"), ImportKind::Static, 1);
                    graph.edge(from, to, Edge::new(import.kind, import.flags));
                    ResolvedImport {
                        import,
                        to: Resolution::File { file: to },
                    }
                })
                .collect();
            let counted = &mut coverage.connectors[0];
            counted.count_file(from, &ParseStatus::Ok);
            for import in &imports {
                counted.count_import(from, import);
            }
            records.push(FileRecord {
                project: Some(ProjectId::from_raw(0)),
                status: ParseStatus::Ok,
                imports,
                unresolved: Vec::new(),
            });
        }
        coverage.finish();

        Index {
            graph: graph.build(),
            projects: vec![ProjectInfo {
                connector: CONNECTOR.to_owned(),
                root: String::new(),
            }],
            records,
            coverage,
            info: IndexInfo {
                built_by: "0.0.0".to_owned(),
                head: Some("a1b2c3d".to_owned()),
                ..IndexInfo::default()
            },
            http_prefixes: Vec::new(),
        }
    }
}
