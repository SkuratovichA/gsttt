//! The middle of a run: open the connectors, give every file to a project, parse, prepare, resolve, read the
//! embedded documents and link.
//!
//! It reads files through a [`RepoView`], so a run over a directory of fixtures is the same code as a run over a
//! repository. The engine puts the file listing, the content ids, the fact cache, the graph and the snapshot
//! around it; a connector's tests call [`run`] on a fixture tree and snapshot [`Output::to_json`].

use std::cmp::Reverse;
use std::fmt;
use std::io;
use std::panic::{self, AssertUnwindSafe};

use rayon::prelude::*;
use serde::Serialize;
use serde_json::{Map, Value};

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
}

/// An embedded document after the embedded phase.
#[derive(Debug)]
pub struct EmbeddedRun {
    /// The connector that read it; `None` when no project reads its language, which coverage reports as a blind
    /// spot.
    pub read_by: Option<&'static str>,
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
    Open,
    Prepare,
    Resolve,
    ParseEmbedded,
    Link,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let call = match self.call {
            Call::Open => "open",
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

/// Runs the middle of a run over every file of `repo`, with `modules` in registry order. Only a bad option stops
/// it; anything else a connector gets wrong, a panic included, ends up in the output.
pub fn run(
    repo: &RepoView,
    modules: &[Module<'_>],
    max_file_bytes: u64,
) -> Result<Output, ConnectorError> {
    let mut problems = Vec::new();
    let Opened {
        mut projects,
        module_of,
    } = open(repo, modules, &mut problems)?;

    let mut unclaimed = Vec::new();
    let mut owned = Vec::new();
    for (id, path) in repo.files() {
        match assign(&projects, &module_of, path) {
            Some(project) => owned.push(Owned { id, path, project }),
            None => unclaimed.push(id),
        }
    }
    let mut members = vec![Vec::new(); projects.len()];
    for (index, file) in owned.iter().enumerate() {
        if let Some(list) = members.get_mut(file.project) {
            list.push(index);
        }
    }

    let parsed: Vec<(FileFacts, Vec<u8>)> = owned
        .par_iter()
        .map(|file| match projects.get(file.project) {
            Some(project) => parse(repo, project.as_ref(), file, max_file_bytes),
            None => (FileFacts::default(), Vec::new()),
        })
        .collect();

    let prepared: Vec<Option<Problem>> = projects
        .par_iter_mut()
        .zip(members.par_iter())
        .map(|(project, members)| {
            let files: Vec<PrepareFile<'_>> = members
                .iter()
                .filter_map(|&index| {
                    let file = owned.get(index)?;
                    let (facts, _) = parsed.get(index)?;
                    Some(PrepareFile {
                        id: file.id,
                        path: file.path,
                        facts,
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
        .map(|(file, (facts, _))| match projects.get(file.project) {
            Some(project) => resolve(project.as_ref(), file, facts),
            None => (Vec::new(), Vec::new()),
        })
        .collect();

    let embedded: Vec<(Vec<EmbeddedRun>, Vec<Problem>)> = parsed
        .par_iter()
        .map(|(facts, _)| read_embedded(&projects, facts))
        .collect();

    let linked: Vec<(Vec<LinkedFact>, Option<Problem>)> = projects
        .par_iter()
        .zip(members.par_iter())
        .map(|(project, members)| {
            let files = members
                .iter()
                .filter_map(|&index| {
                    let file = owned.get(index)?;
                    let (facts, extra) = parsed.get(index)?;
                    let (imports, _) = resolved.get(index)?;
                    Some(LinkFile {
                        id: file.id,
                        path: file.path,
                        facts,
                        imports: imports.as_slice(),
                        extra: extra.as_slice(),
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

    let mut files = Vec::with_capacity(owned.len());
    for (((file, (facts, extra)), (imports, resolve_problems)), (embedded, embedded_problems)) in
        owned.iter().zip(parsed).zip(resolved).zip(embedded)
    {
        problems.extend(resolve_problems);
        problems.extend(embedded_problems);
        files.push(FileRun {
            id: file.id,
            project: ProjectId::from_index(file.project),
            facts,
            extra,
            imports,
            embedded,
        });
    }
    let mut linked_facts = Vec::new();
    for (facts, link_problem) in linked {
        linked_facts.extend(facts);
        problems.extend(link_problem);
    }
    linked_facts.sort_by_key(|fact| fact.file);

    Ok(Output {
        projects: projects
            .iter()
            .map(|project| ProjectRun {
                connector: project.connector(),
                root: project.root().to_owned(),
                salt: project.salt(),
                provides: project.provides(),
            })
            .collect(),
        files,
        unclaimed,
        linked: linked_facts,
        problems,
    })
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

/// The project that owns `path`: of the first module with a project that takes it, the project with the deepest
/// root.
fn assign(projects: &[Box<dyn DynProject>], module_of: &[usize], path: &RepoPath) -> Option<usize> {
    projects
        .iter()
        .zip(module_of)
        .enumerate()
        .filter(|(_, (project, _))| path.is_under(project.root()) && project.matches(path))
        .min_by_key(|(index, (project, module))| (**module, Reverse(depth(project.root())), *index))
        .map(|(index, _)| index)
}

fn depth(root: &str) -> usize {
    if root.is_empty() {
        0
    } else {
        root.split('/').count()
    }
}

/// Reads a file and hands it to its project. A file that cannot be read, is past the size limit or is not UTF-8,
/// and a parse that panics, come back as failed facts.
fn parse(
    repo: &RepoView,
    project: &dyn DynProject,
    file: &Owned<'_>,
    max_file_bytes: u64,
) -> (FileFacts, Vec<u8>) {
    let text = match read(repo, file.path, max_file_bytes) {
        Ok(text) => text,
        Err(failure) => return (FileFacts::failed(failure), Vec::new()),
    };
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

fn read(repo: &RepoView, path: &RepoPath, max_file_bytes: u64) -> Result<String, ParseFailure> {
    let unreadable = |error: io::Error| ParseFailure::Unreadable {
        message: error.to_string(),
    };
    let bytes = repo
        .absolute(path.as_str())
        .metadata()
        .map_err(unreadable)?
        .len();
    if bytes > max_file_bytes {
        return Err(ParseFailure::TooLarge {
            bytes,
            limit: max_file_bytes,
        });
    }
    repo.read_to_string(path.as_str()).map_err(|error| {
        if error.kind() == io::ErrorKind::InvalidData {
            ParseFailure::NotUtf8
        } else {
            unreadable(error)
        }
    })
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

/// Runs a connector call and turns a panic into its message, so one bad file or project never ends a run.
fn contain<T>(call: impl FnOnce() -> T) -> Result<T, String> {
    panic::catch_unwind(AssertUnwindSafe(call)).map_err(|payload| {
        payload
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic without a message".to_owned())
    })
}

fn display_root(root: &str) -> &str {
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
    use camino::Utf8PathBuf;
    use schemars::JsonSchema;
    use serde::Deserialize;
    use serde_json::json;

    use super::*;
    use crate::connector::{Connector, Erased, Parsed, Project};
    use crate::contract::{ContractKey, Role};
    use crate::fact::{Embedded, Import, ImportKind, ParseStatus};

    #[derive(Default, Serialize, Deserialize, JsonSchema)]
    struct Options {
        #[serde(default)]
        tag: String,
    }

    /// Reads `.txt` files: `use <path>` imports a file, `use boom` panics in `resolve`, `embed <lang> <text>`
    /// carries a document, and `boom` panics in `parse`. A project sits at the root and in every directory with a
    /// `ROOT` file; `link` reports a site for every file with a resolved import.
    struct Notes;

    struct NotesProject {
        repo: RepoView,
        root: String,
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

        fn detect(&self, _: &RepoView) -> bool {
            true
        }

        fn open(
            &self,
            repo: &RepoView,
            _: &Options,
            _: &[Pack],
        ) -> Result<Vec<NotesProject>, ConnectorError> {
            let roots = repo
                .files()
                .filter(|(_, path)| path.file_name() == "ROOT")
                .map(|(_, path)| path.parent().to_owned());
            Ok(std::iter::once(String::new())
                .chain(roots)
                .map(|root| NotesProject {
                    repo: repo.clone(),
                    root,
                })
                .collect())
        }
    }

    impl Project for NotesProject {
        type Extra = ();

        fn root(&self) -> &str {
            &self.root
        }

        fn matches(&self, path: &RepoPath) -> bool {
            path.extension() == Some("txt")
        }

        fn salt(&self) -> u64 {
            0
        }

        fn provides(&self) -> Vec<Provide> {
            Vec::new()
        }

        fn parse(&self, file: &SourceFile<'_>) -> Parsed<()> {
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

        fn resolve(&self, _: &RepoPath, import: &Import) -> Resolution {
            assert!(import.spec != "boom", "boom in resolve");
            match self.repo.lookup(&import.spec) {
                Some(file) => Resolution::File { file },
                None => Resolution::Unresolved(UnresolvedReason::NotFound),
            }
        }

        fn link(&self, input: &LinkInput<'_, ()>) -> Vec<LinkedFact> {
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

    /// Reads `.graphql` files as nothing and embedded GraphQL as one site per document.
    struct Graphs;

    struct GraphsProject;

    impl Connector for Graphs {
        type Options = Options;
        type Project = GraphsProject;

        fn meta(&self) -> Meta {
            Meta {
                name: "graphs",
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
            _: &[Pack],
        ) -> Result<Vec<GraphsProject>, ConnectorError> {
            Ok(vec![GraphsProject])
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
            Parsed::default()
        }

        fn resolve(&self, _: &RepoPath, _: &Import) -> Resolution {
            Resolution::Builtin
        }

        fn embeds(&self) -> &[&str] {
            &["graphql"]
        }

        fn parse_embedded(&self, document: &Embedded) -> Vec<ContractFact> {
            vec![fact(&format!("queue {}", document.text.replace(' ', "-")))]
        }
    }

    /// Never opens.
    struct Broken;

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

    static NOTES: Erased<Notes> = Erased(Notes);
    static GRAPHS: Erased<Graphs> = Erased(Graphs);
    static BROKEN: Erased<Broken> = Erased(Broken);

    const LIMIT: u64 = 1024;

    fn fact(key: &str) -> ContractFact {
        ContractFact {
            key: ContractKey::parse(key).unwrap(),
            role: Role::Calls,
            line: 1,
            via: "test".to_owned(),
        }
    }

    fn modules() -> Vec<Module<'static>> {
        [&NOTES as &dyn DynConnector, &GRAPHS, &BROKEN]
            .into_iter()
            .map(|connector| Module {
                connector,
                options: Value::Null,
                packs: Vec::new(),
            })
            .collect()
    }

    fn fixture() -> (tempfile::TempDir, RepoView) {
        let dir = tempfile::tempdir().unwrap();
        let big = "x".repeat(2000);
        let files: [(&str, &[u8]); 9] = [
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
        ];
        for (path, bytes) in files {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let repo = RepoView::scan(root).unwrap();
        (dir, repo)
    }

    fn file<'a>(output: &'a Output, repo: &RepoView, path: &str) -> &'a FileRun {
        output
            .files
            .iter()
            .find(|file| repo.path(file.id).unwrap().as_str() == path)
            .unwrap()
    }

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
        assert_eq!(a.embedded.first().unwrap().facts, [fact("queue query-Q")]);
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
    fn a_bad_option_stops_the_run() {
        let (_dir, repo) = fixture();
        let modules = [Module {
            connector: &NOTES,
            options: json!({"tag": 5}),
            packs: Vec::new(),
        }];
        assert!(matches!(
            run(&repo, &modules, LIMIT),
            Err(ConnectorError::Options { .. })
        ));
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
