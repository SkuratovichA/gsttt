//! The middle of a run: open the connectors, give every file to a project, parse, prepare, resolve, read the
//! embedded documents and link.
//!
//! It reads files through a [`RepoView`], so a run over a directory of fixtures is the same code as a run over a
//! repository. The engine puts the file listing, the content ids, the fact cache, the graph and the snapshot
//! around it; a connector's tests call [`run`] on a fixture tree and snapshot [`Output::to_json`].

use std::cell::Cell;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::fmt;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::time::{Duration, Instant};

use foldhash::fast::RandomState;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{Map, Value};

use super::files::{FileState, Stamp};
use crate::connector::{
    ConnectorError, DynConnector, DynProject, LinkFile, LinkInput, LinkedFact, Meta, PrepareFile,
    Provide, SourceFile,
};
use crate::fact::{
    ContractFact, FileFacts, ParseFailure, Resolution, ResolvedImport, UnresolvedReason,
};
use crate::ids::{FileId, Id, ProjectId};
use crate::path::RepoPath;
use crate::repo::RepoView;
use crate::rules::Pack;

/// A connector chosen for a run and what it is opened with.
#[derive(Debug)]
pub struct Module<'a> {
    pub connector: &'a dyn DynConnector,
    /// Options as JSON; `null` means the defaults.
    pub options: Value,
    /// The rule packs of the connector's language.
    pub packs: Vec<Pack>,
}

/// What the middle of a run found, before the graph is built.
#[derive(Debug, Default)]
pub struct Output {
    /// The projects that opened, by module and then in the order their connector returned them.
    pub projects: Vec<ProjectRun>,
    /// Every file a project took, in id order.
    pub files: Vec<FileRun>,
    /// The files no project took, in id order.
    pub unclaimed: Vec<FileId>,
    /// Contract sites `link` found across files, in file order.
    pub linked: Vec<LinkedFact>,
    /// Connector calls that failed outside a single file's parse.
    pub problems: Vec<Problem>,
}

/// A project that opened.
#[derive(Debug)]
pub struct ProjectRun {
    pub connector: Meta,
    pub root: String,
    pub salt: u64,
    pub provides: Vec<Provide>,
}

/// A file and what its project made of it.
#[derive(Debug)]
pub struct FileRun {
    pub id: FileId,
    /// An index into [`Output::projects`].
    pub project: ProjectId,
    pub facts: FileFacts,
    /// The project's link data, as the cache stores it.
    pub extra: Vec<u8>,
    /// The imports of `facts`, in their order, each with where it points.
    pub imports: Vec<ResolvedImport>,
    /// The embedded documents of `facts`, in their order, as the project that reads their language read them.
    pub embedded: Vec<EmbeddedRun>,
    /// Whether `facts` came from a cache instead of a parse.
    pub reused: bool,
    /// The stamp of the bytes that were parsed, when the file had changed since the listing and so is no longer
    /// the one the listing stamped. A stamp without an id is a file that changed while it was being read.
    pub stamp: Option<Stamp>,
}

/// An embedded document after the embedded phase.
#[derive(Debug)]
pub struct EmbeddedRun {
    /// The connector that read it; `None` when no project reads its language, which coverage reports as a blind
    /// spot.
    pub read_by: Option<&'static str>,
    /// What the reader found, with lines counted in the host file: a fact on the second line of a document that
    /// starts on line 10 is on line 11.
    pub facts: Vec<ContractFact>,
}

/// A connector call that failed outside a single file's parse. The run goes on, and coverage carries the line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub connector: &'static str,
    /// The project's root, `None` when the connector itself did not open.
    pub root: Option<String>,
    pub call: Call,
    pub message: String,
}

/// The connector call a [`Problem`] comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    Detect,
    Open,
    Matches,
    Prepare,
    Resolve,
    ParseEmbedded,
    Link,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let call = match self.call {
            Call::Detect => "detect",
            Call::Open => "open",
            Call::Matches => "matches",
            Call::Prepare => "prepare",
            Call::Resolve => "resolve",
            Call::ParseEmbedded => "parse_embedded",
            Call::Link => "link",
        };
        match self.root.as_deref() {
            Some(root) => write!(
                f,
                "`{}` project at `{}`, {call}: {}",
                self.connector,
                display_root(root),
                self.message
            ),
            None => write!(f, "`{}`, {call}: {}", self.connector, self.message),
        }
    }
}

/// A file a project took, between the phases.
struct Owned<'a> {
    id: FileId,
    path: &'a RepoPath,
    project: usize,
}

/// A file a project took, with what besides its content decides its cached facts.
#[derive(Clone, Copy, Debug)]
pub struct Claim<'a> {
    pub id: FileId,
    pub path: &'a RepoPath,
    /// The index, in the modules given to [`assign_files`], of the module whose connector opened the project.
    pub module: usize,
    /// The index, in [`Assigned::projects`], of the project that took the file.
    pub project: usize,
}

