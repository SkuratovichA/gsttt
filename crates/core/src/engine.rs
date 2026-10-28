//! The engine: from a repository and a [`Setup`](crate::setup::Setup) to an [`Index`](crate::index::Index).
//!
//! A run lists the files with git, hands each to the deepest project of the first connector that claims it, takes
//! its facts from the cache by connector, version, project salt and blob id or parses it, prepares each project,
//! resolves every import, routes embedded documents to the connector that reads their language, runs the link
//! phase, and builds the graph and the coverage.

pub mod pipeline;
