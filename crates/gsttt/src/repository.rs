//! The repository a command works on: its root, its config and its connectors, and what is read from them.
//!
//! The command line and the MCP server read a repository the same way, so both go through [`Repository`]. It keeps
//! nothing between calls but the evaluated config, and a caller that outlives edits to the config, the trust store
//! or the checked-out branch opens it again for every call.

use std::fmt::Write as _;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use gsttt_config::{CONFIG_PATH, ConfigError, External, PackError};
use gsttt_core::connector::ConnectorError;
use gsttt_core::engine::changes::Change;
use gsttt_core::engine::refresh::{Engine, IndexError};
use gsttt_core::engine::workspace::Workspace;
use gsttt_core::path::OutsideRoot;
use gsttt_core::{Freshness, Loaded, RepoPath, Setup, Target};

use crate::registry::Registry;
use crate::root::{self, RootError};
use crate::trust::{self, Declared, Store};

/// Why a repository cannot be opened or read.
///
/// The message names what failed and, where the caller can do something about it, what to do, so that a caller
/// without a terminal can pass it on as it is.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    /// There is no root to work on, or it is one gsttt refuses.
    #[error(transparent)]
    Root(#[from] RootError),

    /// A rule pack that ships with gsttt does not read.
    #[error(
        "{0}; this is a bug in gsttt, please report it at {issues}",
        issues = concat!(env!("CARGO_PKG_REPOSITORY"), "/issues")
    )]
    Packs(#[from] PackError),

    /// `.gsttt/config.toml`, or a pack beside it, does not read into a setup.
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// An `[[external]]` of the config cannot become a connector, though the user trusts it.
    #[error("an `[[external]]` of {CONFIG_PATH} is not valid: {0}")]
    External(#[source] ConnectorError),

    /// Options of the config that the module they are for cannot read: one error for each module, in the order of
    /// the config.
    #[error("the options in {CONFIG_PATH} are not valid:{}", lines(.0))]
    Options(Vec<ConnectorError>),

    /// A path argument is not a path inside the repository.
    #[error("{0}; name a file inside the repository")]
    Path(#[from] OutsideRoot),

    /// The changed files are asked for outside git, which has no history to compare with.
    #[error(
        "`{root}` is not in a git repository, so there is no history to take the changed files from; name the files instead"
    )]
    NoGit {
        /// The root, which has no git repository around it.
        root: Utf8PathBuf,
    },

    /// The engine cannot list, read or compare the files.
    #[error(transparent)]
    Index(#[from] IndexError),
}

/// What a refresh did, and the index it left.
#[derive(Clone, Debug)]
pub struct Refreshed {
    /// The index, which describes the tree as it was when the refresh listed it.
    pub loaded: Loaded,
    /// The stored index still described the tree, so nothing was parsed, resolved or linked.
    pub unchanged: bool,
    /// Files a connector parsed in this run.
    pub parsed: u32,
    /// Files whose facts came from a snapshot.
    pub reused: u32,
    /// How long the refresh took, which is not what the index records of the run that built it.
    pub elapsed: Duration,
}

/// A root, the config evaluated there and the connectors that read the tree.
#[derive(Debug)]
pub struct Repository {
    workspace: Workspace,
    registry: Registry,
    setup: Setup,
    /// The `[[external]]` entries of the config in its order, with whether the user trusts each here.
    externals: Vec<Declared>,
}

impl Repository {
    /// The repository around `start`, which should be a canonical directory: finds the root, refuses the home
    /// directory and `/`, builds the registry and reads `.gsttt/config.toml`. The external connectors of the config
    /// that the user trusts in this root join the registry after the built-in ones, in the order of the config.
    ///
    /// # Errors
    ///
    /// When there is no root, the root is refused, a built-in pack does not read, the config does not, a module
    /// cannot read its options or a trusted external does not become a connector.
    pub fn open(start: &Utf8Path) -> Result<Self, RepositoryError> {
        let workspace = root::locate(start)?;
        let registry = Registry::builtin()?;
        let setup = gsttt_config::evaluate(workspace.root().as_std_path(), &registry.connectors())?;
        let store = Store::locate()
            .inspect_err(|error| tracing::debug!(%error, "no trust store"))
            .ok();
        let externals = trust::declare(workspace.root(), &setup.externals, store.as_ref());
        let trusted = externals
            .iter()
            .filter(|external| external.trusted())
            .map(|external| External::new(external.setup()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(RepositoryError::External)?;
        let registry = registry.with_externals(trusted);
        let unreadable = unreadable_options(&setup, &registry);
        if !unreadable.is_empty() {
            return Err(RepositoryError::Options(unreadable));
        }
        Ok(Self {
            workspace,
            registry,
            setup,
            externals,
        })
    }

    /// The setup the config evaluated to.
    pub const fn setup(&self) -> &Setup {
        &self.setup
    }

    /// The connectors in registry order, and the built-in rule packs: the built-in connectors, then the
    /// externals the user trusts in this root.
    pub const fn registry(&self) -> &Registry {
        &self.registry
    }

    /// The connectors that find themselves in the tree, in registry order, whatever the config says about them.
    ///
    /// # Errors
    ///
    /// When the engine cannot list the files.
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "`Engine::detected` names one lifetime of the engine, and `with_engine` hands over any"
    )]
    pub fn detected(&self) -> Result<Vec<&'static str>, RepositoryError> {
        self.with_engine(|engine| engine.detected())
    }

    /// The `[[external]]` entries of the config in its order, each with whether the user trusts it in this root.
    pub fn externals(&self) -> &[Declared] {
        &self.externals
    }

    /// `index` as every answer reads it: with a line in its coverage problems for each external that is not
    /// trusted, so that a person who sees no file of it knows why and what to run.
    fn answered(&self, mut index: Index) -> Arc<Index> {
        let waiting = self
            .externals
            .iter()
            .filter(|external| !external.trusted())
            .map(Declared::problem);
        index.coverage.problems.extend(waiting);
        index.coverage.finish();
        Arc::new(index)
    }

    /// The top directory of the work tree, by its canonical path.
    pub fn root(&self) -> &Utf8Path {
        self.workspace.root()
    }

    /// Brings the stored index up to date with the tree and returns it. This is the call that can take minutes
    /// on the first index of a large repository.
    ///
    /// # Errors
    ///
    /// When the engine cannot list or read the files.
    pub fn refreshed(&self) -> Result<Refreshed, RepositoryError> {
        let (refresh, elapsed) = self.with_engine(|engine| {
            let started = Instant::now();
            let refresh = engine.refresh()?;
            Ok((refresh, started.elapsed()))
        })?;
        Ok(Refreshed {
            loaded: Loaded {
                index: self.answered(refresh.index),
                freshness: Freshness::Fresh,
            },
            unchanged: refresh.unchanged,
            parsed: refresh.parsed,
            reused: refresh.reused,
            elapsed,
        })
    }

    /// The stored index as it is, with how the tree has moved since, or `None` before the first index.
    ///
    /// # Errors
    ///
    /// When the engine cannot list the files to compare them with the index.
    pub fn stored(&self) -> Result<Option<Loaded>, RepositoryError> {
        self.with_engine(|engine| {
            let Some(index) = engine.load() else {
                return Ok(None);
            };
            let Some(freshness) = engine.freshness()? else {
                return Ok(None);
            };
            Ok(Some(Loaded {
                index: self.answered(index),
                freshness,
            }))
        })
    }

    /// `argument` read against `base`, a directory inside the root: the current directory after `-C` for the
    /// command line, the root itself for the server.
    ///
    /// # Errors
    ///
    /// When the argument is not a path inside the repository.
    pub fn argument(&self, base: &Utf8Path, argument: &str) -> Result<Target, RepositoryError> {
        let path = match RepoPath::from_argument(self.root(), base, argument) {
            Err(OutsideRoot::Leaves { .. }) => self.through_links(base, argument)?,
            read => read?,
        };
        // Only a path the disk says is not there counts as gone: one it cannot be asked about is still a file.
        let on_disk = match std::fs::symlink_metadata(self.root().join(path.as_str())) {
            Ok(_) => true,
            Err(error) => error.kind() != io::ErrorKind::NotFound,
        };
        Ok(Target { path, on_disk })
    }

    /// `argument` read a second time with the links in its path resolved. The root is a canonical path, so a path
    /// spelled through a link to a directory of the repository, as everything under `/tmp` is on macOS, leaves it by
    /// its text and still names a file inside it.
    fn through_links(&self, base: &Utf8Path, argument: &str) -> Result<RepoPath, OutsideRoot> {
        let leaves = || OutsideRoot::Leaves {
            argument: argument.to_owned(),
            root: self.root().to_owned(),
        };
        // Read against `/`, the text rule only folds the path into an absolute one and judges nothing.
        let top = Utf8Path::new("/");
        let folded = RepoPath::from_argument(top, base, argument).map_err(|_| leaves())?;
        let resolved = resolve_existing(&top.join(folded.as_str())).ok_or_else(leaves)?;
        RepoPath::from_argument(self.root(), base, resolved.as_str()).map_err(|error| match error {
            OutsideRoot::IsRoot { .. } => OutsideRoot::IsRoot {
                argument: argument.to_owned(),
            },
            OutsideRoot::Leaves { .. } => leaves(),
        })
    }

    /// The files that differ from `HEAD`, or from the best common ancestor of `HEAD` and `base`.
    ///
    /// # Errors
    ///
    /// When the root is in no git repository, `base` is no revision, or git history cannot be read.
    pub fn changed(&self, base: Option<&str>) -> Result<Vec<Change>, RepositoryError> {
        if self.workspace.git().is_none() {
            return Err(RepositoryError::NoGit {
                root: self.root().to_owned(),
            });
        }
        self.with_engine(|engine| engine.changed_files(base))
    }

    /// Runs `ask` with an engine over this repository.
    fn with_engine<T>(
        &self,
        ask: impl FnOnce(&Engine<'_>) -> Result<T, IndexError>,
    ) -> Result<T, RepositoryError> {
        let connectors = self.registry.connectors();
        let engine = Engine::new(
            self.workspace.clone(),
            &connectors,
            self.registry.packs(),
            &self.setup,
        );
        Ok(ask(&engine)?)
    }
}

/// The modules of `setup` whose options the connector of that name cannot read, in the order of the config. A module
/// that no connector is named after has nothing to read its options.
fn unreadable_options(setup: &Setup, registry: &Registry) -> Vec<ConnectorError> {
    let connectors = registry.connectors();
    setup
        .modules
        .iter()
        .filter_map(|module| {
            connectors
                .iter()
                .find(|connector| connector.meta().name == module.module)?
                .check_options(&module.options)
                .err()
        })
        .collect()
}

/// A line break and two spaces before each of `errors`, so that they read as a list under the line above them.
fn lines(errors: &[ConnectorError]) -> String {
    errors.iter().fold(String::new(), |mut text, error| {
        // Writing to a `String` cannot fail.
        let _ = write!(text, "\n  {error}");
        text
    })
}

/// `path` with its longest prefix that exists resolved through the filesystem and the rest kept as text, so that a
/// file that is gone can still be named through a link.
fn resolve_existing(path: &Utf8Path) -> Option<Utf8PathBuf> {
    path.ancestors().find_map(|ancestor| {
        let resolved = ancestor.canonicalize_utf8().ok()?;
        Some(resolved.join(path.strip_prefix(ancestor).ok()?))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server shares one `Repository` between the requests it answers at the same time.
    #[test]
    fn a_repository_can_be_shared_between_threads() {
        fn shared<T: Send + Sync>() {}
        shared::<Repository>();
        shared::<Loaded>();
        shared::<Refreshed>();
        shared::<RepositoryError>();
    }
}