/// Facts a cache already holds for a file. [`execute`] uses them in place of a parse.
#[derive(Debug)]
pub struct Seed {
    pub facts: FileFacts,
    /// The project's link data for the file, as the cache stores it.
    pub extra: Vec<u8>,
}

/// The projects that opened and the files they took: everything a run knows before it parses.
pub struct Assigned<'a> {
    projects: Vec<Box<dyn DynProject>>,
    module_of: Vec<usize>,
    owned: Vec<Owned<'a>>,
    unclaimed: Vec<FileId>,
    problems: Vec<Problem>,
}

impl fmt::Debug for Assigned<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Assigned")
            .field("projects", &self.projects.len())
            .field("claimed", &self.owned.len())
            .field("unclaimed", &self.unclaimed.len())
            .finish()
    }
}

impl<'a> Assigned<'a> {
    /// The files projects took, in id order.
    pub fn claims(&self) -> Vec<Claim<'a>> {
        self.owned
            .iter()
            .map(|file| Claim {
                id: file.id,
                path: file.path,
                module: self.module_of.get(file.project).copied().unwrap_or(0),
                project: file.project,
            })
            .collect()
    }

    /// The files no project took, in id order.
    pub fn unclaimed(&self) -> &[FileId] {
        &self.unclaimed
    }

    /// The projects that opened, in order.
    pub fn projects(&self) -> Vec<ProjectRun> {
        project_runs(&self.projects)
    }
}

/// How long the phases of [`execute`] took.
#[derive(Clone, Copy, Debug, Default)]
pub struct Phases {
    /// Reading and parsing files, or taking their facts from the seeds.
    pub parse: Duration,
    /// Preparing the projects, resolving imports and reading embedded documents.
    pub resolve: Duration,
    /// The link phase.
    pub link: Duration,
}

/// What the parse phase made of a file.
struct Made {
    facts: FileFacts,
    extra: Vec<u8>,
    reused: bool,
    stamp: Option<Stamp>,
}

/// Asks a connector whether the repository has anything for it. A `detect` that panics finds nothing, and the
/// panic becomes a problem in `problems`.
pub fn detect(connector: &dyn DynConnector, repo: &RepoView, problems: &mut Vec<Problem>) -> bool {
    contain(|| connector.detect(repo)).unwrap_or_else(|message| {
        problems.push(Problem {
            connector: connector.meta().name,
            root: None,
            call: Call::Detect,
            message,
        });
        false
    })
}

/// Opens every module and gives every file of `repo` to a project. Only a bad option fails; a connector that
/// cannot open becomes a problem in the output of [`execute`], and so do the `problems` the caller already has.
pub fn assign_files<'a>(
    repo: &'a RepoView,
    modules: &[Module<'_>],
    mut problems: Vec<Problem>,
) -> Result<Assigned<'a>, ConnectorError> {
    let Opened {
        projects,
        module_of,
    } = open(repo, modules, &mut problems)?;
    let roots = Roots::new(&projects);
    let mut reported = vec![false; projects.len()];
    let mut unclaimed = Vec::new();
    let mut owned = Vec::new();
    for (id, path) in repo.files() {
        match assign(
            &projects,
            &module_of,
            &roots,
            path,
            &mut reported,
            &mut problems,
        ) {
            Some(project) => owned.push(Owned { id, path, project }),
            None => unclaimed.push(id),
        }
    }
    Ok(Assigned {
        projects,
        module_of,
        owned,
        unclaimed,
        problems,
    })
}

