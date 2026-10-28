//! Refreshing an index: the one entry point the commands and the server call.
//!
//! A refresh lists the files, works out which facts it already has (from the snapshot of this work tree or of a
//! sibling work tree of the same repository), runs the pipeline over the rest and writes the snapshot back. When
//! nothing that shapes the index changed, it stops after opening the connectors and computing the fact keys.

use std::time::{Duration, Instant};

use xxhash_rust::xxh3::Xxh3;

use super::build::build_index;
use super::changes::{self, Change, ChangesError, Listing};
use super::files::{self, ContentId, Excluder, FileState, FilesError, Scanned};
use super::keys::{self, FactKey};
use super::packs::merge_packs;
use super::pipeline::{self, Claim, FileRun, Module, ProjectRun, Seed};
use super::snapshot::{self, ArchivedStored, Header, Snapshot, Stored, StoredFile};
use super::workspace::{Workspace, WorkspaceError};
use crate::connector::{ConnectorError, DynConnector};
use crate::coverage::Coverage;
use crate::fact::{FileFacts, ParseFailure, ParseStatus};
use crate::ids::Id;
use crate::index::{Freshness, Index, IndexInfo, Timings};
use crate::path::RepoPath;
use crate::repo::RepoView;
use crate::rules::Pack;
use crate::setup::Setup;

/// Why a run could not produce an index.
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Files(#[from] FilesError),
    #[error(transparent)]
    Connector(#[from] ConnectorError),
    #[error(transparent)]
    Changes(#[from] ChangesError),
}

/// What a refresh did.
#[derive(Debug)]
pub struct Refresh {
    pub index: Index,
    /// The stored index still described the tree, so nothing was parsed, resolved or linked.
    pub unchanged: bool,
    /// Files a connector parsed in this run.
    pub parsed: u32,
    /// Files whose facts came from a snapshot.
    pub reused: u32,
    /// Snapshots of other work trees that were read for facts.
    pub siblings: u32,
}

/// A work tree, the connectors that may read it and the setup that configures them.
#[derive(Debug)]
pub struct Engine<'a> {
    workspace: Workspace,
    connectors: &'a [&'a dyn DynConnector],
    builtin_packs: &'a [Pack],
    setup: &'a Setup,
}

