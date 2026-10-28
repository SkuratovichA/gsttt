//! The language-independent half of gsttt: the graph of files and contracts, the connector contract every
//! language and contract-format module implements, the engine that builds an index, and the queries over it.
//!
//! - [`connector`]: what a module implements.
//! - [`fact`], [`contract`], [`path`]: what a connector says about one file.
//! - [`rules`]: framework rule packs, read the same way by every language connector.
//! - [`setup`]: what a config asks for.
//! - [`engine`], [`graph`], [`index`], [`coverage`]: what a run builds.
//! - [`query`]: what an answer asks.
//! - [`atomic`]: replacing a file as a whole, so that a reader never sees half of it.

pub mod atomic;
pub mod connector;
pub mod contract;
pub mod coverage;
pub mod engine;
pub mod fact;
pub mod graph;
pub mod hex;
pub mod ids;
pub mod index;
pub mod path;
pub mod query;
pub mod repo;
pub mod rules;
pub mod setup;
pub mod text;

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
