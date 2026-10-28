//! External connectors: a command that speaks JSON lines stands in for a language connector.
//!
//! [`External::new`] reads a `g.external { … }` of the config; wrapped in [`Erased`](gsttt_core::connector::Erased) it takes its place in a run beside the connectors of the registry. It claims the files its globs match, and the first request for one of them starts the process, so a run that is answered from the snapshot starts none. The messages are in [`protocol`].
//!
//! A process that exits, answers out of turn, sends a line that is not a message or takes longer than its timeout for one request fails every file it has not answered, as a panic that names the connector, and the run goes on. The process is not started again.

mod process;
pub mod protocol;

use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use globset::{Glob, GlobSet, GlobSetBuilder};
use gsttt_core::connector::{
    Connector, ConnectorError, Meta, Parsed, Project, Provide, SourceFile,
};
use gsttt_core::fact::{FileFacts, Import, ParseFailure, Resolution, UnresolvedReason};
use gsttt_core::rules::Pack;
use gsttt_core::setup::ExternalSetup;
use gsttt_core::{FileId, RepoPath, RepoView};
use xxhash_rust::xxh3::Xxh3;

use process::{Failure, Process};
pub use protocol::PROTOCOL;
use protocol::WireResolution;

/// A connector that is a command, from `g.external { name = "kotlin", cmd = { "gsttt-kotlin" }, files = { "**/*.kt" } }`.
#[derive(Clone, Debug)]
pub struct External {
    name: &'static str,
    program: String,
    arguments: Vec<String>,
    version: String,
    files: GlobSet,
    timeout: Duration,
}

impl External {
    /// How long the process gets to answer one request.
    pub const TIMEOUT: Duration = Duration::from_secs(10);

    /// The connector for `setup`. An empty command or a glob that does not parse is an error that names the connector.
    pub fn new(setup: &ExternalSetup) -> Result<Self, ConnectorError> {
        let options = |message: String| ConnectorError::Options {
            connector: setup.name.clone(),
            message,
        };
        let Some((program, arguments)) = setup.cmd.split_first() else {
            return Err(options(
                "`cmd` is empty: it needs the program and its arguments".to_owned(),
            ));
        };
        let files = glob_set(&setup.files).map_err(|error| options(format!("`files` {error}")))?;
        Ok(Self {
            name: intern(&setup.name),
            program: program.clone(),
            arguments: arguments.to_vec(),
            version: setup.version.clone(),
            files,
            timeout: Self::TIMEOUT,
        })
    }

    /// The same connector with another time for one request.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Mixed into the cache key of every file: a new release of the command, or a new protocol, drops the cached facts.
    fn salt(&self) -> u64 {
        let mut hash = Xxh3::new();
        hash.update(&PROTOCOL.to_le_bytes());
        hash.update(self.version.as_bytes());
        for part in std::iter::once(&self.program).chain(&self.arguments) {
            hash.update(&[0]);
            hash.update(part.as_bytes());
        }
        hash.digest()
    }
}

impl Connector for External {
    type Options = ();
    type Project = ExternalProject;

    fn meta(&self) -> Meta {
        Meta {
            name: self.name,
            version: PROTOCOL,
            summary: "A connector that runs as a command and speaks JSON lines",
            extensions: &[],
        }
    }

    fn detect(&self, repo: &RepoView) -> bool {
        repo.files()
            .any(|(_, path)| self.files.is_match(path.as_str()))
    }

    /// One project at the repository root when any file is claimed. The process is not started here.
    fn open(
        &self,
        repo: &RepoView,
        _options: &(),
        _packs: &[Pack],
    ) -> Result<Vec<ExternalProject>, ConnectorError> {
        if !self.detect(repo) {
            return Ok(Vec::new());
        }
        Ok(vec![ExternalProject {
            files: self.files.clone(),
            repo: repo.clone(),
            salt: self.salt(),
            session: Mutex::new(Session {
                name: self.name,
                program: self.program.clone(),
                arguments: self.arguments.clone(),
                root: repo.root().as_std_path().to_path_buf(),
                timeout: self.timeout,
                process: None,
                stopped: None,
            }),
        }])
    }
}

/// The files of an [`External`] in one repository, and the one process that reads them.
#[derive(Debug)]
pub struct ExternalProject {
    files: GlobSet,
    repo: RepoView,
    salt: u64,
    /// Requests come from many threads and go to one process one at a time.
    session: Mutex<Session>,
}

