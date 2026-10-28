//! Refreshing an index: the one entry point the commands and the server call.
//!
//! A refresh lists the files, works out which facts it already has (from the snapshot of this work tree or of a
//! sibling work tree of the same repository), runs the pipeline over the rest and writes the snapshot back. When
//! nothing that shapes the index changed, it stops after opening the connectors and computing the fact keys.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use camino::Utf8Path;

use super::build::build_index;
use super::changes::{self, Change, ChangesError, Listing};
use super::files::{self, ContentId, Excluder, FileState, FilesError, Stamp};
use super::keys::{self, FactKey};
use super::packs::merge_packs;
use super::pipeline::{self, Claim, FileRun, Module, ProjectRun, Seed};
use super::snapshot::{self, Header, SnapshotError, Stored, StoredFile};
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
    Snapshot(#[from] SnapshotError),
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
        snapshot::read(self.workspace.snapshot_path().as_std_path()).map(|(_, stored)| stored.index)
    }

    /// The header of the stored snapshot, read without its body.
    pub fn peek(&self) -> Option<Header> {
        snapshot::peek(self.workspace.snapshot_path().as_std_path())
    }

    /// How the stored index stands against the tree, `None` when there is no snapshot.
    pub fn freshness(&self) -> Result<Option<Freshness>, IndexError> {
        let Some((_, stored)) = snapshot::read(self.workspace.snapshot_path().as_std_path()) else {
            return Ok(None);
        };
        let states = self.listing(Some(&stored))?;
        let before: HashMap<&RepoPath, ContentId> = stored
            .files
            .iter()
            .map(|file| (&file.path, file.stamp.id))
            .collect();
        let (mut added, mut changed, mut kept) = (0_u32, 0_u32, 0_usize);
        for state in &states {
            match before.get(&state.path) {
                None => added += 1,
                Some(id) => {
                    kept += 1;
                    if *id != state.stamp.id {
                        changed += 1;
                    }
                }
            }
        }
        let removed = to_u32(stored.files.len().saturating_sub(kept));
        Ok(Some(Freshness::since(added, removed, changed)))
    }

    /// The files that differ from HEAD, or from the best common ancestor of HEAD and `base` when one is given.
    pub fn changed_files(&self, base: Option<&str>) -> Result<Vec<Change>, IndexError> {
        let previous =
            snapshot::read(self.workspace.snapshot_path().as_std_path()).map(|(_, stored)| stored);
        let states = self.listing(previous.as_ref())?;
        self.changes_since(&states, base)
    }

    /// Brings the stored index up to date with the tree and returns it.
    pub fn refresh(&self) -> Result<Refresh, IndexError> {
        let started = Instant::now();
        let snapshot_path = self.workspace.snapshot_path();
        let previous = snapshot::read(snapshot_path.as_std_path()).map(|(_, stored)| stored);

        let mut states = self.listing(previous.as_ref())?;
        states.sort_by(|a, b| a.path.cmp(&b.path));
        let listed = started.elapsed();
        let repo = RepoView::new(
            self.workspace.root().to_owned(),
            states.iter().map(|state| state.path.clone()).collect(),
        );

        let mut modules = Vec::new();
        for &connector in self.connectors {
            let name = connector.meta().name;
            if self.setup.runs(name, || connector.detect(&repo)) {
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

        let assigned = pipeline::assign_files(&repo, &modules)?;
        let claims = assigned.claims();
        let module_states: Vec<_> = modules
            .iter()
            .map(|module| {
                keys::module_state(&module.connector.meta(), &module.options, &module.packs)
            })
            .collect();
        let mut file_keys: Vec<Option<FactKey>> = vec![None; states.len()];
        let mut unverifiable = false;
        for claim in &claims {
            let state = states.get(claim.id.index());
            let key = module_states
                .get(claim.module)
                .zip(state.filter(|state| state.stamp.id != ContentId::UNKNOWN))
                .map(|(module, state)| {
                    keys::file_key(module, claim.salt, claim.path, state.stamp.id)
                });
            unverifiable |= key.is_none();
            if let Some(slot) = file_keys.get_mut(claim.id.index()) {
                *slot = key;
            }
        }
        let config = keys::config_fingerprint(self.setup, self.connectors, self.builtin_packs);
        let head = head_commit(self.workspace.root());

        match previous {
            Some(stored)
                if !unverifiable
                    && is_current(
                        &stored,
                        config,
                        &states,
                        &file_keys,
                        &assigned.projects(),
                        head.as_deref(),
                    ) =>
            {
                Ok(Refresh {
                    reused: to_u32(claims.len()),
                    index: stored.index,
                    unchanged: true,
                    parsed: 0,
                })
            }
            own => self.rebuild(Rebuild {
                repo: &repo,
                modules: &ran,
                assigned,
                claims,
                states,
                file_keys,
                config,
                head,
                own,
                started,
                listed,
            }),
        }
    }

    /// Runs the pipeline over the files that have no facts yet and writes the snapshot.
    fn rebuild(&self, run: Rebuild<'_, '_>) -> Result<Refresh, IndexError> {
        let Rebuild {
            repo,
            modules,
            assigned,
            claims,
            states,
            file_keys,
            config,
            head,
            own,
            started,
            listed,
        } = run;
        let max = self.setup.max_file_bytes;
        let seeds = self.seeds(&claims, &states, &file_keys, own.as_ref());

        let (output, phases) = pipeline::execute(assigned, repo, max, seeds);
        let reused = output.files.iter().filter(|file| file.reused).count();
        let parsed = output.files.len().saturating_sub(reused);
        let dirty = self
            .changes_since(&states, None)
            .map_or(0, |changes| to_u32(changes.len()));
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
            files: stored_files(states, file_keys, output.files),
        };
        snapshot::write(
            self.workspace.snapshot_path().as_std_path(),
            &header,
            &stored,
        )?;
        let Stored { index, .. } = stored;
        Ok(Refresh {
            index,
            unchanged: false,
            parsed: to_u32(parsed),
            reused: to_u32(reused),
        })
    }

    /// The facts a snapshot already holds for each claimed file, from this work tree's snapshot first and then from
    /// the snapshots of sibling work trees. A file over the size limit never gets a seed: it is rejected before any
    /// cache is asked.
    fn seeds(
        &self,
        claims: &[Claim<'_>],
        states: &[FileState],
        file_keys: &[Option<FactKey>],
        own: Option<&Stored>,
    ) -> Vec<Option<Seed>> {
        let max = self.setup.max_file_bytes;
        let wanted_key = |claim: &Claim<'_>| -> Option<FactKey> {
            let state = states.get(claim.id.index())?;
            if state.stamp.size > max {
                return None;
            }
            file_keys.get(claim.id.index()).copied().flatten()
        };
        let own: HashMap<FactKey, &StoredFile> = own
            .into_iter()
            .flat_map(|stored| stored.files.iter())
            .filter(|file| snapshot::is_reusable(&file.facts))
            .filter_map(|file| Some((file.key?, file)))
            .collect();
        let mut missing: HashSet<FactKey> = claims
            .iter()
            .filter_map(wanted_key)
            .filter(|key| !own.contains_key(key))
            .collect();
        let mut siblings: HashMap<FactKey, StoredFile> = HashMap::new();
        for path in self.workspace.sibling_snapshots() {
            if missing.is_empty() {
                break;
            }
            for (key, file) in snapshot::reusable_facts(path.as_std_path(), &missing) {
                missing.remove(&key);
                siblings.insert(key, file);
            }
        }
        claims
            .iter()
            .map(|claim| {
                let key = wanted_key(claim)?;
                let stored = own.get(&key).copied().or_else(|| siblings.get(&key))?;
                Some(Seed {
                    facts: stored.facts.clone(),
                    extra: stored.extra.clone(),
                })
            })
            .collect()
    }

    /// The files of the work tree, with the stamps of the previous snapshot to spare hashing what did not change.
    fn listing(&self, previous: Option<&Stored>) -> Result<Vec<FileState>, FilesError> {
        let stamps: HashMap<&RepoPath, Stamp> = previous
            .into_iter()
            .flat_map(|stored| stored.files.iter())
            .map(|file| (&file.path, file.stamp))
            .collect();
        files::scan(&self.workspace, &self.setup.exclude, &|path| {
            stamps.get(path).copied()
        })
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

/// What a rebuild needs from the steps before it.
struct Rebuild<'r, 'a> {
    repo: &'a RepoView,
    modules: &'r [&'r str],
    assigned: pipeline::Assigned<'a>,
    claims: Vec<Claim<'a>>,
    states: Vec<FileState>,
    file_keys: Vec<Option<FactKey>>,
    config: u128,
    head: Option<String>,
    own: Option<Stored>,
    started: Instant,
    listed: Duration,
}

/// Whether a stored index is what a run over this tree would build again: same configuration, same commit, same
/// projects, and for every file the same content and the same fact key. The content is compared for every file,
/// not only for those a project took, because the dirty count and the freshness of the snapshot depend on all of
/// them.
fn is_current(
    stored: &Stored,
    config: u128,
    states: &[FileState],
    file_keys: &[Option<FactKey>],
    projects: &[ProjectRun],
    head: Option<&str>,
) -> bool {
    stored.config == config
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
                old.path == new.path
                    && old.stamp.id == new.stamp.id
                    && old.key == *key
                    // A failed read says nothing about the content, so it is tried again.
                    && !matches!(
                        old.facts.status,
                        ParseStatus::Failed(ParseFailure::Unreadable { .. })
                    )
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

/// The commit HEAD points at, `None` outside git and before the first commit.
fn head_commit(root: &Utf8Path) -> Option<String> {
    let repo = gix::open(root.as_std_path()).ok()?;
    let id = repo.head_id().ok()?;
    Some(id.to_string())
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

    use camino::Utf8PathBuf;
    use serde_json::json;

    use super::*;
    use crate::connector::Erased;
    use crate::contract::ContractKey;
    use crate::engine::changes::ChangeKind;
    use crate::engine::testing::{Counters, Graphs, Notes, git, init_repo, write};
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

    fn kinds(changes: Vec<Change>) -> Vec<(String, ChangeKind)> {
        changes
            .into_iter()
            .map(|change| (change.path.as_str().to_owned(), change.kind))
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

    #[gsttt_scenario::scenario("indexing", "Only edited files are parsed")]
    #[test]
    fn only_edited_files_are_parsed() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", ""), ("b.txt", ""), ("c.txt", "")]);
        let setup = Setup::default();
        index(&rig, &dir, &setup).unwrap();
        rig.counters.take();
        write(dir.path(), "b.txt", "use a.txt\n");
        let run = index(&rig, &dir, &setup).unwrap();
        assert_eq!((run.parsed, run.reused), (1, 2));
        assert_eq!(rig.counters.take()[0], 1);
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
        let holder = tempfile::tempdir().unwrap();
        let linked = holder.path().join("feature");
        git(
            dir.path(),
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feature",
                linked.to_str().unwrap(),
            ],
        );
        let root = Utf8PathBuf::from_path_buf(linked).unwrap();
        let connectors = rig.connectors();
        let engine = Engine::new(
            Workspace::locate_with(&root, None).unwrap(),
            &connectors,
            &[],
            &setup,
        );
        let run = engine.refresh().unwrap();
        assert_eq!((run.parsed, run.reused), (0, 2));
        assert_eq!(rig.counters.take()[0], 0);
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
        let dir = init_repo(&[("a.txt", "use b.txt\nuse linkboom\n"), ("b.txt", "")]);
        let run = index(&rig, &dir, &Setup::default()).unwrap();
        let graph = &run.index.graph;
        assert_eq!(graph.file_count(), 2);
        assert_eq!(graph.contract_count(), 0);
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

    #[gsttt_scenario::scenario("indexing", "Bad options fail the run")]
    #[test]
    fn bad_options_fail_the_run() {
        let rig = Rig::new();
        let dir = init_repo(&[("a.txt", "")]);
        let mut setup = Setup::default();
        setup.modules.push(ModuleSetup {
            module: "notes".to_owned(),
            options: json!({"nope": 5}),
        });
        let error = index(&rig, &dir, &setup).unwrap_err().to_string();
        assert!(error.contains("notes") && error.contains("nope"), "{error}");
    }

    #[gsttt_scenario::scenario("indexing", "One thread and eight agree")]
    #[test]
    fn one_thread_and_eight_agree() {
        let files = [
            ("a.txt", "use b.txt\nembed graphql query Q\n"),
            ("b.txt", ""),
            ("pkg/ROOT", ""),
            ("pkg/c.txt", "use a.txt\nuse missing.txt\n"),
            ("q.graphql", "type Query\n"),
        ];
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
        assert_eq!(built(1), built(8));
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
        let header_len =
            usize::try_from(u32::from_le_bytes(bytes[12..16].try_into().unwrap())).unwrap();
        bytes.truncate(16 + header_len);
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

    #[gsttt_scenario::scenario("indexing", "A user pack replaces the built-in one")]
    #[test]
    fn a_user_pack_replaces_the_built_in_one() {
        let builtin = [
            pack("express", "ts", "built-in"),
            pack("axios", "ts", "built-in"),
        ];
        let setup = Setup {
            packs: vec![pack("express", "ts", "user")],
            ..Setup::default()
        };
        let merged = merge_packs(&builtin, &setup, "ts");
        let express: Vec<_> = merged
            .iter()
            .filter(|pack| pack.name == "express")
            .collect();
        assert_eq!(express.len(), 1);
        assert_eq!(express[0].detect, ["user"]);
        assert!(merged.iter().any(|pack| pack.name == "axios"));
    }

    #[gsttt_scenario::scenario("indexing", "A disabled pack reaches nobody")]
    #[test]
    fn a_disabled_pack_reaches_nobody() {
        let builtin = [
            pack("axios", "ts", "built-in"),
            pack("axios", "python", "built-in"),
            pack("fastapi", "python", "built-in"),
        ];
        let setup = Setup {
            disable_packs: vec!["axios".to_owned()],
            ..Setup::default()
        };
        for language in ["ts", "python"] {
            let merged = merge_packs(&builtin, &setup, language);
            assert!(merged.iter().all(|pack| pack.name != "axios"));
        }
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
