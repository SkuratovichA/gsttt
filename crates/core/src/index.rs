//! One run's result: the graph, what each file said, the projects, the coverage and where it all came from.

use serde::{Deserialize, Serialize};

use crate::coverage::Coverage;
use crate::fact::{ParseStatus, ResolvedImport, UnresolvedSite};
use crate::graph::Graph;
use crate::ids::{FileId, Id, ProjectId};

/// Everything a query reads. `records` has one entry per file of the graph, by [`FileId`], and `projects` one
/// per [`ProjectId`]; the accessors return `None` past either.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Index {
    pub graph: Graph,
    pub projects: Vec<ProjectInfo>,
    pub records: Vec<FileRecord>,
    pub coverage: Coverage,
    pub info: IndexInfo,
    /// [`Setup::http_prefixes`](crate::setup::Setup::http_prefixes) of the run, so a query that answers from a
    /// snapshot matches HTTP paths the way the config asked.
    pub http_prefixes: Vec<String>,
}

/// A project as answers name it.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ProjectInfo {
    pub connector: String,
    /// The directory it owns, `""` for the repository root.
    pub root: String,
}

/// What one file said, after resolution.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct FileRecord {
    /// The project that read it; `None` when no connector did.
    pub project: Option<ProjectId>,
    pub status: ParseStatus,
    pub imports: Vec<ResolvedImport>,
    /// Sites the connector saw but could not read.
    pub unresolved: Vec<UnresolvedSite>,
}

/// Where an index came from.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct IndexInfo {
    /// The gsttt version that built it; a snapshot from another version is rebuilt, never read.
    pub built_by: String,
    /// The commit checked out, `None` in a repository without commits.
    pub head: Option<String>,
    /// Files that differed from the commit: modified, added or untracked.
    pub dirty: u32,
    /// Files whose facts came from the cache.
    pub reused: u32,
    /// Files parsed by this run.
    pub parsed: u32,
    pub timings: Timings,
}

/// Wall time of each phase of a run, in microseconds.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Timings {
    /// Listing the files and reading their git status.
    pub files: u64,
    /// Hashing, cache lookups and parsing.
    pub parse: u64,
    pub resolve: u64,
    pub link: u64,
    /// Building the graph and the coverage.
    pub graph: u64,
    pub total: u64,
}

/// How an index stands against the working tree when an answer is read from it. The engine works it out and the
/// answers print it, so it lives here where both can reach it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Freshness {
    /// Built or refreshed for this answer, or nothing changed since it was built.
    Fresh,
    /// Read without a refresh from an index the tree has moved away from.
    Stale {
        added: u32,
        removed: u32,
        changed: u32,
    },
}

impl Freshness {
    /// `Fresh` when nothing was added, removed or changed.
    pub const fn since(added: u32, removed: u32, changed: u32) -> Self {
        if added == 0 && removed == 0 && changed == 0 {
            Self::Fresh
        } else {
            Self::Stale {
                added,
                removed,
                changed,
            }
        }
    }
}

impl Index {
    pub fn record(&self, file: FileId) -> Option<&FileRecord> {
        self.records.get(file.index())
    }

    pub fn project(&self, project: ProjectId) -> Option<&ProjectInfo> {
        self.projects.get(project.index())
    }

    /// The project that read `file`.
    pub fn project_of(&self, file: FileId) -> Option<&ProjectInfo> {
        self.record(file)
            .and_then(|record| record.project)
            .and_then(|project| self.project(project))
    }
}
