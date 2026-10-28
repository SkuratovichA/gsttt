//! Fixtures shared by the engine's tests: throw-away git repositories, a small tree to run the pipeline over and
//! connectors that count their calls.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::pipeline::Module;
use crate::connector::{
    Connector, ConnectorError, DynConnector, Erased, LinkInput, LinkedFact, Meta, Parsed,
    PrepareFile, Project, Provide, SourceFile,
};
use crate::contract::{ContractKey, Role};
use crate::fact::{
    ContractFact, Embedded, FileFacts, Import, ImportKind, Resolution, UnresolvedReason,
};
use crate::path::RepoPath;
use crate::repo::RepoView;
use crate::rules::Pack;

/// Runs `git` in `dir` with a fixed identity and no filters, whatever the command's outcome.
fn run_git(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
        .args(args)
        .output()
        .unwrap()
}

/// Runs `git` in `dir` and returns what it printed. A failing command fails the test.
pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    let output = run_git(dir, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Runs the test `name` in a child process and returns what it printed. The test has to be `#[ignore]`d, so that a
/// run of the suite skips it. The child's environment is the parent's with `set` added and `remove` taken out: the
/// environment of a running process cannot be changed from safe code, and tests share one process under
/// `cargo test`.
pub(crate) fn in_environment(name: &str, set: &[(&str, &OsStr)], remove: &[&str]) -> String {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args([
        "--ignored",
        "--exact",
        name,
        "--nocapture",
        "--test-threads=1",
    ]);
    for key in remove {
        command.env_remove(key);
    }
    for (key, value) in set {
        command.env(key, value);
    }
    let output = command.output().unwrap();
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success() && printed.contains("1 passed"),
        "{name} did not pass in a child process: {printed}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    printed
}

/// Merges `branch` into the checked-out branch, which has to stop on a conflict and leave the index conflicted.
pub(crate) fn merge_with_conflict(dir: &Path, branch: &str) {
    let output = run_git(dir, &["merge", "-q", branch]);
    assert!(
        !output.status.success(),
        "merging {branch} was meant to conflict"
    );
}

/// Sets the modification time of `path` to `time`.
pub(crate) fn touch(path: &Path, time: SystemTime) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(time).unwrap();
}

/// Writes `text` to `path` under `dir`, creating the directories above it.
pub(crate) fn write(dir: &Path, path: &str, text: &str) {
    let target = dir.join(path);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(target, text).unwrap();
}

/// A repository on the branch `main` with `files` committed.
pub(crate) fn init_repo(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    for (path, text) in files {
        write(dir.path(), path, text);
    }
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "initial"]);
    dir
}

/// A linked work tree of the repository in `dir`, on a new branch `name`. The directory it is in goes when the
/// first value does.
pub(crate) fn add_worktree(dir: &Path, name: &str) -> (tempfile::TempDir, Utf8PathBuf) {
    let holder = tempfile::tempdir().unwrap();
    let linked = holder.path().join(name);
    git(
        dir,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            name,
            linked.to_str().unwrap(),
        ],
    );
    (holder, Utf8PathBuf::from_path_buf(linked).unwrap())
}

/// How often the calls of a [`Notes`] connector ran.
#[derive(Debug)]
pub(crate) struct Counters {
    parse: AtomicUsize,
    prepare: AtomicUsize,
    resolve: AtomicUsize,
    link: AtomicUsize,
    /// How often a project was asked for its root.
    roots: AtomicUsize,
    /// The most imports any `prepare` found already resolved.
    resolved_before_prepare: AtomicUsize,
    /// The project root and the file paths of every `prepare` call.
    prepared: Mutex<Vec<(String, Vec<String>)>>,
    /// A file, as `(path, text)`, that the next `open` saves again, or removes when there is no text.
    rewrite_on_open: Mutex<Option<(String, Option<String>)>>,
    /// The packs each `open` received, one list per call.
    opened_with: Mutex<Vec<Vec<Pack>>>,
}

