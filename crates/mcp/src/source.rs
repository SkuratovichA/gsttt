//! What the server asks of the repository, and what comes back.
//!
//! This crate cannot depend on the binary, which holds the root, the evaluated config, the registry and the trust
//! store. The binary implements [`IndexSource`] once, and the server only ever sees the trait.

use std::sync::Arc;

use gsttt_core::engine::changes::Change;
use gsttt_core::{Freshness, Index, RepoPath};

/// An index and how it relates to the tree it describes.
#[derive(Clone, Debug)]
pub struct Loaded {
    /// The index to answer from.
    pub index: Arc<Index>,
    /// Whether the tree has changed since the index was built.
    pub freshness: Freshness,
}

/// A path argument, read from the root of the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// The path from the root.
    pub path: RepoPath,
    /// Whether a file is there. A path with no file is asked about as deleted, so that the answer can still list what
    /// used to import it.
    pub on_disk: bool,
}

/// Why the source could not answer, as a sentence the model can act on.
///
/// The server hands the message to the model as it is, so it names what failed and says what to do about it.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct SourceError {
    /// The sentence the model reads.
    pub message: String,
}

impl SourceError {
    /// An error that says `message`.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Where the server gets its index.
///
/// Every method may read the disk, run git or build an index, so the server calls them on a blocking thread and one
/// at a time. The binary opens its repository again on every call, because a session outlives edits to the config,
/// the trust store and the checked-out branch.
pub trait IndexSource: Send + Sync + 'static {
    /// Refreshes the index and returns it. This is the call that can take minutes on the first index of a large
    /// repository.
    fn refreshed(&self) -> Result<Loaded, SourceError>;

    /// The stored index without refreshing it, or `None` before the first index.
    fn stored(&self) -> Result<Option<Loaded>, SourceError>;

    /// A path argument read from the root, and whether a file is there. A path that leaves the root is an error.
    fn path(&self, argument: &str) -> Result<Target, SourceError>;

    /// The files that differ from `HEAD`, or from the best common ancestor of `HEAD` and `base`.
    fn changed(&self, base: Option<&str>) -> Result<Vec<Change>, SourceError>;
}
