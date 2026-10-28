//! The language-independent half of gsttt: the graph of files and contracts, the connector contract every language and contract-format module implements, the engine that builds an index, and the queries over it.
//!
//! - [`connector`]: what a module implements.
//! - [`fact`], [`contract`], [`path`]: what a connector says about one file.
//! - [`rules`]: framework rule packs, read the same way by every language connector.
//! - [`setup`]: what a config asks for.
//! - [`repo`]: the file set of one run, as connectors see it.
//! - [`tree`]: the ordered tree a JSON or YAML document is read into, with the line of every key.
//! - [`engine`], [`graph`], [`index`], [`coverage`]: what a run builds.
//! - [`ids`]: the dense ids that files and contracts get at index time, and the vectors and sets keyed by them.
//! - [`query`]: what an answer asks.
//! - [`text`]: text helpers shared by the connectors and by the words of answers: line numbers from byte offsets, counts, names and times.
//! - [`atomic`]: replacing a file as a whole, so that a reader never sees half of it.
//! - `testing`, behind the feature of that name: indexes written as lists of facts, for the tests of this crate and of those that put questions to an index.

pub mod atomic;
pub mod connector;
pub mod contract;
pub mod coverage;
pub mod engine;
pub mod fact;
pub mod graph;
pub mod ids;
pub mod index;
pub mod path;
pub mod query;
pub mod repo;
pub mod rules;
pub mod setup;
#[cfg(feature = "testing")]
pub mod testing;
pub mod text;
pub mod tree;

pub use connector::{
    Connector, DynConnector, DynProject, Erased, Meta, Parsed, Project, SourceFile,
};
pub use contract::{ContractKey, ContractKind, Role};
pub use graph::Graph;
pub use ids::{ContractId, FileId, Id, NameId, ProjectId};
pub use index::{Freshness, Index, Loaded};
pub use path::{RepoPath, Target};
pub use repo::RepoView;
pub use setup::Setup;

/// The version of this build of gsttt: `GSTTT_VERSION` as it was at build time, which every release sets, or the package version followed by `-dev` when it was unset or empty. `--version` prints it, the MCP server names itself with it, and a snapshot records the version that wrote it and is read only by the same one, because a release may parse a file differently from the release before without changing the shape of the snapshot.
pub const VERSION: &str = match option_env!("GSTTT_VERSION") {
    Some(version) if !version.is_empty() => version,
    _ => concat!(env!("CARGO_PKG_VERSION"), "-dev"),
};