impl Counters {
    pub(crate) const fn new() -> Self {
        Self {
            parse: AtomicUsize::new(0),
            prepare: AtomicUsize::new(0),
            resolve: AtomicUsize::new(0),
            link: AtomicUsize::new(0),
            roots: AtomicUsize::new(0),
            resolved_before_prepare: AtomicUsize::new(0),
            prepared: Mutex::new(Vec::new()),
            rewrite_on_open: Mutex::new(None),
            opened_with: Mutex::new(Vec::new()),
        }
    }

    /// A counter set that lives as long as the program, for a connector that has to be `'static`.
    pub(crate) fn leak() -> &'static Self {
        Box::leak(Box::new(Self::new()))
    }

    /// The counts of `[parse, prepare, resolve, link]`, which start again from zero.
    pub(crate) fn take(&self) -> [usize; 4] {
        [&self.parse, &self.prepare, &self.resolve, &self.link]
            .map(|count| count.swap(0, Ordering::Relaxed))
    }

    /// How often projects were asked for their root, which starts again from zero.
    pub(crate) fn take_roots(&self) -> usize {
        self.roots.swap(0, Ordering::Relaxed)
    }

    /// Makes the next `open` of a [`Notes`] connector save `text` to `path`: an editor saving after a run listed
    /// the files and before it reads them.
    pub(crate) fn rewrite_on_next_open(&self, path: &str, text: &str) {
        *self.rewrite_on_open.lock().unwrap() = Some((path.to_owned(), Some(text.to_owned())));
    }

    /// Makes the next `open` of a [`Notes`] connector remove `path`: a file that is gone by the time it is read.
    pub(crate) fn remove_on_next_open(&self, path: &str) {
        *self.rewrite_on_open.lock().unwrap() = Some((path.to_owned(), None));
    }

    /// The packs each `open` received, in the order of the calls.
    pub(crate) fn opened_with(&self) -> Vec<Vec<Pack>> {
        self.opened_with.lock().unwrap().clone()
    }

    /// The `prepare` calls as `(project root, file paths)`, sorted.
    pub(crate) fn prepared(&self) -> Vec<(String, Vec<String>)> {
        let mut calls = self.prepared.lock().unwrap().clone();
        calls.sort();
        calls
    }

    pub(crate) fn resolved_before_prepare(&self) -> usize {
        self.resolved_before_prepare.load(Ordering::Relaxed)
    }
}

impl Default for Counters {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Options {
    #[serde(default)]
    pub(crate) tag: String,
}

/// Reads `.txt` files: `use <path>` imports a file, `use boom` panics in `resolve`, `use linkboom` makes `link`
/// panic, `embed <lang> <text>` carries a document, and `boom` panics in `parse`. A file called `explode.txt` makes
/// `matches` panic, and a file called `DETECT_BOOM` anywhere makes `detect` panic. A project sits at the root and in
/// every directory with a `ROOT` file; `link` reports a site for every file with a resolved import. `open` saves
/// the file [`Counters::rewrite_on_next_open`] names, or removes the one [`Counters::remove_on_next_open`] names,
/// and keeps the packs it received for [`Counters::opened_with`].
pub(crate) struct Notes {
    pub(crate) counters: &'static Counters,
}

pub(crate) struct NotesProject {
    repo: RepoView,
    root: String,
    counters: &'static Counters,
}

impl Connector for Notes {
    type Options = Options;
    type Project = NotesProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "notes",
            version: 1,
            summary: "test",
            extensions: &["txt"],
        }
    }

    fn detect(&self, repo: &RepoView) -> bool {
        assert!(repo.lookup("DETECT_BOOM").is_none(), "boom in detect");
        true
    }

    fn open(
        &self,
        repo: &RepoView,
        _: &Options,
        packs: &[Pack],
    ) -> Result<Vec<NotesProject>, ConnectorError> {
        self.counters
            .opened_with
            .lock()
            .unwrap()
            .push(packs.to_vec());
        let rewrite = self.counters.rewrite_on_open.lock().unwrap().take();
        if let Some((path, text)) = rewrite {
            match text {
                Some(text) => std::fs::write(repo.absolute(&path), text).unwrap(),
                None => std::fs::remove_file(repo.absolute(&path)).unwrap(),
            }
        }
        let roots = repo
            .files()
            .filter(|(_, path)| path.file_name() == "ROOT")
            .map(|(_, path)| path.parent().to_owned());
        Ok(std::iter::once(String::new())
            .chain(roots)
            .map(|root| NotesProject {
                repo: repo.clone(),
                root,
                counters: self.counters,
            })
            .collect())
    }
}