impl<'a> Engine<'a> {
    /// An engine over `workspace`. `connectors` are in registry order, which decides between two connectors that
    /// claim the same file; `builtin_packs` are the rule packs that ship with the connectors.
    pub const fn new(
        workspace: Workspace,
        connectors: &'a [&'a dyn DynConnector],
        builtin_packs: &'a [Pack],
        setup: &'a Setup,
    ) -> Self {
        Self {
            workspace,
            connectors,
            builtin_packs,
            setup,
        }
    }

    /// The work tree this engine reads.
    pub const fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// The stored index, whether or not the tree has moved on since. `None` when there is no readable snapshot.
    pub fn load(&self) -> Option<Index> {
        let snapshot = Snapshot::open(self.workspace.snapshot_path().as_std_path())?;
        snapshot.stored()?.to_index()
    }

    /// The header of the stored snapshot, read without its body.
    pub fn peek(&self) -> Option<Header> {
        snapshot::peek(self.workspace.snapshot_path().as_std_path())
    }

    /// How the stored index stands against the tree, `None` when there is no snapshot.
    pub fn freshness(&self) -> Result<Option<Freshness>, IndexError> {
        let Some(snapshot) = Snapshot::open(self.workspace.snapshot_path().as_std_path()) else {
            return Ok(None);
        };
        let Some(stored) = snapshot.stored() else {
            return Ok(None);
        };
        let states = self.listing(Some(stored))?.files;
        let (mut added, mut changed, mut kept) = (0_u32, 0_u32, 0_usize);
        for state in &states {
            match stored.file(&state.path) {
                None => added += 1,
                Some(old) => {
                    kept += 1;
                    if old.stamp.id != state.stamp.id {
                        changed += 1;
                    }
                }
            }
        }
        let removed = to_u32(stored.files.len().saturating_sub(kept));
        Ok(Some(Freshness::since(added, removed, changed)))
    }

    /// The connectors that find themselves in the tree, in registry order, whatever the setup says about them: the
    /// ones `detect` alone would turn on. A connector whose `detect` panics is left out.
    pub fn detected(&self) -> Result<Vec<&'static str>, IndexError> {
        let snapshot = Snapshot::open(self.workspace.snapshot_path().as_std_path());
        let mut states = self
            .listing(snapshot.as_ref().and_then(Snapshot::stored))?
            .files;
        let repo = self.view_of(&mut states);
        let mut problems = Vec::new();
        let mut detected = Vec::new();
        for &connector in self.connectors {
            if pipeline::detect(connector, &repo, &mut problems) {
                detected.push(connector.meta().name);
            }
        }
        Ok(detected)
    }

    /// The files that differ from HEAD, or from the best common ancestor of HEAD and `base` when one is given.
    pub fn changed_files(&self, base: Option<&str>) -> Result<Vec<Change>, IndexError> {
        let snapshot = Snapshot::open(self.workspace.snapshot_path().as_std_path());
        let states = self
            .listing(snapshot.as_ref().and_then(Snapshot::stored))?
            .files;
        self.changes_since(&states, base)
    }

    /// Brings the stored index up to date with the tree and returns it.
    pub fn refresh(&self) -> Result<Refresh, IndexError> {
        let started = Instant::now();
        let listed_at = files::unix_now();
        let snapshot_path = self.workspace.snapshot_path();
        let snapshot = Snapshot::open(snapshot_path.as_std_path());
        let previous = snapshot.as_ref().and_then(Snapshot::stored);

        let Scanned {
            files: mut states,
            unreadable,
        } = self.listing(previous)?;
        let repo = self.view_of(&mut states);
        let listed = started.elapsed();

        let mut modules = Vec::new();
        let mut detect_problems = Vec::new();
        for &connector in self.connectors {
            let name = connector.meta().name;
            if self.setup.runs(name, || {
                pipeline::detect(connector, &repo, &mut detect_problems)
            }) {
                modules.push(Module {
                    connector,
                    options: self.setup.options(name).clone(),
                    packs: merge_packs(self.builtin_packs, self.setup, name),
                });
            }
        }
        let ran: Vec<&str> = modules
            .iter()
            .map(|module| module.connector.meta().name)
            .collect();

        let assigned = pipeline::assign_files(&repo, &modules, detect_problems)?;
        let claims = assigned.claims();
        let projects = assigned.projects();
        let module_states: Vec<_> = modules
            .iter()
            .map(|module| {
                keys::module_state(&module.connector.meta(), &module.options, &module.packs)
            })
            .collect();
        let mut file_keys: Vec<Option<FactKey>> = vec![None; states.len()];
        let mut unverifiable = false;
        for claim in &claims {
            let key = states
                .get(claim.id.index())
                .and_then(|state| claim_key(&module_states, &projects, claim, state.stamp.id));
            unverifiable |= key.is_none();
            if let Some(slot) = file_keys.get_mut(claim.id.index()) {
                *slot = key;
            }
        }
        let config = keys::config_fingerprint(self.setup, self.connectors, self.builtin_packs);
        let (head, problems) = self.head();

        let current = previous.filter(|stored| {
            !unverifiable
                && problems.is_empty()
                && is_current(
                    stored,
                    config,
                    &states,
                    &file_keys,
                    &projects,
                    head.as_deref(),
                )
        });
        // Only the index is copied out of the snapshot; the entries of the files are not needed.
        if let Some(index) = current.and_then(ArchivedStored::to_index) {
            return Ok(Refresh {
                reused: to_u32(claims.len()),
                index,
                unchanged: true,
                parsed: 0,
                siblings: 0,
            });
        }
        Ok(self.rebuild(Rebuild {
            repo: &repo,
            modules: &ran,
            assigned,
            claims,
            projects,
            module_states,
            states,
            file_keys,
            config,
            head,
            problems,
            unreadable,
            own: previous,
            started,
            listed_at,
            listed,
        }))
    }

    /// Runs the pipeline over the files that have no facts yet and writes the snapshot.
    fn rebuild(&self, run: Rebuild<'_, '_>) -> Refresh {
        let Rebuild {
            repo,
            modules,
            assigned,
            claims,
            projects,
            module_states,
            mut states,
            mut file_keys,
            config,
            head,
            mut problems,
            unreadable,
            own,
            started,
            listed_at,
            listed,
        } = run;
        let max = self.setup.max_file_bytes;
        let (seeds, siblings) = self.seeds(&claims, &states, &file_keys, own);

        let (output, phases) = pipeline::execute(assigned, repo, max, &states, seeds);
        // A file saved between the listing and the read is filed under the bytes that were read, not under the
        // id of the listing: a revert or a sibling must not find its facts under the old id.
        for (claim, run) in claims.iter().zip(&output.files) {
            let Some(stamp) = run.stamp else {
                continue;
            };
            if let Some(state) = states.get_mut(claim.id.index()) {
                state.stamp = stamp;
            }
            if let Some(slot) = file_keys.get_mut(claim.id.index()) {
                *slot = claim_key(&module_states, &projects, claim, stamp.id);
            }
        }
        let reused = output.files.iter().filter(|file| file.reused).count();
        let parsed = output.files.len().saturating_sub(reused);
        let dirty = self.dirty(&states, &mut problems);
        let info = IndexInfo {
            built_by: snapshot::VERSION.to_owned(),
            head,
            dirty,
            reused: to_u32(reused),
            parsed: to_u32(parsed),
            timings: Timings {
                files: micros(listed),
                parse: micros(phases.parse),
                resolve: micros(phases.resolve),
                link: micros(phases.link),
                graph: 0,
                total: 0,
            },
        };
        let mut index = build_index(repo, &output, modules, &self.setup.http_prefixes, info);
        index.info.timings.total = micros(started.elapsed());
        index.coverage.problems.extend(problems);
        index
            .coverage
            .problems
            .extend(unreadable.iter().map(|path| {
                format!("the directory `{path}` cannot be read: the files under it may be missing")
            }));
        index.coverage.finish();

        let header = Header {
            built_by: snapshot::VERSION.to_owned(),
            built_at: snapshot::now(),
            head: index.info.head.clone(),
            dirty,
            files: to_u32(states.len()),
            connectors: modules.iter().map(|name| (*name).to_owned()).collect(),
            coverage: coverage_line(&index.coverage),
        };
        let stored = Stored {
            index,
            config,
            listed_at,
            files: stored_files(states, file_keys, output.files),
        };
        // The index is built, so a snapshot that cannot be stored (a read-only `.git`, a full disk) costs the next
        // run its head start and nothing else.
        let snapshot_path = self.workspace.snapshot_path();
        if let Err(error) = snapshot::write(snapshot_path.as_std_path(), &header, &stored) {
            tracing::warn!(%error, "the snapshot was not stored; the index is returned without it");
        }
        let Stored { index, .. } = stored;
        Refresh {
            index,
            unchanged: false,
            parsed: to_u32(parsed),
            reused: to_u32(reused),
            siblings,
        }
    }

    /// The facts a snapshot already holds for each claimed file, from this work tree's snapshot first and then, when
    /// that one cannot answer for most files, from the snapshots of sibling work trees; with the number of those
    /// that were read. A file over the size limit never gets a seed: it is rejected before any cache is asked.
    fn seeds(
        &self,
        claims: &[Claim<'_>],
        states: &[FileState],
        file_keys: &[Option<FactKey>],
        own: Option<&ArchivedStored>,
    ) -> (Vec<Option<Seed>>, u32) {
        let max = self.setup.max_file_bytes;
        // The path and the key each claimed file's facts are filed under; a file over the size limit has none.
        let wanted: Vec<Option<(&RepoPath, FactKey)>> = claims
            .iter()
            .map(|claim| {
                let state = states
                    .get(claim.id.index())
                    .filter(|state| state.stamp.size <= max)?;
                let key = file_keys.get(claim.id.index()).copied().flatten()?;
                Some((&state.path, key))
            })
            .collect();
        let take = |stored: &ArchivedStored, want: Option<(&RepoPath, FactKey)>| {
            let (path, key) = want?;
            let (facts, extra) = stored.reusable(path, key)?;
            Some(Seed { facts, extra })
        };
        let mut seeds: Vec<Option<Seed>> = wanted
            .iter()
            .map(|&want| own.and_then(|stored| take(stored, want)))
            .collect();
        let missing = |seeds: &[Option<Seed>]| {
            seeds
                .iter()
                .zip(&wanted)
                .filter(|(seed, want)| seed.is_none() && want.is_some())
                .count()
        };
        let mut read = 0_u32;
        if own.is_none() || siblings_pay(missing(&seeds), claims.len()) {
            for path in self.workspace.sibling_snapshots() {
                if missing(&seeds) == 0 {
                    break;
                }
                // A snapshot of another version is rejected by its header, before its body is read.
                let Some(snapshot) = Snapshot::open(path.as_std_path()) else {
                    continue;
                };
                read += 1;
                let Some(sibling) = snapshot.stored() else {
                    continue;
                };
                for (seed, &want) in seeds.iter_mut().zip(&wanted) {
                    if seed.is_none() {
                        *seed = take(sibling, want);
                    }
                }
            }
        }
        (seeds, read)
    }

    /// The commit HEAD points at, and a line saying why it is unknown when git cannot tell. Outside git there is
    /// neither a commit nor a problem.
    fn head(&self) -> (Option<String>, Vec<String>) {
        if self.workspace.git().is_none() {
            return (None, Vec::new());
        }
        match changes::head_commit(self.workspace.root()) {
            Ok(head) => (head, Vec::new()),
            Err(error) => (None, vec![format!("HEAD is not known: {error}")]),
        }
    }

    /// How many files differ from HEAD. When git cannot say, the count is 0 and `problems` gets a line; outside git
    /// nothing is counted, and neither is anything when HEAD itself could not be read, which already has its line.
    fn dirty(&self, states: &[FileState], problems: &mut Vec<String>) -> u32 {
        if self.workspace.git().is_none() || !problems.is_empty() {
            return 0;
        }
        match self.changes_since(states, None) {
            Ok(changes) => to_u32(changes.len()),
            Err(error) => {
                problems.push(format!(
                    "the files that differ from HEAD are not counted: {error}"
                ));
                0
            }
        }
    }

    /// Puts `states` in path order, which is the order of the file ids, and the view the connectors detect against.
    fn view_of(&self, states: &mut [FileState]) -> RepoView {
        states.sort_by(|a, b| a.path.cmp(&b.path));
        RepoView::new(
            self.workspace.root().to_owned(),
            states.iter().map(|state| state.path.clone()).collect(),
        )
    }

    /// The files of the work tree, with the stamps of the previous snapshot to spare hashing what did not change.
    /// A stamp of a file written too close to the listing it was taken in is left out: the file is read again.
    fn listing(&self, previous: Option<&ArchivedStored>) -> Result<Scanned, FilesError> {
        let listed_at = previous.map(|stored| stored.listed_at.to_native());
        files::scan(
            &self.workspace,
            &self.setup.exclude,
            self.setup.max_file_bytes,
            &|path| {
                previous?
                    .stamp_of(path)
                    .filter(|stamp| listed_at.is_some_and(|at| stamp.is_settled(at)))
            },
        )
    }

    fn changes_since(
        &self,
        states: &[FileState],
        base: Option<&str>,
    ) -> Result<Vec<Change>, IndexError> {
        let root = self.workspace.root();
        let excluder = Excluder::new(root, &self.setup.exclude)?;
        let from = match base {
            Some(revision) => changes::base_tree(root, revision, &excluder)?,
            None => changes::head_tree(root, &excluder)?.unwrap_or_default(),
        };
        let to: Listing = states
            .iter()
            .map(|state| (state.path.clone(), state.stamp.id))
            .collect();
        Ok(changes::diff(&from, &to))
    }
}

/// How many files the own snapshot may lack facts for before the snapshots of sibling work trees are read.
const OWN_MISSES_FLOOR: usize = 64;

/// Whether the snapshots of sibling work trees are worth reading: the own snapshot lacks the facts of more than
/// 64 files and more than a twentieth of the claimed ones. A sibling is validated whole, which costs more than
/// parsing the handful of files a normal edit touches.
fn siblings_pay(misses: usize, claimed: usize) -> bool {
    misses > OWN_MISSES_FLOOR.max(claimed / 20)
}

/// What a rebuild needs from the steps before it.
struct Rebuild<'r, 'a> {
    repo: &'a RepoView,
    modules: &'r [&'r str],
    assigned: pipeline::Assigned<'a>,
    claims: Vec<Claim<'a>>,
    /// The projects that opened, in the order [`Claim::project`] counts them.
    projects: Vec<ProjectRun>,
    /// The part of a fact key that every file of a module shares, by module.
    module_states: Vec<Xxh3>,
    states: Vec<FileState>,
    file_keys: Vec<Option<FactKey>>,
    config: u128,
    head: Option<String>,
    /// Lines for the coverage about what git could not tell.
    problems: Vec<String>,
    /// The directories the listing could not read, which get a line of the coverage each.
    unreadable: Vec<String>,
    own: Option<&'r ArchivedStored>,
    started: Instant,
    /// The unix second at which the listing began.
    listed_at: i64,
    listed: Duration,
}

/// The key a claimed file's facts are filed under when its content has the id `id`. A file of unknown content has
/// none, so its facts are never reused.
fn claim_key(
    module_states: &[Xxh3],
    projects: &[ProjectRun],
    claim: &Claim<'_>,
    id: ContentId,
) -> Option<FactKey> {
    let module = module_states.get(claim.module)?;
    let project = projects.get(claim.project)?;
    (id != ContentId::UNKNOWN)
        .then(|| keys::file_key(module, &project.root, project.salt, claim.path, id))
}