impl ExternalProject {
    fn lock(&self) -> MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The files of the repository an answer by path points at.
    fn locate(&self, wire: WireResolution) -> Resolution {
        let not_found = || Resolution::Unresolved(UnresolvedReason::NotFound);
        match wire {
            WireResolution::File { path } => self
                .repo
                .lookup(&path)
                .map_or_else(not_found, |file| Resolution::File { file }),
            WireResolution::Files { paths } => {
                let mut files: Vec<FileId> = paths
                    .iter()
                    .filter_map(|path| self.repo.lookup(path))
                    .collect();
                files.sort_unstable();
                files.dedup();
                if files.is_empty() {
                    not_found()
                } else {
                    Resolution::Files { files }
                }
            }
            WireResolution::External { package } => Resolution::External { package },
            WireResolution::Builtin => Resolution::Builtin,
            WireResolution::Unresolved(reason) => Resolution::Unresolved(reason.into()),
        }
    }
}

impl Project for ExternalProject {
    type Extra = ();

    fn root(&self) -> &'static str {
        ""
    }

    fn matches(&self, path: &RepoPath) -> bool {
        self.files.is_match(path.as_str())
    }

    fn salt(&self) -> u64 {
        self.salt
    }

    /// What the process said in `ready`. Asking never starts it, so this is empty until a request has.
    fn provides(&self) -> Vec<Provide> {
        self.lock()
            .process
            .as_ref()
            .map(|process| process.provides().to_vec())
            .unwrap_or_default()
    }

    fn parse(&self, file: &SourceFile<'_>) -> Parsed<()> {
        let path = file.path.as_str();
        let facts = self
            .lock()
            .run(format_args!("the parse of `{path}`"), |process| {
                process.parse(path, file.text)
            })
            .unwrap_or_else(|message| FileFacts::failed(ParseFailure::Panicked { message }));
        Parsed::new(facts)
    }

    /// After the process has stopped, the imports of the files it did parse are unresolved, with the reason it stopped.
    fn resolve(&self, from: &RepoPath, import: &Import) -> Resolution {
        let from = from.as_str();
        let spec = &import.spec;
        let answer = self.lock().run(
            format_args!("the resolve of `{spec}` in `{from}`"),
            |process| process.resolve(from, import),
        );
        match answer {
            Ok(wire) => self.locate(wire),
            Err(message) => Resolution::Unresolved(UnresolvedReason::Unsupported(message)),
        }
    }
}

/// A glob of an external's `files` that does not parse.
#[derive(Debug, thiserror::Error)]
#[error("holds `{glob}`: {source}")]
pub(crate) struct GlobError {
    glob: String,
    source: globset::Error,
}

impl GlobError {
    fn new(source: globset::Error) -> Self {
        Self {
            glob: source.glob().unwrap_or_default().to_owned(),
            source,
        }
    }
}

/// The globs of an external as one set.
pub(crate) fn glob_set(patterns: &[String]) -> Result<GlobSet, GlobError> {
    let mut set = GlobSetBuilder::new();
    for pattern in patterns {
        set.add(Glob::new(pattern).map_err(GlobError::new)?);
    }
    set.build().map_err(GlobError::new)
}

/// The state of the one process of a project: not started yet, running, or stopped for good.
#[derive(Debug)]
struct Session {
    name: &'static str,
    program: String,
    arguments: Vec<String>,
    root: PathBuf,
    timeout: Duration,
    process: Option<Process>,
    /// Why the process stopped. Every request after that fails with it, so a dead command is not started again.
    stopped: Option<String>,
}

impl Session {
    /// Runs `call` on the process, which the first call starts. A failure stops the process and is what every later call returns; `what` says which request it came on.
    fn run<T>(
        &mut self,
        what: fmt::Arguments<'_>,
        call: impl FnOnce(&mut Process) -> Result<T, Failure>,
    ) -> Result<T, String> {
        if let Some(message) = &self.stopped {
            return Err(message.clone());
        }
        self.call(call).map_err(|failure| {
            let message = format!(
                "external connector `{}` stopped at {what}: {failure}",
                self.name
            );
            tracing::warn!("{message}");
            if let Some(mut process) = self.process.take() {
                process.kill();
            }
            self.stopped = Some(message.clone());
            message
        })
    }

    fn call<T>(
        &mut self,
        call: impl FnOnce(&mut Process) -> Result<T, Failure>,
    ) -> Result<T, Failure> {
        let process = match &mut self.process {
            Some(process) => process,
            None => self.process.insert(Process::start(
                self.name,
                &self.program,
                &self.arguments,
                &self.root,
                self.timeout,
            )?),
        };
        call(process)
    }
}

/// The name of a connector as a `&'static str`, which is what [`Meta`] holds. A name is leaked once, however often it is opened, and a config declares a handful.
fn intern(name: &str) -> &'static str {
    static NAMES: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
    let mut names = NAMES.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(known) = names.get(name) {
        return known;
    }
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    names.insert(leaked);
    leaked
}