impl Project for NotesProject {
    type Extra = ();

    fn root(&self) -> &str {
        self.counters.roots.fetch_add(1, Ordering::Relaxed);
        &self.root
    }

    fn matches(&self, path: &RepoPath) -> bool {
        assert!(path.file_name() != "explode.txt", "boom in matches");
        path.extension() == Some("txt")
    }

    fn salt(&self) -> u64 {
        0
    }

    fn provides(&self) -> Vec<Provide> {
        Vec::new()
    }

    fn parse(&self, file: &SourceFile<'_>) -> Parsed<()> {
        self.counters.parse.fetch_add(1, Ordering::Relaxed);
        let mut facts = FileFacts::default();
        for (index, line) in file.text.lines().enumerate() {
            let number = u32::try_from(index + 1).unwrap();
            if line == "boom" {
                panic!("boom in parse");
            } else if let Some(spec) = line.strip_prefix("use ") {
                facts
                    .imports
                    .push(Import::new(spec, ImportKind::Static, number));
            } else if let Some((lang, text)) = line
                .strip_prefix("embed ")
                .and_then(|rest| rest.split_once(' '))
            {
                facts.embedded.push(Embedded {
                    lang: lang.to_owned(),
                    line: number,
                    text: text.to_owned(),
                    via: "notes".to_owned(),
                });
            }
        }
        Parsed::new(facts)
    }

    fn prepare(&mut self, files: &[PrepareFile<'_>]) {
        self.counters.prepare.fetch_add(1, Ordering::Relaxed);
        self.counters.resolved_before_prepare.fetch_max(
            self.counters.resolve.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        let paths = files
            .iter()
            .map(|file| file.path.as_str().to_owned())
            .collect();
        self.counters
            .prepared
            .lock()
            .unwrap()
            .push((self.root.clone(), paths));
    }

    fn resolve(&self, _: &RepoPath, import: &Import) -> Resolution {
        self.counters.resolve.fetch_add(1, Ordering::Relaxed);
        assert!(import.spec != "boom", "boom in resolve");
        match self.repo.lookup(&import.spec) {
            Some(file) => Resolution::File { file },
            None => Resolution::Unresolved(UnresolvedReason::NotFound),
        }
    }

    fn link(&self, input: &LinkInput<'_, ()>) -> Vec<LinkedFact> {
        self.counters.link.fetch_add(1, Ordering::Relaxed);
        assert!(
            !input.files().iter().any(|file| file
                .facts
                .imports
                .iter()
                .any(|import| import.spec == "linkboom")),
            "boom in link"
        );
        input
            .files()
            .iter()
            .filter(|file| {
                file.imports
                    .iter()
                    .any(|import| !import.to.files().is_empty())
            })
            .map(|file| LinkedFact {
                file: file.id,
                fact: fact("queue linked"),
            })
            .collect()
    }
}

/// Reads `.graphql` files as nothing and embedded GraphQL as one site per document. A document that starts with
/// `@N ` has its fact on line N of the document instead of line 1. `open` keeps the packs it received for
/// [`Counters::opened_with`].
pub(crate) struct Graphs {
    pub(crate) counters: &'static Counters,
    /// The connector's name, so two of them can compete for the same files.
    pub(crate) name: &'static str,
}

pub(crate) struct GraphsProject {
    counters: &'static Counters,
}

impl Connector for Graphs {
    type Options = Options;
    type Project = GraphsProject;

    fn meta(&self) -> Meta {
        Meta {
            name: self.name,
            version: 1,
            summary: "test",
            extensions: &["graphql"],
        }
    }

    fn detect(&self, _: &RepoView) -> bool {
        true
    }

    fn open(
        &self,
        _: &RepoView,
        _: &Options,
        packs: &[Pack],
    ) -> Result<Vec<GraphsProject>, ConnectorError> {
        self.counters
            .opened_with
            .lock()
            .unwrap()
            .push(packs.to_vec());
        Ok(vec![GraphsProject {
            counters: self.counters,
        }])
    }
}

impl Project for GraphsProject {
    type Extra = ();

    fn root(&self) -> &'static str {
        ""
    }

    fn matches(&self, path: &RepoPath) -> bool {
        path.extension() == Some("graphql")
    }

    fn salt(&self) -> u64 {
        0
    }

    fn provides(&self) -> Vec<Provide> {
        Vec::new()
    }

    fn parse(&self, _: &SourceFile<'_>) -> Parsed<()> {
        self.counters.parse.fetch_add(1, Ordering::Relaxed);
        Parsed::default()
    }

    fn resolve(&self, _: &RepoPath, _: &Import) -> Resolution {
        Resolution::Builtin
    }

    fn embeds(&self) -> &[&str] {
        &["graphql"]
    }

    fn parse_embedded(&self, document: &Embedded) -> Vec<ContractFact> {
        let (line, text) = document
            .text
            .strip_prefix('@')
            .and_then(|rest| rest.split_once(' '))
            .and_then(|(line, text)| Some((line.parse().ok()?, text)))
            .unwrap_or((1, document.text.as_str()));
        vec![ContractFact {
            line,
            ..fact(&format!("queue {}", text.replace(' ', "-")))
        }]
    }
}

/// Never opens.
pub(crate) struct Broken;

impl Connector for Broken {
    type Options = Options;
    type Project = GraphsProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "broken",
            version: 1,
            summary: "test",
            extensions: &[],
        }
    }

    fn detect(&self, _: &RepoView) -> bool {
        true
    }

    fn open(
        &self,
        _: &RepoView,
        _: &Options,
        _: &[Pack],
    ) -> Result<Vec<GraphsProject>, ConnectorError> {
        Err(ConnectorError::Project {
            connector: "broken".to_owned(),
            root: "x".to_owned(),
            message: "unreadable manifest".to_owned(),
        })
    }
}