/// Whether a stored index is what a run over this tree would build again: same configuration, same commit, same
/// projects, and for every file the same content and the same fact key. The content is compared for every file,
/// not only for those a project took, because the dirty count and the freshness of the snapshot depend on all of
/// them.
fn is_current(
    stored: &ArchivedStored,
    config: u128,
    states: &[FileState],
    file_keys: &[Option<FactKey>],
    projects: &[ProjectRun],
    head: Option<&str>,
) -> bool {
    stored.config.to_native() == config
        && stored.index.info.head.as_deref() == head
        && stored.index.projects.len() == projects.len()
        && stored
            .index
            .projects
            .iter()
            .zip(projects)
            .all(|(info, run)| info.connector == run.connector.name && info.root == run.root)
        && stored.files.len() == states.len()
        && stored
            .files
            .iter()
            .zip(states)
            .zip(file_keys)
            .all(|((old, new), key)| {
                old.path.as_str() == new.path.as_str()
                    && old.stamp.id == new.stamp.id
                    && old.key == *key
            })
}

/// The entries of the snapshot, one per file in file id order.
fn stored_files(
    states: Vec<FileState>,
    file_keys: Vec<Option<FactKey>>,
    runs: Vec<FileRun>,
) -> Vec<StoredFile> {
    let mut by_file: Vec<Option<FileRun>> = states.iter().map(|_| None).collect();
    for run in runs {
        if let Some(slot) = by_file.get_mut(run.id.index()) {
            *slot = Some(run);
        }
    }
    states
        .into_iter()
        .zip(file_keys)
        .zip(by_file)
        .map(|((state, key), run)| {
            let (facts, extra) = run.map_or_else(
                || (FileFacts::default(), Vec::new()),
                |run| (run.facts, run.extra),
            );
            // A failed read says nothing about the content, so the file is tried again: no key stands for it, and
            // the next run finds the stored key different from the one it computes.
            let key = key.filter(|_| {
                !matches!(
                    facts.status,
                    ParseStatus::Failed(ParseFailure::Unreadable { .. })
                )
            });
            StoredFile {
                path: state.path,
                key,
                stamp: state.stamp,
                facts,
                extra,
            }
        })
        .collect()
}

/// The coverage in one line, for the header of the snapshot.
fn coverage_line(coverage: &Coverage) -> String {
    let read: u32 = coverage.connectors.iter().map(|entry| entry.files).sum();
    let unclaimed: u32 = coverage.unclaimed.iter().map(|entry| entry.files).sum();
    format!(
        "{read} files read by {} connectors, {unclaimed} unclaimed",
        coverage.connectors.len()
    )
}