/// Runs the middle of a run over every file of `repo`, with `modules` in registry order. Only a bad option stops
/// it; anything else a connector gets wrong, a panic included, ends up in the output.
pub fn run(
    repo: &RepoView,
    modules: &[Module<'_>],
    max_file_bytes: u64,
) -> Result<Output, ConnectorError> {
    let assigned = assign_files(repo, modules, Vec::new())?;
    Ok(execute(assigned, repo, max_file_bytes, &[], Vec::new()).0)
}

/// Parses, prepares, resolves, reads the embedded documents and links the files `assigned` holds. `seeds` has one
/// entry per claimed file, in [`Assigned::claims`] order; a file with a seed is not read or parsed. `listing` is
/// the file set `repo` was made from, in file id order: a file that no longer matches its stamp when it is read
/// comes back with the stamp of what was read, in [`FileRun::stamp`]. An empty listing checks nothing.
pub fn execute(
    assigned: Assigned<'_>,
    repo: &RepoView,
    max_file_bytes: u64,
    listing: &[FileState],
    mut seeds: Vec<Option<Seed>>,
) -> (Output, Phases) {
    let Assigned {
        mut projects,
        owned,
        unclaimed,
        mut problems,
        ..
    } = assigned;
    seeds.resize_with(owned.len(), || None);
    let mut members = vec![Vec::new(); projects.len()];
    for (index, file) in owned.iter().enumerate() {
        if let Some(list) = members.get_mut(file.project) {
            list.push(index);
        }
    }

    let started = Instant::now();
    let parsed: Vec<Made> = owned
        .par_iter()
        .zip(seeds)
        .map(|(file, seed)| {
            if let Some(Seed { facts, extra }) = seed {
                return Made {
                    facts,
                    extra,
                    reused: true,
                    stamp: None,
                };
            }
            let listed = listing.get(file.id.index()).map(|state| &state.stamp);
            match projects.get(file.project) {
                Some(project) => parse(repo, project.as_ref(), file, max_file_bytes, listed),
                None => Made {
                    facts: FileFacts::default(),
                    extra: Vec::new(),
                    reused: false,
                    stamp: None,
                },
            }
        })
        .collect();
    let parse_time = started.elapsed();

    let started = Instant::now();
    let prepared: Vec<Option<Problem>> = projects
        .par_iter_mut()
        .zip(members.par_iter())
        .map(|(project, members)| {
            let files: Vec<PrepareFile<'_>> = members
                .iter()
                .filter_map(|&index| {
                    let file = owned.get(index)?;
                    let made = parsed.get(index)?;
                    Some(PrepareFile {
                        id: file.id,
                        path: file.path,
                        facts: &made.facts,
                    })
                })
                .collect();
            contain(|| project.prepare(&files))
                .err()
                .map(|message| problem(project.as_ref(), Call::Prepare, message))
        })
        .collect();
    problems.extend(prepared.into_iter().flatten());

    let resolved: Vec<(Vec<ResolvedImport>, Vec<Problem>)> = owned
        .par_iter()
        .zip(parsed.par_iter())
        .map(|(file, made)| match projects.get(file.project) {
            Some(project) => resolve(project.as_ref(), file, &made.facts),
            None => (Vec::new(), Vec::new()),
        })
        .collect();

    let embedded: Vec<(Vec<EmbeddedRun>, Vec<Problem>)> = parsed
        .par_iter()
        .map(|made| read_embedded(&projects, &made.facts))
        .collect();
    let resolve_time = started.elapsed();

    let started = Instant::now();
    let linked: Vec<(Vec<LinkedFact>, Option<Problem>)> = projects
        .par_iter()
        .zip(members.par_iter())
        .map(|(project, members)| {
            let files = members
                .iter()
                .filter_map(|&index| {
                    let file = owned.get(index)?;
                    let made = parsed.get(index)?;
                    let (imports, _) = resolved.get(index)?;
                    Some(LinkFile {
                        id: file.id,
                        path: file.path,
                        facts: &made.facts,
                        imports: imports.as_slice(),
                        extra: made.extra.as_slice(),
                    })
                })
                .collect();
            let input = LinkInput::new(files);
            match contain(|| project.link(&input)) {
                Ok(Ok(facts)) => (facts, None),
                Ok(Err(error)) => (
                    Vec::new(),
                    Some(problem(project.as_ref(), Call::Link, error.to_string())),
                ),
                Err(message) => (
                    Vec::new(),
                    Some(problem(project.as_ref(), Call::Link, message)),
                ),
            }
        })
        .collect();
    let link_time = started.elapsed();

    let mut files = Vec::with_capacity(owned.len());
    for (((file, made), (imports, resolve_problems)), (embedded, embedded_problems)) in
        owned.iter().zip(parsed).zip(resolved).zip(embedded)
    {
        problems.extend(resolve_problems);
        problems.extend(embedded_problems);
        files.push(FileRun {
            id: file.id,
            project: ProjectId::from_index(file.project),
            facts: made.facts,
            extra: made.extra,
            imports,
            embedded,
            reused: made.reused,
            stamp: made.stamp,
        });
    }
    let mut linked_facts = Vec::new();
    for (facts, link_problem) in linked {
        linked_facts.extend(facts);
        problems.extend(link_problem);
    }
    linked_facts.sort_by_key(|fact| fact.file);

    let output = Output {
        projects: project_runs(&projects),
        files,
        unclaimed,
        linked: linked_facts,
        problems,
    };
    let phases = Phases {
        parse: parse_time,
        resolve: resolve_time,
        link: link_time,
    };
    (output, phases)
}

fn project_runs(projects: &[Box<dyn DynProject>]) -> Vec<ProjectRun> {
    projects
        .iter()
        .map(|project| ProjectRun {
            connector: project.connector(),
            root: project.root().to_owned(),
            salt: project.salt(),
            provides: project.provides(),
        })
        .collect()
}

/// The projects that opened, in module order, each with the index of its module.
struct Opened {
    projects: Vec<Box<dyn DynProject>>,
    module_of: Vec<usize>,
}

/// Opens every module. A connector that cannot open its projects is a problem; a bad option is an error.
fn open(
    repo: &RepoView,
    modules: &[Module<'_>],
    problems: &mut Vec<Problem>,
) -> Result<Opened, ConnectorError> {
    let mut projects = Vec::new();
    let mut module_of = Vec::new();
    for (index, module) in modules.iter().enumerate() {
        let connector = module.connector.meta().name;
        let failed = |root, message| Problem {
            connector,
            root,
            call: Call::Open,
            message,
        };
        match contain(|| module.connector.open(repo, &module.options, &module.packs)) {
            Ok(Ok(opened)) => {
                module_of.extend(std::iter::repeat_n(index, opened.len()));
                projects.extend(opened);
            }
            Ok(Err(error @ ConnectorError::Options { .. })) => return Err(error),
            Ok(Err(ConnectorError::Project { root, message, .. })) => {
                problems.push(failed(Some(root), message));
            }
            Ok(Err(error)) => problems.push(failed(None, error.to_string())),
            Err(message) => problems.push(failed(None, message)),
        }
    }
    Ok(Opened {
        projects,
        module_of,
    })
}

/// The projects by root, so a file is offered to the projects above it and not to every project of the run.
struct Roots<'p>(HashMap<&'p str, Vec<usize>, RandomState>);

impl<'p> Roots<'p> {
    /// Reads the root of every project once. The projects of a root stay in the order they were opened.
    fn new(projects: &'p [Box<dyn DynProject>]) -> Self {
        let mut by_root: HashMap<&str, Vec<usize>, RandomState> = HashMap::default();
        for (index, project) in projects.iter().enumerate() {
            by_root.entry(project.root()).or_default().push(index);
        }
        Self(by_root)
    }

    /// The projects rooted at `dir`.
    fn at(&self, dir: &str) -> &[usize] {
        self.0.get(dir).map_or(&[], Vec::as_slice)
    }
}

/// The project that owns `path`: of the first module with a project that takes it, the project with the deepest
/// root. A `matches` that panics takes nothing; `reported` keeps the first panic of each project the only problem
/// it adds, so a project that panics on every file does not fill the coverage.
fn assign(
    projects: &[Box<dyn DynProject>],
    module_of: &[usize],
    roots: &Roots<'_>,
    path: &RepoPath,
    reported: &mut [bool],
    problems: &mut Vec<Problem>,
) -> Option<usize> {
    let mut best = None;
    for dir in path.directories() {
        for &index in roots.at(dir) {
            let (Some(project), Some(&module)) = (projects.get(index), module_of.get(index)) else {
                continue;
            };
            let takes = contain(|| project.matches(path)).unwrap_or_else(|message| {
                if let Some(flag) = reported.get_mut(index)
                    && !std::mem::replace(flag, true)
                {
                    problems.push(problem(
                        project.as_ref(),
                        Call::Matches,
                        format!("`{path}`: {message}"),
                    ));
                }
                false
            });
            let key = (module, Reverse(depth(dir)), index);
            if takes && best.is_none_or(|current| key < current) {
                best = Some(key);
            }
        }
    }
    best.map(|(_, _, index)| index)
}

fn depth(root: &str) -> usize {
    if root.is_empty() {
        0
    } else {
        root.split('/').count()
    }
}

/// Reads a file and hands it to its project. A file that cannot be read, is past the size limit or is not UTF-8,
/// and a parse that panics, come back as failed facts. `listed` is the stamp the listing made of the file.
fn parse(
    repo: &RepoView,
    project: &dyn DynProject,
    file: &Owned<'_>,
    max_file_bytes: u64,
    listed: Option<&Stamp>,
) -> Made {
    let Read { text, stamp } = read(repo, file.path, max_file_bytes, listed);
    let (facts, extra) = match text {
        Ok(text) => {
            let source = SourceFile {
                id: file.id,
                path: file.path,
                text: &text,
            };
            contain(|| project.parse(&source)).unwrap_or_else(|message| {
                (
                    FileFacts::failed(ParseFailure::Panicked { message }),
                    Vec::new(),
                )
            })
        }
        Err(failure) => (FileFacts::failed(failure), Vec::new()),
    };
    Made {
        facts,
        extra,
        reused: false,
        stamp,
    }
}

/// A file as a run read it.
struct Read {
    /// Its text, or why it has none.
    text: Result<String, ParseFailure>,
    /// See [`FileRun::stamp`].
    stamp: Option<Stamp>,
}

fn read(repo: &RepoView, path: &RepoPath, max_file_bytes: u64, listed: Option<&Stamp>) -> Read {
    let unreadable = |error: io::Error| ParseFailure::Unreadable {
        message: error.to_string(),
    };
    let failed = |failure| Read {
        text: Err(failure),
        stamp: None,
    };
    let absolute = repo.absolute(path.as_str());
    let before = match absolute.metadata() {
        Ok(meta) => meta,
        Err(error) => return failed(unreadable(error)),
    };
    if before.len() > max_file_bytes {
        return failed(ParseFailure::TooLarge {
            bytes: before.len(),
            limit: max_file_bytes,
        });
    }
    let bytes = match std::fs::read(&absolute) {
        Ok(bytes) => bytes,
        Err(error) => return failed(unreadable(error)),
    };
    let stamp = listed.and_then(|listed| listed.restamp(&absolute, &before, &bytes));
    Read {
        text: String::from_utf8(bytes).map_err(|_| ParseFailure::NotUtf8),
        stamp,
    }
}

/// Resolves every import of a file. A resolve that panics leaves its import unresolved and is a problem.
fn resolve(
    project: &dyn DynProject,
    file: &Owned<'_>,
    facts: &FileFacts,
) -> (Vec<ResolvedImport>, Vec<Problem>) {
    let mut problems = Vec::new();
    let imports = facts
        .imports
        .iter()
        .map(|import| {
            let to = contain(|| project.resolve(file.path, import)).unwrap_or_else(|message| {
                let message = format!("`{}` in {}: {message}", import.spec, file.path);
                problems.push(problem(project, Call::Resolve, message));
                Resolution::Unresolved(UnresolvedReason::Unsupported(
                    "the connector panicked".to_owned(),
                ))
            });
            ResolvedImport {
                import: import.clone(),
                to,
            }
        })
        .collect();
    (imports, problems)
}

/// Hands each embedded document of a file to the first project, in registry order, that reads its language.
fn read_embedded(
    projects: &[Box<dyn DynProject>],
    facts: &FileFacts,
) -> (Vec<EmbeddedRun>, Vec<Problem>) {
    let mut problems = Vec::new();
    let runs = facts
        .embedded
        .iter()
        .map(|document| {
            let Some(reader) = projects
                .iter()
                .find(|project| project.embeds().contains(&document.lang.as_str()))
            else {
                return EmbeddedRun {
                    read_by: None,
                    facts: Vec::new(),
                };
            };
            let facts = contain(|| reader.parse_embedded(document)).unwrap_or_else(|message| {
                let message = format!(
                    "{} document on line {}: {message}",
                    document.lang, document.line
                );
                problems.push(problem(reader.as_ref(), Call::ParseEmbedded, message));
                Vec::new()
            });
            // A reader counts lines from the first line of its document, the host file from its own.
            let facts = facts
                .into_iter()
                .map(|mut fact| {
                    fact.line = document.line.saturating_add(fact.line).saturating_sub(1);
                    fact
                })
                .collect();
            EmbeddedRun {
                read_by: Some(reader.connector().name),
                facts,
            }
        })
        .collect();
    (runs, problems)
}

fn problem(project: &dyn DynProject, call: Call, message: String) -> Problem {
    Problem {
        connector: project.connector().name,
        root: Some(project.root().to_owned()),
        call,
        message,
    }
}

thread_local! {
    /// How many contained calls are running on this thread. The panic hook of [`quiet_contained_panics`] reads it
    /// on the thread that panics, which is the thread of the call, a rayon worker as often as not.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Marks the thread as inside a contained call for as long as it lives, however the call ends.
struct Containing;

impl Containing {
    fn enter() -> Self {
        // A thread that is being torn down has no counter; there is nothing to mute then.
        let _ = DEPTH.try_with(|depth| depth.set(depth.get().saturating_add(1)));
        Self
    }
}

impl Drop for Containing {
    fn drop(&mut self) {
        let _ = DEPTH.try_with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// Makes the panic hook stay silent about the panics the engine catches in connector calls, which are reported as
/// coverage lines, and leaves every other panic to the hook that was installed before. A binary calls it once, at
/// the start of `main`: without it the default hook prints each caught panic to stderr before the engine can
/// contain it.
pub fn quiet_contained_panics() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if DEPTH.try_with(Cell::get).unwrap_or(0) == 0 {
            previous(info);
        }
    }));
}

