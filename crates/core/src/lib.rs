//! The language-independent half of gsttt: the graph of files and contracts, the connector contract every
//! language and contract-format module implements, the engine that builds an index, and the queries over it.
//!
//! - [`connector`]: what a module implements.
//! - [`fact`], [`contract`], [`path`]: what a connector says about one file.
//! - [`rules`]: framework rule packs, read the same way by every language connector.
//! - [`setup`]: what a config asks for.
//! - [`tree`]: the ordered tree a JSON or YAML document is read into, with the line of every key.
//! - [`engine`], [`graph`], [`index`], [`coverage`]: what a run builds.
//! - [`query`]: what an answer asks.

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
pub mod text;
pub mod tree;

pub use connector::{
    Connector, DynConnector, DynProject, Erased, Meta, Parsed, Project, SourceFile,
};
pub use contract::{ContractKey, ContractKind, Role};
pub use graph::Graph;
pub use ids::{ContractId, FileId, Id, ProjectId};
pub use index::{Freshness, Index};
pub use path::RepoPath;
pub use repo::RepoView;
pub use setup::Setup;