static NO_COUNTERS: Counters = Counters::new();
pub(crate) static NOTES: Erased<Notes> = Erased(Notes {
    counters: &NO_COUNTERS,
});
pub(crate) static GRAPHS: Erased<Graphs> = Erased(Graphs {
    counters: &NO_COUNTERS,
    name: "graphs",
});
pub(crate) static BROKEN: Erased<Broken> = Erased(Broken);

pub(crate) const LIMIT: u64 = 1024;

pub(crate) fn fact(key: &str) -> ContractFact {
    ContractFact {
        key: ContractKey::parse(key).unwrap(),
        role: Role::Calls,
        line: 1,
        via: "test".to_owned(),
    }
}

pub(crate) fn modules() -> Vec<Module<'static>> {
    [&NOTES as &dyn DynConnector, &GRAPHS, &BROKEN]
        .into_iter()
        .map(|connector| Module {
            connector,
            options: Value::Null,
            packs: Vec::new(),
        })
        .collect()
}

/// A directory holding `files`, and the view of it that a run over a fixture tree uses.
pub(crate) fn tree(files: &[(&str, &[u8])]) -> (tempfile::TempDir, RepoView) {
    let dir = tempfile::tempdir().unwrap();
    for (path, bytes) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let repo = RepoView::scan(root).unwrap();
    (dir, repo)
}

pub(crate) fn fixture() -> (tempfile::TempDir, RepoView) {
    let big = "x".repeat(2000);
    tree(&[
        (
            "a.txt",
            b"use b.txt\nuse missing.txt\nuse boom\nembed graphql query Q\nembed sql select 1\n",
        ),
        ("b.txt", b""),
        ("bad.txt", b"boom\n"),
        ("big.txt", big.as_bytes()),
        ("latin.txt", b"caf\xe9\n"),
        ("q.graphql", b"type Query { a: Int }\n"),
        ("README.md", b"# fixture\n"),
        ("pkg/ROOT", b""),
        ("pkg/c.txt", b"use a.txt\n"),
    ])
}