/// Runs a connector call and turns a panic into its message, so one bad file or project never ends a run.
fn contain<T>(call: impl FnOnce() -> T) -> Result<T, String> {
    let _containing = Containing::enter();
    panic::catch_unwind(AssertUnwindSafe(call)).map_err(|payload| {
        payload
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic without a message".to_owned())
    })
}

pub(super) fn display_root(root: &str) -> &str {
    if root.is_empty() { "." } else { root }
}

impl Output {
    /// The output as JSON, with paths in place of file ids and `<connector> <root>` in place of project ids, for
    /// snapshots and for looking at what a connector made of a tree.
    pub fn to_json(&self, repo: &RepoView) -> Value {
        let path = |id: FileId| {
            repo.path(id).map_or_else(
                || Value::String(id.to_string()),
                |path| Value::String(path.as_str().to_owned()),
            )
        };
        let projects = self
            .projects
            .iter()
            .map(|project| {
                let mut entry = Map::new();
                entry.insert("connector".to_owned(), project.connector.name.into());
                entry.insert("root".to_owned(), display_root(&project.root).into());
                entry.insert("provides".to_owned(), to_value(&project.provides));
                Value::Object(entry)
            })
            .collect();
        let files = self
            .files
            .iter()
            .map(|file| {
                let mut entry = object(&file.facts);
                entry.insert("path".to_owned(), path(file.id));
                let project =
                    self.projects
                        .get(file.project.index())
                        .map_or(Value::Null, |project| {
                            format!("{} {}", project.connector.name, display_root(&project.root))
                                .into()
                        });
                entry.insert("project".to_owned(), project);
                let imports = file
                    .imports
                    .iter()
                    .map(|resolved| {
                        let mut import = object(&resolved.import);
                        import.insert("to".to_owned(), resolution(&resolved.to, &path));
                        Value::Object(import)
                    })
                    .collect();
                entry.insert("imports".to_owned(), Value::Array(imports));
                let embedded = file
                    .facts
                    .embedded
                    .iter()
                    .zip(&file.embedded)
                    .map(|(document, run)| {
                        let mut document = object(document);
                        document.insert(
                            "read_by".to_owned(),
                            run.read_by.map_or(Value::Null, Value::from),
                        );
                        document.insert("contracts".to_owned(), to_value(&run.facts));
                        Value::Object(document)
                    })
                    .collect();
                entry.insert("embedded".to_owned(), Value::Array(embedded));
                Value::Object(entry)
            })
            .collect();
        let linked = self
            .linked
            .iter()
            .map(|linked| {
                let mut fact = object(&linked.fact);
                fact.insert("file".to_owned(), path(linked.file));
                if let Some(key) = &linked.replaces {
                    fact.insert("replaces".to_owned(), to_value(key));
                }
                Value::Object(fact)
            })
            .collect();
        let mut output = Map::new();
        output.insert("projects".to_owned(), Value::Array(projects));
        output.insert("files".to_owned(), Value::Array(files));
        output.insert("linked".to_owned(), Value::Array(linked));
        output.insert(
            "unclaimed".to_owned(),
            self.unclaimed.iter().map(|&id| path(id)).collect(),
        );
        output.insert(
            "problems".to_owned(),
            self.problems.iter().map(ToString::to_string).collect(),
        );
        Value::Object(output)
    }
}