fn to_u32(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use camino::Utf8PathBuf;
    use serde_json::json;

    use super::*;
    use crate::connector::Erased;
    use crate::contract::ContractKey;
    use crate::engine::changes::ChangeKind;
    use crate::engine::testing::Locked;
    use crate::engine::testing::{
        Counters, Graphs, Notes, add_worktree, git, init_repo, touch, write,
    };
    use crate::fact::UnresolvedReason;
    use crate::setup::ModuleSetup;

    /// The two connectors of the fixtures, each with counters nobody else shares.
    struct Rig {
        notes: Erased<Notes>,
        graphs: Erased<Graphs>,
        counters: &'static Counters,
        graph_counters: &'static Counters,
    }

    impl Rig {
        fn new() -> Self {
            let counters = Counters::leak();
            let graph_counters = Counters::leak();
            Self {
                notes: Erased(Notes { counters }),
                graphs: Erased(Graphs {
                    counters: graph_counters,
                    name: "graphs",
                }),
                counters,
                graph_counters,
            }
        }

        fn connectors(&self) -> [&dyn DynConnector; 2] {
            [&self.notes, &self.graphs]
        }
    }

    fn workspace(dir: &tempfile::TempDir) -> Workspace {
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        Workspace::locate_with(&root, None).unwrap()
    }

    /// Indexes `dir` once with a new engine, the way a command does.
    fn index(rig: &Rig, dir: &tempfile::TempDir, setup: &Setup) -> Result<Refresh, IndexError> {
        let connectors = rig.connectors();
        Engine::new(workspace(dir), &connectors, &[], setup).refresh()
    }

    /// Indexes `dir`, a repository or not; outside a repository the snapshot goes into `cache`.
    fn index_at(rig: &Rig, dir: &Path, cache: &Path, setup: &Setup) -> Refresh {
        let connectors = rig.connectors();
        let root = Utf8PathBuf::from_path_buf(dir.to_path_buf()).unwrap();
        let cache = Utf8PathBuf::from_path_buf(cache.to_path_buf()).unwrap();
        let workspace = Workspace::locate_with(&root, Some(cache)).unwrap();
        Engine::new(workspace, &connectors, &[], setup)
            .refresh()
            .unwrap()
    }

    fn kinds(changes: Vec<Change>) -> Vec<(String, ChangeKind)> {
        changes
            .into_iter()
            .map(|change| (change.path.as_str().to_owned(), change.kind))
            .collect()
    }

    /// How many imports the facts of `path` hold.
    fn imports_of(index: &Index, path: &str) -> usize {
        let file = index.graph.file_id(path).unwrap();
        index.record(file).unwrap().imports.len()
    }

    /// The sites of the contract `key` as `path:line role`, none when the graph has no such contract.
    fn sites_of(index: &Index, key: &str) -> Vec<String> {
        let graph = &index.graph;
        let Some(contract) = graph.contract_id(&ContractKey::parse(key).unwrap()) else {
            return Vec::new();
        };
        graph
            .sites(contract)
            .iter()
            .map(|site| {
                let path = graph.path(site.file).unwrap();
                format!("{path}:{} {}", site.line, site.role)
            })
            .collect()
    }

    fn pack(name: &str, language: &str, detect: &str) -> Pack {
        serde_json::from_value(json!({"name": name, "language": language, "detect": [detect]}))
            .unwrap()
    }

    #[gsttt_scenario::scenario("indexing", "A clean tree is not parsed again")]
    #[test]
    fn a_clean_tree_is_not_parsed_again() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "use b.txt\n"), ("b.txt", "")]);
        let setup = Setup::default();
        let first = index(&rig, &dir, &setup).unwrap();
        assert_eq!((first.parsed, first.reused), (2, 0));
        assert_eq!(rig.counters.take()[0], 2);
        let second = index(&rig, &dir, &setup).unwrap();
        assert_eq!((second.parsed, second.reused), (0, 2));
        assert_eq!(rig.counters.take()[0], 0);
    }

    #[gsttt_scenario::scenario("indexing", "A file that moves to another project is parsed again")]
    #[test]
    fn a_file_that_moves_to_another_project_is_parsed_again() {
        let rig = Rig::new();
        let dir = init_repo(&[("pkg/a.txt", ""), ("top.txt", "")]);
        let setup = Setup::default();
        let first = index(&rig, &dir, &setup).unwrap();
        assert_eq!((first.parsed, first.reused), (2, 0));
        rig.counters.take();
        // The notes connector gives every project the salt 0, so only the root tells the two projects apart.
        write(dir.path(), "pkg/ROOT", "");
        let second = index(&rig, &dir, &setup).unwrap();
        assert_eq!((second.parsed, second.reused), (1, 1));
        assert_eq!(rig.counters.take()[0], 1);
    }

    #[gsttt_scenario::scenario("indexing", "Only edited files are parsed")]
    #[test]
    fn only_edited_files_are_parsed() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", ""), ("b.txt", ""), ("c.txt", "")]);
        let setup = Setup::default();
        let depends_on = |run: &Refresh, from: &str, to: &str| {
            let graph = &run.index.graph;
            let (from, to) = (graph.file_id(from).unwrap(), graph.file_id(to).unwrap());
            graph.dependencies(from).iter().any(|(file, _)| *file == to)
        };
        index(&rig, &dir, &setup).unwrap();
        rig.counters.take();
        // Saved long ago, so the stamp the run files for it is old enough to be trusted by the next one.
        write(dir.path(), "b.txt", "use a.txt\n");
        touch(
            &dir.path().join("b.txt"),
            SystemTime::now() - Duration::from_secs(3600),
        );
        let run = index(&rig, &dir, &setup).unwrap();
        assert_eq!((run.parsed, run.reused), (1, 2));
        assert_eq!(rig.counters.take()[0], 1);
        assert!(depends_on(&run, "b.txt", "a.txt"));

        // A save of the same size: only the modification time tells it from the one before.
        write(dir.path(), "b.txt", "use c.txt\n");
        let run = index(&rig, &dir, &setup).unwrap();
        assert_eq!((run.parsed, run.reused), (1, 2));
        assert_eq!(rig.counters.take()[0], 1);
        assert!(depends_on(&run, "b.txt", "c.txt"));
    }

    #[gsttt_scenario::scenario("indexing", "A same-size edit in the same clock tick is noticed")]
    #[test]
    fn a_same_size_edit_in_the_same_clock_tick_is_noticed() {
        let setup = Setup::default();
        let cache = tempfile::tempdir().unwrap();
        let repo = init_repo(&[("a.txt", "use b.txt\n"), ("b.txt", "")]);
        let plain = tempfile::tempdir().unwrap();
        write(plain.path(), "a.txt", "use b.txt\n");
        write(plain.path(), "b.txt", "");
        // In a repository a.txt is a modified file, which the git index cannot vouch for; outside one no index does.
        for dir in [repo.path(), plain.path()] {
            let rig = Rig::new();
            index_at(&rig, dir, cache.path(), &setup);
            rig.counters.take();
            let a = dir.join("a.txt");
            let saved = fs::metadata(&a).unwrap().modified().unwrap();
            fs::write(&a, "use c.txt\n").unwrap();
            touch(&a, saved);

            let run = index_at(&rig, dir, cache.path(), &setup);

            assert!(!run.unchanged, "{}", dir.display());
            assert_eq!(rig.counters.take()[0], 1, "{}", dir.display());
        }
    }

    #[gsttt_scenario::scenario("indexing", "An old file that did not change is not read again")]
    #[test]
    fn an_old_file_that_did_not_change_is_not_read_again() {
        use std::os::unix::fs::PermissionsExt;

        let setup = Setup::default();
        let cache = tempfile::tempdir().unwrap();
        let repo = init_repo(&[("a.txt", "use b.txt\n"), ("b.txt", "")]);
        let plain = tempfile::tempdir().unwrap();
        write(plain.path(), "b.txt", "");
        let an_hour_ago = SystemTime::now() - Duration::from_secs(3_600);
        for dir in [repo.path(), plain.path()] {
            let rig = Rig::new();
            // a.txt is modified in the repository, old.txt is untracked: the git index vouches for neither.
            write(dir, "a.txt", "use old.txt\n");
            write(dir, "old.txt", "");
            for name in ["a.txt", "old.txt"] {
                touch(&dir.join(name), an_hour_ago);
            }
            index_at(&rig, dir, cache.path(), &setup);
            for name in ["a.txt", "old.txt"] {
                fs::set_permissions(dir.join(name), fs::Permissions::from_mode(0o000)).unwrap();
            }
            // A process that can read the files anyway cannot show that they were not read.
            if fs::read(dir.join("old.txt")).is_ok() {
                return;
            }

            let run = index_at(&rig, dir, cache.path(), &setup);

            assert!(run.unchanged, "{}", dir.display());
        }
    }

    #[gsttt_scenario::scenario("indexing", "Changed options parse that module's files again")]
    #[test]
    fn changed_options_parse_that_modules_files_again() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", ""), ("b.txt", ""), ("q.graphql", "")]);
        let mut setup = Setup::default();
        index(&rig, &dir, &setup).unwrap();
        rig.counters.take();
        rig.graph_counters.take();
        setup.modules.push(ModuleSetup {
            module: "notes".to_owned(),
            options: json!({"tag": "x"}),
        });
        let run = index(&rig, &dir, &setup).unwrap();
        assert_eq!(run.parsed, 2);
        assert_eq!(
            (rig.counters.take()[0], rig.graph_counters.take()[0]),
            (2, 0)
        );
    }

    #[gsttt_scenario::scenario("indexing", "A new worktree reuses its sibling's facts")]
    #[test]
    fn a_new_worktree_reuses_its_siblings_facts() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "use b.txt\n"), ("b.txt", "")]);
        let setup = Setup::default();
        index(&rig, &dir, &setup).unwrap();
        rig.counters.take();
        let (_holder, root) = add_worktree(dir.path(), "feature");
        let connectors = rig.connectors();
        let engine = Engine::new(
            Workspace::locate_with(&root, None).unwrap(),
            &connectors,
            &[],
            &setup,
        );
        let run = engine.refresh().unwrap();
        assert_eq!((run.parsed, run.reused, run.siblings), (0, 2, 1));
        assert_eq!(rig.counters.take()[0], 0);
    }

    #[gsttt_scenario::scenario(
        "indexing",
        "Siblings are read only when the own snapshot cannot answer"
    )]
    #[test]
    fn siblings_are_read_only_when_the_own_snapshot_cannot_answer() {
        let rig = Rig::new();
        let names: Vec<String> = (0..80).map(|n| format!("f{n:02}.txt")).collect();
        let files: Vec<(&str, &str)> = names.iter().map(|name| (name.as_str(), "")).collect();
        let dir = init_repo(&files);
        let setup = Setup::default();
        let connectors = rig.connectors();
        let main = Engine::new(workspace(&dir), &connectors, &[], &setup);
        assert_eq!(main.refresh().unwrap().siblings, 0);

        // A linked work tree with seventy files edited. It has no snapshot of its own, so it reads the main one.
        let (_holder, linked) = add_worktree(dir.path(), "feature");
        for name in names.iter().take(70) {
            write(linked.as_std_path(), name, "use f79.txt\n");
        }
        let feature = Engine::new(
            Workspace::locate_with(&linked, None).unwrap(),
            &connectors,
            &[],
            &setup,
        );
        assert_eq!(feature.refresh().unwrap().siblings, 1);

        // One edit: the own snapshot answers for 79 files of 80, so the sibling is left alone.
        write(dir.path(), &names[0], "use f78.txt\n");
        let few = main.refresh().unwrap();
        assert_eq!((few.parsed, few.siblings), (1, 0));

        // Seventy edits that the sibling holds: the own snapshot lacks too many files, so the sibling is read once.
        for name in names.iter().take(70) {
            write(dir.path(), name, "use f79.txt\n");
        }
        let many = main.refresh().unwrap();
        assert_eq!((many.parsed, many.reused, many.siblings), (0, 80, 1));
    }

    #[gsttt_scenario::scenario("indexing", "A sibling of another version is not read")]
    #[test]
    fn a_sibling_of_another_version_is_not_read() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "use b.txt\n"), ("b.txt", "")]);
        let setup = Setup::default();
        let connectors = rig.connectors();
        let main = Engine::new(workspace(&dir), &connectors, &[], &setup);
        main.refresh().unwrap();
        // Make the snapshot look as if another version wrote it: one digit of the version in its header.
        let path = main.workspace().snapshot_path();
        let mut bytes = fs::read(&path).unwrap();
        let version = snapshot::VERSION.as_bytes();
        let at = bytes
            .windows(version.len())
            .position(|window| window == version)
            .unwrap();
        bytes[at] = if bytes[at] == b'9' { b'8' } else { b'9' };
        fs::write(&path, bytes).unwrap();
        assert!(main.peek().is_none());

        let (_holder, linked) = add_worktree(dir.path(), "feature");
        let feature = Engine::new(
            Workspace::locate_with(&linked, None).unwrap(),
            &connectors,
            &[],
            &setup,
        );
        let run = feature.refresh().unwrap();
        assert_eq!((run.parsed, run.siblings), (2, 0));
    }

    #[test]
    fn siblings_pay_past_64_misses_and_a_twentieth_of_the_files() {
        assert!(!siblings_pay(0, 100));
        assert!(!siblings_pay(64, 100));
        assert!(siblings_pay(65, 100));
        assert!(!siblings_pay(500, 10_000));
        assert!(siblings_pay(501, 10_000));
    }

    #[gsttt_scenario::scenario("indexing", "An oversized bundle is not parsed")]
    #[test]
    fn an_oversized_bundle_is_not_parsed() {
        let rig = Rig::new();
        let bytes = 3 * 1024 * 1024;
        let big = "x".repeat(bytes);
        let dir = init_repo(&[("a.txt", ""), ("b.txt", ""), ("big.txt", big.as_str())]);
        let setup = Setup::default();
        let bytes = u64::try_from(bytes).unwrap();
        assert!(setup.max_file_bytes < bytes);
        let run = index(&rig, &dir, &setup).unwrap();
        let id = run.index.graph.file_id("big.txt").unwrap();
        assert_eq!(
            run.index.record(id).unwrap().status,
            ParseStatus::Failed(ParseFailure::TooLarge {
                bytes,
                limit: setup.max_file_bytes
            })
        );
        assert_eq!(rig.counters.take()[0], 2);
    }

    #[gsttt_scenario::scenario("indexing", "A binary file is not parsed")]
    #[test]
    fn a_binary_file_is_not_parsed() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "")]);
        fs::write(dir.path().join("bin.txt"), [0xff_u8, 0xfe, 0x00]).unwrap();
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        let id = run.index.graph.file_id("bin.txt").unwrap();
        assert_eq!(
            run.index.record(id).unwrap().status,
            ParseStatus::Failed(ParseFailure::NotUtf8)
        );
        assert_eq!(rig.counters.take()[0], 1);
    }

    #[gsttt_scenario::scenario("indexing", "Prepare sees every file before resolution")]
    #[test]
    fn prepare_sees_every_file_before_resolution() {
        let rig = Rig::new();
        let dir = init_repo(&[
            ("a.txt", "use b.txt\n"),
            ("b.txt", ""),
            ("pkg/ROOT", ""),
            ("pkg/c.txt", "use a.txt\n"),
        ]);
        index(&rig, &dir, &Setup::default()).unwrap();
        let owned = |root: &str, files: &[&str]| {
            (
                root.to_owned(),
                files
                    .iter()
                    .map(|file| (*file).to_owned())
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            rig.counters.prepared(),
            [owned("", &["a.txt", "b.txt"]), owned("pkg", &["pkg/c.txt"])]
        );
        assert_eq!(rig.counters.resolved_before_prepare(), 0);
        assert!(rig.counters.take()[2] > 0);
    }

    #[gsttt_scenario::scenario("indexing", "An embedded document reaches its reader")]
    #[test]
    fn an_embedded_document_reaches_its_reader() {
        let rig = Rig::new();
        let host = format!("{}embed graphql @2 query Q\n", "x\n".repeat(9));
        let dir = init_repo(&[("a.txt", host.as_str())]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        let graph = &run.index.graph;
        let contract = graph
            .contract_id(&ContractKey::parse("queue query-Q").unwrap())
            .unwrap();
        let sites: Vec<_> = graph
            .sites(contract)
            .iter()
            .map(|site| (site.file, site.line))
            .collect();
        assert_eq!(sites, [(graph.file_id("a.txt").unwrap(), 11)]);
    }

    #[gsttt_scenario::scenario("indexing", "An embedded language nobody reads is a blind spot")]
    #[test]
    fn an_embedded_language_nobody_reads_is_a_blind_spot() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "embed sql select 1\n")]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        let blind = &run.index.coverage.connector("notes").unwrap().blind;
        assert_eq!(blind.len(), 1);
        assert_eq!(
            blind[0].reason,
            UnresolvedReason::Unsupported("embedded sql document".to_owned())
        );
        assert_eq!((blind[0].count, blind[0].first.line), (1, 1));
    }

    #[gsttt_scenario::scenario("indexing", "Linked facts become sites")]
    #[test]
    fn linked_facts_become_sites() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "use b.txt\n"), ("b.txt", "")]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        let graph = &run.index.graph;
        let contract = graph
            .contract_id(&ContractKey::parse("queue linked").unwrap())
            .unwrap();
        let files: Vec<_> = graph.sites(contract).iter().map(|site| site.file).collect();
        assert_eq!(files, [graph.file_id("a.txt").unwrap()]);
    }

    #[gsttt_scenario::scenario("indexing", "A linked fact replaces the own fact it re-keys")]
    #[test]
    fn a_linked_fact_replaces_the_own_fact_it_re_keys() {
        let rig = Rig::new();
        let text = "fact 1 serves http GET /bookings/{}\nlinked 1 serves http GET /api/bookings/{} replaces http GET /bookings/{}\n";
        let dir = init_repo(&[("a.txt", text)]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        assert_eq!(
            sites_of(&run.index, "http GET /api/bookings/{}"),
            ["a.txt:1 serves"]
        );
        let own = sites_of(&run.index, "http GET /bookings/{}");
        assert!(own.is_empty(), "{own:?}");
    }

    #[gsttt_scenario::scenario("indexing", "An own fact on another line keeps its site")]
    #[test]
    fn an_own_fact_on_another_line_keeps_its_site() {
        let rig = Rig::new();
        // Line 2 has the key that the linked fact names, so only the line tells the two own facts apart.
        let text = "fact 1 serves http GET /bookings/{}\nfact 2 serves http GET /bookings/{}\nlinked 1 serves http GET /api/bookings/{} replaces http GET /bookings/{}\n";
        let dir = init_repo(&[("a.txt", text)]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        assert_eq!(
            sites_of(&run.index, "http GET /api/bookings/{}"),
            ["a.txt:1 serves"]
        );
        assert_eq!(
            sites_of(&run.index, "http GET /bookings/{}"),
            ["a.txt:2 serves"]
        );
    }

    #[gsttt_scenario::scenario("indexing", "An own fact of another kind or role keeps its site")]
    #[test]
    fn an_own_fact_of_another_kind_or_role_keeps_its_site() {
        let rig = Rig::new();
        let text = "fact 1 serves http GET /bookings/{}\nfact 1 calls http GET /bookings/{}\nfact 1 serves queue bookings\nlinked 1 serves http GET /api/bookings/{} replaces http GET /bookings/{}\n";
        let dir = init_repo(&[("a.txt", text)]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        assert_eq!(
            sites_of(&run.index, "http GET /api/bookings/{}"),
            ["a.txt:1 serves"]
        );
        assert_eq!(
            sites_of(&run.index, "http GET /bookings/{}"),
            ["a.txt:1 calls"]
        );
        assert_eq!(sites_of(&run.index, "queue bookings"), ["a.txt:1 serves"]);
    }

    #[gsttt_scenario::scenario("indexing", "An own fact of another file keeps its site")]
    #[test]
    fn an_own_fact_of_another_file_keeps_its_site() {
        let rig = Rig::new();
        let declared = "fact 1 serves http GET /bookings/{}\n";
        let mounted = format!(
            "{declared}linked 1 serves http GET /api/bookings/{{}} replaces http GET /bookings/{{}}\n"
        );
        let dir = init_repo(&[("a.txt", mounted.as_str()), ("b.txt", declared)]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        assert_eq!(
            sites_of(&run.index, "http GET /api/bookings/{}"),
            ["a.txt:1 serves"]
        );
        assert_eq!(
            sites_of(&run.index, "http GET /bookings/{}"),
            ["b.txt:1 serves"]
        );
    }

    #[gsttt_scenario::scenario(
        "indexing",
        "A declaration mounted under two prefixes has two linked sites"
    )]
    #[test]
    fn a_declaration_mounted_under_two_prefixes_has_two_linked_sites() {
        let rig = Rig::new();
        let text = "fact 1 serves http GET /bookings/{}\nlinked 1 serves http GET /v1/bookings/{} replaces http GET /bookings/{}\nlinked 1 serves http GET /v2/bookings/{} replaces http GET /bookings/{}\n";
        let dir = init_repo(&[("a.txt", text)]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        for prefix in ["v1", "v2"] {
            assert_eq!(
                sites_of(&run.index, &format!("http GET /{prefix}/bookings/{{}}")),
                ["a.txt:1 serves"]
            );
        }
        let own = sites_of(&run.index, "http GET /bookings/{}");
        assert!(own.is_empty(), "{own:?}");
    }

    #[gsttt_scenario::scenario("indexing", "An own fact no linked fact names keeps its site")]
    #[test]
    fn an_own_fact_no_linked_fact_names_keeps_its_site() {
        let rig = Rig::new();
        let text = "fact 1 serves http GET /a\nfact 1 serves http GET /b\nlinked 1 serves http GET /api/a replaces http GET /a\n";
        let dir = init_repo(&[("a.txt", text)]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        assert_eq!(sites_of(&run.index, "http GET /api/a"), ["a.txt:1 serves"]);
        assert_eq!(sites_of(&run.index, "http GET /b"), ["a.txt:1 serves"]);
        let own = sites_of(&run.index, "http GET /a");
        assert!(own.is_empty(), "{own:?}");
    }

    #[gsttt_scenario::scenario("indexing", "A linked fact that names no own fact replaces nothing")]
    #[test]
    fn a_linked_fact_that_names_no_own_fact_replaces_nothing() {
        let rig = Rig::new();
        let text = "fact 1 serves http GET /b\nlinked 1 serves http GET /api/x\n";
        let dir = init_repo(&[("a.txt", text)]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        assert_eq!(sites_of(&run.index, "http GET /api/x"), ["a.txt:1 serves"]);
        assert_eq!(sites_of(&run.index, "http GET /b"), ["a.txt:1 serves"]);
    }

    #[gsttt_scenario::scenario("indexing", "An embedded fact is not replaced by a linked one")]
    #[test]
    fn an_embedded_fact_is_not_replaced_by_a_linked_one() {
        let rig = Rig::new();
        let text = "embed graphql query Q\nlinked 1 calls queue other replaces queue query-Q\n";
        let dir = init_repo(&[("a.txt", text)]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        assert_eq!(sites_of(&run.index, "queue other"), ["a.txt:1 calls"]);
        assert_eq!(sites_of(&run.index, "queue query-Q"), ["a.txt:1 calls"]);
    }

    #[gsttt_scenario::scenario("indexing", "A panicking parse fails one file")]
    #[test]
    fn a_panicking_parse_fails_one_file() {
        let rig = Rig::new();
        let dir = init_repo(&[
            ("ok1.txt", ""),
            ("ok2.txt", "use ok1.txt\n"),
            ("bad.txt", "boom\n"),
        ]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        let graph = &run.index.graph;
        assert_eq!(graph.file_count(), 3);
        let record = |path: &str| run.index.record(graph.file_id(path).unwrap()).unwrap();
        assert_eq!(
            record("bad.txt").status,
            ParseStatus::Failed(ParseFailure::Panicked {
                message: "boom in parse".to_owned()
            })
        );
        assert_eq!(record("ok1.txt").status, ParseStatus::Ok);
        assert_eq!(record("ok2.txt").imports.len(), 1);
    }

    #[gsttt_scenario::scenario("indexing", "A panicking link loses only linked facts")]
    #[test]
    fn a_panicking_link_loses_only_linked_facts() {
        let rig = Rig::new();
        let text = "use b.txt\nuse linkboom\nfact 1 serves http GET /bookings/{}\nlinked 1 serves http GET /api/bookings/{} replaces http GET /bookings/{}\n";
        let dir = init_repo(&[("a.txt", text), ("b.txt", "")]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        let graph = &run.index.graph;
        assert_eq!(graph.file_count(), 2);
        // The parsed fact stays, though the fact that would have re-keyed it was lost with the link.
        assert_eq!(graph.contract_count(), 1);
        assert_eq!(
            sites_of(&run.index, "http GET /bookings/{}"),
            ["a.txt:1 serves"]
        );
        let a = graph.file_id("a.txt").unwrap();
        assert_eq!(graph.dependencies(a).len(), 1);
        assert!(
            run.index
                .coverage
                .problems
                .iter()
                .any(|line| line.contains("`notes`")
                    && line.contains("project at `.`")
                    && line.contains("link")),
            "{:?}",
            run.index.coverage.problems
        );
    }

    #[gsttt_scenario::scenario("indexing", "A panicking detect disables its connector")]
    #[test]
    fn a_panicking_detect_disables_its_connector() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", ""), ("q.graphql", ""), ("DETECT_BOOM", "")]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        let ran: Vec<_> = run
            .index
            .coverage
            .connectors
            .iter()
            .map(|connector| connector.connector.as_str())
            .collect();
        assert_eq!(ran, ["graphs"]);
        assert_eq!(
            run.index.coverage.problems,
            ["`notes`, detect: boom in detect"]
        );
        assert_eq!(rig.counters.take()[0], 0);
    }

    #[gsttt_scenario::scenario("indexing", "Bad options fail the run")]
    #[test]
    fn bad_options_fail_the_run() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "")]);
        // An option the schema does not have, and an option of the wrong type.
        for (options, option) in [(json!({"nope": 5}), "nope"), (json!({"tag": 5}), "tag")] {
            let mut setup = Setup::default();
            setup.modules.push(ModuleSetup {
                module: "notes".to_owned(),
                options,
            });
            let error = index(&rig, &dir, &setup).unwrap_err().to_string();
            assert!(error.contains("notes") && error.contains(option), "{error}");
        }
    }

    #[gsttt_scenario::scenario("indexing", "One thread and eight agree")]
    #[test]
    fn one_thread_and_eight_agree() {
        // Enough files for the threads to really share the work, spread over projects: every file imports two
        // others (some in another project) and every ninth one a file that does not exist, every fifth one
        // carries an embedded document and every 97th one panics when it is parsed.
        const FILES: usize = 500;
        const PROJECTS: usize = 20;
        let path = |index: usize| format!("p{:02}/f{index:03}.txt", index % PROJECTS);
        let mut owned: Vec<(String, String)> = (0..FILES)
            .map(|index| {
                let mut lines = vec![
                    format!("use {}", path((index + 1) % FILES)),
                    format!("use {}", path(index * 7 % FILES)),
                ];
                if index % 9 == 0 {
                    lines.push(format!("use missing{index}.txt"));
                }
                if index % 5 == 0 {
                    lines.push(format!("embed graphql query Q{index}"));
                }
                if index % 97 == 0 {
                    lines.push("boom".to_owned());
                }
                (path(index), lines.join("\n") + "\n")
            })
            .collect();
        owned.extend((0..PROJECTS).map(|project| (format!("p{project:02}/ROOT"), String::new())));
        owned.push(("q.graphql".to_owned(), "type Query\n".to_owned()));
        let files: Vec<(&str, &str)> = owned
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        let built = |threads| {
            let rig = Rig::new();
            let dir = init_repo(&files);
            let mut index = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap()
                .install(|| index(&rig, &dir, &Setup::default()).unwrap().index);
            index.info = IndexInfo::default();
            index
        };
        let one = built(1);
        // The fixture has to reach the work the threads share, or agreeing on it proves nothing.
        let notes = &one.coverage.connectors[0];
        assert_eq!(notes.projects.len(), PROJECTS + 1);
        assert!(!notes.failed.is_empty() && !notes.unresolved.is_empty());
        assert!(one.graph.contract_count() > 0);
        assert_eq!(one, built(8));
    }

    #[gsttt_scenario::scenario("indexing", "A stored index loads back equal")]
    #[test]
    fn a_stored_index_loads_back_equal() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "use b.txt\n"), ("b.txt", "")]);
        let setup = Setup::default();
        let connectors = rig.connectors();
        let engine = Engine::new(workspace(&dir), &connectors, &[], &setup);
        let run = engine.refresh().unwrap();
        assert_eq!(engine.load().as_ref(), Some(&run.index));
    }

    #[gsttt_scenario::scenario(
        "indexing",
        "A snapshot that cannot be written does not fail the run"
    )]
    #[test]
    fn a_snapshot_that_cannot_be_written_does_not_fail_the_run() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "use b.txt\n"), ("b.txt", "")]);
        // A file where the state directory belongs, so nothing can be stored under it.
        fs::write(dir.path().join(".git").join("gsttt"), "").unwrap();
        let setup = Setup::default();
        let connectors = rig.connectors();
        let engine = Engine::new(workspace(&dir), &connectors, &[], &setup);
        let run = engine.refresh().unwrap();
        assert_eq!((run.index.graph.file_count(), run.parsed), (2, 2));
        assert!(engine.load().is_none());
    }

    #[gsttt_scenario::scenario("indexing", "A corrupt snapshot counts as none")]
    #[test]
    fn a_corrupt_snapshot_counts_as_none() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "use b.txt\n"), ("b.txt", "")]);
        let setup = Setup::default();
        let connectors = rig.connectors();
        let engine = Engine::new(workspace(&dir), &connectors, &[], &setup);
        engine.refresh().unwrap();
        let path = engine.workspace().snapshot_path();
        let bytes = fs::read(&path).unwrap();
        fs::write(&path, &bytes[..bytes.len() - 40]).unwrap();
        assert!(engine.load().is_none());
        let again = engine.refresh().unwrap();
        assert!(!again.unchanged);
        assert_eq!(engine.load().as_ref(), Some(&again.index));
    }

    #[gsttt_scenario::scenario("indexing", "Peek reads only the header")]
    #[test]
    fn peek_reads_only_the_header() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "use b.txt\n"), ("b.txt", "")]);
        let setup = Setup::default();
        let connectors = rig.connectors();
        let engine = Engine::new(workspace(&dir), &connectors, &[], &setup);
        let run = engine.refresh().unwrap();
        let path = engine.workspace().snapshot_path();
        let mut bytes = fs::read(&path).unwrap();
        // The prefix is 24 bytes (magic, layout, header length, hash of the body); the length is at byte 12.
        let header_len =
            usize::try_from(u32::from_le_bytes(bytes[12..16].try_into().unwrap())).unwrap();
        bytes.truncate(24 + header_len);
        bytes.extend([0xa5_u8; 64]);
        fs::write(&path, bytes).unwrap();
        let header = engine.peek().unwrap();
        assert_eq!(header.built_by, snapshot::VERSION);
        assert_eq!(header.head, run.index.info.head);
        assert_eq!((header.dirty, header.files), (0, 2));
        assert!(engine.load().is_none());
    }

    #[gsttt_scenario::scenario("indexing", "A no-op refresh calls no connector work")]
    #[test]
    fn a_no_op_refresh_calls_no_connector_work() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "use b.txt\n"), ("b.txt", "")]);
        let setup = Setup::default();
        let first = index(&rig, &dir, &setup).unwrap();
        assert!(!first.unchanged);
        rig.counters.take();
        let second = index(&rig, &dir, &setup).unwrap();
        assert!(second.unchanged);
        assert_eq!(rig.counters.take(), [0, 0, 0, 0]);
        assert_eq!(second.index, first.index);
    }

    #[gsttt_scenario::scenario("indexing", "An edit to an unclaimed file is not a no-op")]
    #[test]
    fn an_edit_to_an_unclaimed_file_is_not_a_no_op() {
        let rig = Rig::new();
        let dir = init_repo(&[
            ("a.txt", "use b.txt\n"),
            ("b.txt", ""),
            ("README.md", "one\n"),
        ]);
        let setup = Setup::default();
        let connectors = rig.connectors();
        let engine = Engine::new(workspace(&dir), &connectors, &[], &setup);
        assert_eq!(engine.refresh().unwrap().index.info.dirty, 0);
        write(dir.path(), "README.md", "two\n");
        let second = engine.refresh().unwrap();
        assert!(!second.unchanged);
        assert_eq!((second.parsed, second.reused), (0, 2));
        assert_eq!(second.index.info.dirty, 1);
        assert_eq!(engine.freshness().unwrap(), Some(Freshness::Fresh));
        assert!(engine.refresh().unwrap().unchanged);
    }

    #[gsttt_scenario::scenario(
        "indexing",
        "A file saved during a run is filed under what was read"
    )]
    #[test]
    fn a_file_saved_during_a_run_is_filed_under_what_was_read() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "use b.txt\n"), ("b.txt", "")]);
        let setup = Setup::default();
        let connectors = rig.connectors();
        let engine = Engine::new(workspace(&dir), &connectors, &[], &setup);
        // The run lists the committed file, then an editor saves it, then the run reads it.
        rig.counters
            .rewrite_on_next_open("a.txt", "use b.txt\nuse c.txt\n");
        let first = engine.refresh().unwrap();
        assert_eq!(first.parsed, 2);
        assert_eq!(imports_of(&first.index, "a.txt"), 2);
        rig.counters.take();

        // What was read is what the snapshot filed, so the tree is indexed as it is.
        let second = engine.refresh().unwrap();
        assert!(second.unchanged);
        assert_eq!(rig.counters.take(), [0, 0, 0, 0]);

        // Saving the committed text again is an edit: nothing is filed under that id yet.
        write(dir.path(), "a.txt", "use b.txt\n");
        let third = engine.refresh().unwrap();
        assert!(!third.unchanged);
        assert_eq!((third.parsed, third.reused), (1, 1));
        assert_eq!(imports_of(&third.index, "a.txt"), 1);
    }

    #[gsttt_scenario::scenario("indexing", "A file above the size limit does not stop a no-op")]
    #[test]
    fn a_file_above_the_limit_does_not_stop_a_no_op() {
        use std::os::unix::fs::PermissionsExt;

        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "use b.txt\n"), ("b.txt", "")]);
        write(dir.path(), "dump.txt", "0123456789012345678\n");
        let dump = dir.path().join("dump.txt");
        fs::set_permissions(&dump, fs::Permissions::from_mode(0o000)).unwrap();
        // A process that can read the file anyway cannot show that it is not read.
        if fs::read(&dump).is_ok() {
            return;
        }
        let setup = Setup {
            max_file_bytes: 12,
            ..Setup::default()
        };

        let first = index(&rig, &dir, &setup).unwrap();
        let file = first.index.graph.file_id("dump.txt").unwrap();
        assert!(matches!(
            first.index.record(file).unwrap().status,
            ParseStatus::Failed(ParseFailure::TooLarge { .. })
        ));
        assert!(index(&rig, &dir, &setup).unwrap().unchanged);
    }

    #[gsttt_scenario::scenario("indexing", "A file that could not be read is read again")]
    #[test]
    fn a_file_that_could_not_be_read_is_read_again() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "use b.txt\n"), ("b.txt", "")]);
        let setup = Setup::default();
        let connectors = rig.connectors();
        let engine = Engine::new(workspace(&dir), &connectors, &[], &setup);
        // The run lists the file, and then it is gone by the time the run reads it.
        rig.counters.remove_on_next_open("a.txt");
        let first = engine.refresh().unwrap();
        let file = first.index.graph.file_id("a.txt").unwrap();
        assert!(matches!(
            first.index.record(file).unwrap().status,
            ParseStatus::Failed(ParseFailure::Unreadable { .. })
        ));

        // The file is back as it was committed, so its content id and its key are the ones the first run listed.
        write(dir.path(), "a.txt", "use b.txt\n");
        let second = engine.refresh().unwrap();
        assert!(!second.unchanged);
        assert_eq!((second.parsed, second.reused), (1, 1));
        assert_eq!(imports_of(&second.index, "a.txt"), 1);
        assert!(engine.refresh().unwrap().unchanged);
    }

    #[gsttt_scenario::scenario("indexing", "A new commit is not a no-op")]
    #[test]
    fn a_new_commit_is_not_a_no_op() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "use b.txt\n"), ("b.txt", "")]);
        write(dir.path(), "a.txt", "use b.txt\n\n");
        let setup = Setup::default();
        let connectors = rig.connectors();
        let engine = Engine::new(workspace(&dir), &connectors, &[], &setup);
        let before = engine.refresh().unwrap();
        assert_eq!(before.index.info.dirty, 1);
        git(dir.path(), &["commit", "-q", "-am", "edit a"]);
        let after = engine.refresh().unwrap();
        assert!(!after.unchanged);
        assert_eq!((after.parsed, after.reused), (0, 2));
        assert_eq!(after.index.info.dirty, 0);
        assert_ne!(after.index.info.head, before.index.info.head);
        assert!(engine.refresh().unwrap().unchanged);
    }

    #[gsttt_scenario::scenario("indexing", "An edit after indexing makes the index stale")]
    #[test]
    fn an_edit_after_indexing_makes_the_index_stale() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "one\n"), ("b.txt", "two\n")]);
        let setup = Setup::default();
        let connectors = rig.connectors();
        let engine = Engine::new(workspace(&dir), &connectors, &[], &setup);
        engine.refresh().unwrap();
        assert_eq!(engine.freshness().unwrap(), Some(Freshness::Fresh));
        write(dir.path(), "a.txt", "one, edited\n");
        write(dir.path(), "c.txt", "three\n");
        assert_eq!(
            engine.freshness().unwrap(),
            Some(Freshness::Stale {
                added: 1,
                removed: 0,
                changed: 1
            })
        );
    }

    #[gsttt_scenario::scenario("indexing", "Work tree changes are listed by kind")]
    #[test]
    fn work_tree_changes_are_listed_by_kind() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "one\n"), ("b.txt", "two\n"), ("c.txt", "three\n")]);
        write(dir.path(), "a.txt", "one, edited\n");
        fs::remove_file(dir.path().join("b.txt")).unwrap();
        write(dir.path(), "d.txt", "four\n");
        let setup = Setup::default();
        let connectors = rig.connectors();
        let engine = Engine::new(workspace(&dir), &connectors, &[], &setup);
        assert_eq!(
            kinds(engine.changed_files(None).unwrap()),
            [
                ("a.txt".to_owned(), ChangeKind::Modified),
                ("b.txt".to_owned(), ChangeKind::Deleted),
                ("d.txt".to_owned(), ChangeKind::Added),
            ]
        );
    }

    #[test]
    fn detected_lists_the_connectors_that_find_themselves_whatever_the_setup_says() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "one\n")]);
        let setup = Setup {
            detect: false,
            disable: vec!["graphs".to_owned()],
            ..Setup::default()
        };
        let connectors = rig.connectors();
        let engine = Engine::new(workspace(&dir), &connectors, &[], &setup);

        assert_eq!(engine.detected().unwrap(), ["notes", "graphs"]);

        // A connector whose `detect` panics finds nothing, and the others are still asked.
        write(dir.path(), "DETECT_BOOM", "");
        assert_eq!(engine.detected().unwrap(), ["graphs"]);
    }

    #[gsttt_scenario::scenario("indexing", "A HEAD git cannot read is a coverage line")]
    #[test]
    fn a_head_git_cannot_read_is_a_coverage_line() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "")]);
        // A branch that points at a commit the object database does not hold.
        let branch = dir
            .path()
            .join(".git")
            .join("refs")
            .join("heads")
            .join("main");
        fs::write(branch, format!("{}\n", "1".repeat(40))).unwrap();
        let setup = Setup::default();
        let connectors = rig.connectors();
        let engine = Engine::new(workspace(&dir), &connectors, &[], &setup);
        for _ in 0..2 {
            let run = engine.refresh().unwrap();
            assert!(!run.unchanged);
            assert_eq!(
                (run.index.info.head.as_deref(), run.index.info.dirty),
                (None, 0)
            );
            let problems = &run.index.coverage.problems;
            assert!(
                problems.iter().any(|line| line.contains("HEAD")),
                "{problems:?}"
            );
        }
    }

    #[gsttt_scenario::scenario("indexing", "A history git cannot read is a coverage line")]
    #[test]
    fn a_history_git_cannot_read_is_a_coverage_line() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "")]);
        let tree = git(dir.path(), &["rev-parse", "HEAD^{tree}"]);
        let (fan_out, rest) = tree.trim().split_at(2);
        let object = dir
            .path()
            .join(".git")
            .join("objects")
            .join(fan_out)
            .join(rest);
        fs::remove_file(object).unwrap();
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        assert!(run.index.info.head.is_some());
        assert_eq!(run.index.info.dirty, 0);
        let problems = &run.index.coverage.problems;
        assert!(
            problems
                .iter()
                .any(|line| line.contains("differ from HEAD")),
            "{problems:?}"
        );
    }

    #[gsttt_scenario::scenario(
        "indexing",
        "A tracked directory that cannot be read does not fail the run"
    )]
    #[test]
    fn a_tracked_directory_that_cannot_be_read_does_not_fail_the_run() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", ""), ("locked/b.txt", "")]);
        write(dir.path(), "new.txt", "");
        let Some(_locked) = Locked::new(&dir.path().join("locked")) else {
            return;
        };

        let run = index(&rig, &dir, &Setup::default()).unwrap();

        let graph = &run.index.graph;
        assert!(graph.file_id("a.txt").is_some() && graph.file_id("new.txt").is_some());
        assert!(graph.file_id("locked/b.txt").is_none());
        let problems = &run.index.coverage.problems;
        assert!(
            problems.iter().any(|line| line.contains("`locked`")),
            "{problems:?}"
        );

        // The stored index keeps the line, so the directory does not make every run a rebuild.
        let again = index(&rig, &dir, &Setup::default()).unwrap();
        assert!(again.unchanged);
        assert_eq!(again.index.coverage.problems, *problems);
    }

    /// Indexes a tree outside git whose directory `locked` has the permissions `mode`: the run succeeds, the index
    /// leaves `locked/b.txt` out and the coverage names `locked`. A process that can reach the directory anyway
    /// checks nothing.
    fn assert_directory_reported_outside_git(mode: u32) {
        let rig = Rig::new();
        let plain = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        write(plain.path(), "a.txt", "");
        write(plain.path(), "locked/b.txt", "");
        let Some(_locked) = Locked::with_mode(&plain.path().join("locked"), mode) else {
            return;
        };

        let run = index_at(&rig, plain.path(), cache.path(), &Setup::default());

        let graph = &run.index.graph;
        assert!(graph.file_id("a.txt").is_some(), "mode {mode:o}");
        assert!(graph.file_id("locked/b.txt").is_none(), "mode {mode:o}");
        let problems = &run.index.coverage.problems;
        assert!(
            problems.iter().any(|line| line.contains("`locked`")),
            "mode {mode:o}: {problems:?}"
        );
    }

    /// Indexes a repository, gives its tracked directory `locked` the permissions `mode` and indexes it again: the
    /// files of `locked` have left the index, they count as removed from HEAD and the coverage names `locked`. A
    /// process that can reach the directory anyway checks nothing.
    fn assert_tracked_files_removed(mode: u32) {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", ""), ("locked/b.txt", "")]);
        let setup = Setup::default();
        let first = index(&rig, &dir, &setup).unwrap();
        assert!(first.index.graph.file_id("locked/b.txt").is_some());
        let Some(_locked) = Locked::with_mode(&dir.path().join("locked"), mode) else {
            return;
        };

        let run = index(&rig, &dir, &setup).unwrap();

        assert!(
            run.index.graph.file_id("locked/b.txt").is_none(),
            "mode {mode:o}"
        );
        assert_eq!(run.index.info.dirty, 1, "mode {mode:o}");
        let connectors = rig.connectors();
        let changes = Engine::new(workspace(&dir), &connectors, &[], &setup)
            .changed_files(None)
            .unwrap();
        assert_eq!(
            kinds(changes),
            [("locked/b.txt".to_owned(), ChangeKind::Deleted)],
            "mode {mode:o}"
        );
        let problems = &run.index.coverage.problems;
        assert!(
            problems.iter().any(|line| line.contains("`locked`")),
            "mode {mode:o}: {problems:?}"
        );
    }

    #[gsttt_scenario::scenario(
        "indexing",
        "A directory outside git that cannot be read does not fail the run"
    )]
    #[test]
    fn a_directory_outside_git_that_cannot_be_read_does_not_fail_the_run() {
        assert_directory_reported_outside_git(0o000);
    }

    #[gsttt_scenario::scenario(
        "indexing",
        "A directory that can be listed but not searched is reported"
    )]
    #[test]
    fn a_directory_outside_git_that_can_be_listed_but_not_searched_is_reported() {
        assert_directory_reported_outside_git(0o444);
    }

    #[test]
    fn the_tracked_files_of_a_directory_that_cannot_be_read_count_as_removed() {
        assert_tracked_files_removed(0o000);
    }

    #[test]
    fn the_tracked_files_of_a_directory_that_can_be_listed_but_not_searched_count_as_removed() {
        assert_tracked_files_removed(0o444);
    }

    #[test]
    fn a_repository_without_commits_has_no_head_and_no_problem() {
        let rig = Rig::new();
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q", "-b", "main"]);
        write(dir.path(), "a.txt", "one\n");
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        assert_eq!(
            (run.index.info.head.as_deref(), run.index.info.dirty),
            (None, 1)
        );
        assert_eq!(run.index.coverage.problems, Vec::<String>::new());
    }

    #[gsttt_scenario::scenario("indexing", "A base revision adds committed changes")]
    #[test]
    fn a_base_revision_adds_committed_changes() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "one\n"), ("b.txt", "two\n")]);
        git(dir.path(), &["checkout", "-q", "-b", "feature"]);
        write(dir.path(), "a.txt", "one, committed\n");
        git(dir.path(), &["add", "a.txt"]);
        git(dir.path(), &["commit", "-q", "-m", "change a"]);
        write(dir.path(), "b.txt", "two, edited\n");
        let setup = Setup::default();
        let connectors = rig.connectors();
        let engine = Engine::new(workspace(&dir), &connectors, &[], &setup);
        assert_eq!(
            kinds(engine.changed_files(Some("main")).unwrap()),
            [
                ("a.txt".to_owned(), ChangeKind::Modified),
                ("b.txt".to_owned(), ChangeKind::Modified),
            ]
        );
    }

    #[gsttt_scenario::scenario("indexing", "An unknown base is an error")]
    #[test]
    fn an_unknown_base_is_an_error() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "one\n")]);
        let setup = Setup::default();
        let connectors = rig.connectors();
        let engine = Engine::new(workspace(&dir), &connectors, &[], &setup);
        let error = engine.changed_files(Some("no-such-branch")).unwrap_err();
        assert!(error.to_string().contains("no-such-branch"), "{error}");
    }

    /// The `(name, detect)` of each pack a connector's `open` received, for every call.
    fn received(counters: &Counters) -> Vec<Vec<(String, Vec<String>)>> {
        counters
            .opened_with()
            .into_iter()
            .map(|packs| {
                packs
                    .into_iter()
                    .map(|pack| (pack.name, pack.detect))
                    .collect()
            })
            .collect()
    }

    #[gsttt_scenario::scenario("indexing", "A user pack replaces the built-in one")]
    #[test]
    fn a_user_pack_replaces_the_built_in_one() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "")]);
        let builtin = [
            pack("express", "notes", "built-in"),
            pack("axios", "notes", "built-in"),
        ];
        let setup = Setup {
            packs: vec![pack("express", "notes", "user")],
            ..Setup::default()
        };
        let connectors = rig.connectors();
        Engine::new(workspace(&dir), &connectors, &builtin, &setup)
            .refresh()
            .unwrap();
        let [mut packs] = <[_; 1]>::try_from(received(rig.counters)).unwrap();
        packs.sort();
        assert_eq!(
            packs,
            [
                ("axios".to_owned(), vec!["built-in".to_owned()]),
                ("express".to_owned(), vec!["user".to_owned()]),
            ]
        );
    }

    #[gsttt_scenario::scenario("indexing", "A disabled pack reaches nobody")]
    #[test]
    fn a_disabled_pack_reaches_nobody() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", ""), ("q.graphql", "")]);
        let builtin = [
            pack("axios", "notes", "built-in"),
            pack("axios", "graphs", "built-in"),
            pack("fastapi", "graphs", "built-in"),
        ];
        let setup = Setup {
            disable_packs: vec!["axios".to_owned()],
            ..Setup::default()
        };
        let connectors = rig.connectors();
        Engine::new(workspace(&dir), &connectors, &builtin, &setup)
            .refresh()
            .unwrap();
        // The pack that stays proves the packs reach `open` at all.
        assert_eq!(
            received(rig.counters),
            [Vec::<(String, Vec<String>)>::new()]
        );
        assert_eq!(
            received(rig.graph_counters),
            [vec![("fastapi".to_owned(), vec!["built-in".to_owned()])]]
        );
    }

    #[gsttt_scenario::scenario("indexing", "Unclaimed files are counted by language")]
    #[test]
    fn unclaimed_files_are_counted_by_language() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", ""), ("Main.kt", "fun main() {}\n")]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        let unclaimed = &run.index.coverage.unclaimed;
        assert_eq!(unclaimed.len(), 1);
        assert_eq!(
            (unclaimed[0].extension.as_str(), unclaimed[0].files),
            ("kt", 1)
        );
        assert_eq!(unclaimed[0].language(), Some("Kotlin"));
    }

    #[gsttt_scenario::scenario("indexing", "An unresolved import is counted with its place")]
    #[test]
    fn an_unresolved_import_is_counted_with_its_place() {
        let rig = Rig::new();
        let dir = init_repo(&[("src/a.txt", "x\nx\nuse missing.txt\n")]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        let notes = run.index.coverage.connector("notes").unwrap();
        assert_eq!(notes.unresolved.len(), 1);
        let missing = &notes.unresolved[0];
        assert_eq!(missing.reason, UnresolvedReason::NotFound);
        assert_eq!(missing.count, 1);
        let file = run.index.graph.file_id("src/a.txt").unwrap();
        assert_eq!((missing.first.file, missing.first.line), (file, 3));
    }
}