fn resolution(to: &Resolution, path: &impl Fn(FileId) -> Value) -> Value {
    let single = |key: &str, value: Value| {
        let mut map = Map::new();
        map.insert(key.to_owned(), value);
        Value::Object(map)
    };
    match to {
        Resolution::File { file } => single("file", path(*file)),
        Resolution::Files { files } => {
            single("files", files.iter().map(|&file| path(file)).collect())
        }
        Resolution::External { package } => single("external", package.as_str().into()),
        Resolution::Builtin => "builtin".into(),
        Resolution::Unresolved(reason) => single("unresolved", to_value(reason)),
    }
}

fn to_value(value: &impl Serialize) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn object(value: &impl Serialize) -> Map<String, Value> {
    match to_value(value) {
        Value::Object(map) => map,
        other => {
            let mut map = Map::new();
            map.insert("value".to_owned(), other);
            map
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::connector::Erased;
    use crate::engine::testing::{
        Counters, Graphs, LIMIT, NOTES, Notes, fact, fixture, modules, tree,
    };
    use crate::fact::ParseStatus;

    fn file<'a>(output: &'a Output, repo: &RepoView, path: &str) -> &'a FileRun {
        output
            .files
            .iter()
            .find(|file| repo.path(file.id).unwrap().as_str() == path)
            .unwrap()
    }

    #[gsttt_scenario::scenario("indexing", "The deepest root wins")]
    #[test]
    fn files_go_to_the_deepest_project_of_the_first_module_that_takes_them() {
        let (_dir, repo) = fixture();
        let output = run(&repo, &modules(), LIMIT).unwrap();
        let json = output.to_json(&repo);
        let projects: Vec<_> = json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|project| format!("{} {}", project["connector"], project["root"]))
            .collect();
        assert_eq!(
            projects,
            [r#""notes" ".""#, r#""notes" "pkg""#, r#""graphs" ".""#]
        );
        let owner = |path: &str| {
            json["files"]
                .as_array()
                .unwrap()
                .iter()
                .find(|file| file["path"] == path)
                .unwrap()["project"]
                .clone()
        };
        assert_eq!(owner("a.txt"), "notes .");
        assert_eq!(owner("pkg/c.txt"), "notes pkg");
        assert_eq!(owner("q.graphql"), "graphs .");
        assert_eq!(json["unclaimed"], json!(["README.md", "pkg/ROOT"]));
    }

    #[test]
    fn a_nested_root_takes_its_own_files_and_a_longer_name_is_not_inside_it() {
        let (_dir, repo) = tree(&[
            ("a/ROOT", b""),
            ("a/b/ROOT", b""),
            ("ab/ROOT", b""),
            ("a/b/c.txt", b""),
            ("a/x.txt", b""),
            ("ab/y.txt", b""),
            ("z.txt", b""),
        ]);
        let output = run(&repo, &modules(), LIMIT).unwrap();
        let json = output.to_json(&repo);
        let owners: Vec<_> = json["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| format!("{} {}", file["path"], file["project"]))
            .collect();
        assert_eq!(
            owners,
            [
                r#""a/b/c.txt" "notes a/b""#,
                r#""a/x.txt" "notes a""#,
                r#""ab/y.txt" "notes ab""#,
                r#""z.txt" "notes .""#,
            ]
        );
    }

    #[test]
    fn a_file_is_tested_against_the_projects_above_it_only() {
        let names: Vec<String> = (0..12)
            .flat_map(|n| {
                [
                    format!("pkg{n}/ROOT"),
                    format!("pkg{n}/a.txt"),
                    format!("pkg{n}/b.txt"),
                ]
            })
            .collect();
        let files: Vec<(&str, &[u8])> =
            names.iter().map(|name| (name.as_str(), &b""[..])).collect();
        let (_dir, repo) = tree(&files);
        let counters = Counters::leak();
        let notes = Erased(Notes { counters });
        let modules = [Module {
            connector: &notes,
            options: Value::Null,
            packs: Vec::new(),
        }];
        let assigned = assign_files(&repo, &modules, Vec::new()).unwrap();
        // The root of every project is read once, to find its files, and not once per file and project.
        let projects = 12 + 1;
        assert!(counters.take_roots() <= projects);
        assert_eq!(assigned.claims().len(), 24);
    }

    #[gsttt_scenario::scenario("indexing", "A panicking matches leaves one file unclaimed")]
    #[test]
    fn a_panicking_matches_leaves_its_files_unclaimed_and_is_one_problem() {
        let (_dir, repo) = tree(&[
            ("a.txt", b""),
            ("explode.txt", b""),
            ("sub/explode.txt", b""),
        ]);
        let output = run(&repo, &modules(), LIMIT).unwrap();
        let json = output.to_json(&repo);
        assert_eq!(json["unclaimed"], json!(["explode.txt", "sub/explode.txt"]));
        let claimed: Vec<_> = output
            .files
            .iter()
            .map(|file| repo.path(file.id).unwrap().as_str())
            .collect();
        assert_eq!(claimed, ["a.txt"]);
        assert_eq!(
            json["problems"],
            json!([
                "`broken` project at `x`, open: unreadable manifest",
                "`notes` project at `.`, matches: `explode.txt`: boom in matches",
            ])
        );
    }

    #[gsttt_scenario::scenario("indexing", "Registry order decides between connectors")]
    #[test]
    fn the_first_registered_connector_takes_a_file_two_connectors_match() {
        let (_dir, repo) = fixture();
        let first = Erased(Graphs {
            counters: Counters::leak(),
            name: "first",
        });
        let second = Erased(Graphs {
            counters: Counters::leak(),
            name: "second",
        });
        let modules: Vec<Module<'_>> = [&first as &dyn DynConnector, &second]
            .into_iter()
            .map(|connector| Module {
                connector,
                options: Value::Null,
                packs: Vec::new(),
            })
            .collect();
        let output = run(&repo, &modules, LIMIT).unwrap();
        let owner = file(&output, &repo, "q.graphql").project;
        assert_eq!(output.projects[owner.index()].connector.name, "first");
    }

    #[test]
    fn imports_resolve_and_panics_stay_inside_their_file() {
        let (_dir, repo) = fixture();
        let output = run(&repo, &modules(), LIMIT).unwrap();
        let json = output.to_json(&repo);
        let a = json["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["path"] == "a.txt")
            .unwrap();
        let targets: Vec<_> = a["imports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|import| import["to"].clone())
            .collect();
        assert_eq!(
            targets,
            [
                json!({"file": "b.txt"}),
                json!({"unresolved": {"reason": "not-found"}}),
                json!({"unresolved": {"reason": "unsupported", "detail": "the connector panicked"}}),
            ]
        );
        assert_eq!(
            file(&output, &repo, "bad.txt").facts.status,
            ParseStatus::Failed(ParseFailure::Panicked {
                message: "boom in parse".to_owned()
            })
        );
        assert_eq!(
            file(&output, &repo, "big.txt").facts.status,
            ParseStatus::Failed(ParseFailure::TooLarge {
                bytes: 2000,
                limit: LIMIT
            })
        );
        assert_eq!(
            file(&output, &repo, "latin.txt").facts.status,
            ParseStatus::Failed(ParseFailure::NotUtf8)
        );
        assert_eq!(
            json["problems"],
            json!([
                "`broken` project at `x`, open: unreadable manifest",
                "`notes` project at `.`, resolve: `boom` in a.txt: boom in resolve",
            ])
        );
    }

    #[test]
    fn embedded_documents_go_to_the_project_that_reads_their_language() {
        let (_dir, repo) = fixture();
        let output = run(&repo, &modules(), LIMIT).unwrap();
        let a = file(&output, &repo, "a.txt");
        let readers: Vec<_> = a.embedded.iter().map(|run| run.read_by).collect();
        assert_eq!(readers, [Some("graphs"), None]);
        // The document starts on line 4 of the file, and the reader puts its fact on the document's first line.
        assert_eq!(
            a.embedded.first().unwrap().facts,
            [ContractFact {
                line: 4,
                ..fact("queue query-Q")
            }]
        );
    }

    #[test]
    fn link_sees_resolved_imports_across_files() {
        let (_dir, repo) = fixture();
        let output = run(&repo, &modules(), LIMIT).unwrap();
        let json = output.to_json(&repo);
        let linked: Vec<_> = json["linked"]
            .as_array()
            .unwrap()
            .iter()
            .map(|fact| fact["file"].clone())
            .collect();
        assert_eq!(linked, [json!("a.txt"), json!("pkg/c.txt")]);
    }

    #[test]
    fn a_linked_fact_shows_the_key_it_replaces_only_when_it_names_one() {
        let text = b"fact 1 serves http GET /a\nlinked 1 serves http GET /api/a replaces http GET /a\nlinked 1 serves http GET /b\n";
        let (_dir, repo) = tree(&[("a.txt", text)]);
        let output = run(&repo, &modules(), LIMIT).unwrap();
        let json = output.to_json(&repo);
        let linked = json["linked"].as_array().unwrap();
        assert_eq!(linked.len(), 2);
        assert_eq!(linked[0]["key"], json!("http GET /api/a"));
        assert_eq!(linked[0]["replaces"], json!("http GET /a"));
        assert_eq!(linked[1]["key"], json!("http GET /b"));
        assert!(linked[1].get("replaces").is_none(), "{:?}", linked[1]);
    }

    #[test]
    fn a_bad_option_stops_the_run() {
        let (_dir, repo) = fixture();
        let modules = [Module {
            connector: &NOTES,
            options: json!({"tag": 5}),
            packs: Vec::new(),
        }];
        let error = run(&repo, &modules, LIMIT).unwrap_err();
        let ConnectorError::Options { connector, message } = error else {
            panic!("expected an options error, got {error:?}")
        };
        assert_eq!(connector, "notes");
        assert!(message.contains("`tag`"), "{message}");
    }

    #[gsttt_scenario::scenario("indexing", "A contained panic is not printed")]
    #[test]
    fn a_contained_panic_is_muted_and_another_one_is_not() {
        use std::sync::{Arc, Mutex};

        // The recorder stands in for the default hook, so the test can see what reaches it. The test runs alone in
        // its process under nextest; the hook it replaces is put back before it asserts anything.
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let recorder = Arc::clone(&seen);
        let before = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            recorder.lock().unwrap().push(info.to_string());
        }));
        quiet_contained_panics();

        let contained = contain(|| panic!("contained"));
        let nested = contain(|| contain(|| panic!("nested")));
        (0..16).into_par_iter().for_each(|_| {
            drop(contain(|| panic!("on a worker")));
        });
        // The same thread again, after the contained calls have ended.
        let escaped = panic::catch_unwind(|| panic!("uncontained"));

        drop(panic::take_hook());
        panic::set_hook(before);
        assert_eq!(contained, Err("contained".to_owned()));
        assert_eq!(nested, Ok(Err("nested".to_owned())));
        assert!(escaped.is_err());
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert!(seen[0].contains("uncontained"), "{seen:?}");
    }

    #[test]
    fn output_does_not_depend_on_the_number_of_threads() {
        let (_dir, repo) = fixture();
        let with = |threads| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap()
                .install(|| run(&repo, &modules(), LIMIT).unwrap().to_json(&repo))
        };
        assert_eq!(with(1), with(8));
    }
}
