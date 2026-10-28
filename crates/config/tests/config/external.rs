//! External connectors against scripted fake processes, and the schema of their protocol.
//!
//! A fake process is a shell script in the repository root. It appends every line it reads to `requests.log`, answers by the first `case` pattern the line matches, and touches `started` when it starts, so a test sees what gsttt sent, in what order, from which directory, and whether the command ran at all.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use gsttt_config::{External, protocol_schema};
use gsttt_core::connector::{
    Connector, ConnectorError, DynConnector, Erased, Meta, Parsed, Project, Provide, SourceFile,
};
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::engine::pipeline::{Module, Output, run};
use gsttt_core::fact::{FileFacts, Import, ImportKind, ParseFailure, ParseStatus, Resolution};
use gsttt_core::rules::Pack;
use gsttt_core::setup::ExternalSetup;
use gsttt_core::{RepoPath, RepoView};
use serde_json::{Value, json};

use crate::fakes::NoOptions;
use crate::tree::Tree;

/// The size limit of a run, far above any file here.
const LIMIT: u64 = 1 << 20;

const OPEN_LINE: &str = r#"{"type":"open","protocol":1,"root":"","options":{}}"#;
const DONE_LINE: &str = r#"{"type":"done"}"#;

/// A connector that reads text files, one import per line, so that a test sees the other connectors go on.
struct Plain;

struct PlainProject;

impl Connector for Plain {
    type Options = NoOptions;
    type Project = PlainProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "plain",
            version: 1,
            summary: "text files",
            extensions: &["txt"],
        }
    }

    fn detect(&self, repo: &RepoView) -> bool {
        repo.files()
            .any(|(_, path)| path.extension() == Some("txt"))
    }

    fn open(
        &self,
        _: &RepoView,
        _: &NoOptions,
        _: &[Pack],
    ) -> Result<Vec<PlainProject>, ConnectorError> {
        Ok(vec![PlainProject])
    }
}

impl Project for PlainProject {
    type Extra = ();

    fn root(&self) -> &'static str {
        ""
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
        let imports = file
            .text
            .lines()
            .map(|line| Import::new(line, ImportKind::Static, 1))
            .collect();
        Parsed::new(FileFacts {
            imports,
            ..FileFacts::default()
        })
    }

    fn resolve(&self, _: &RepoPath, _: &Import) -> Resolution {
        Resolution::Builtin
    }
}

/// The `g.external` of the Kotlin stand-in, running `command` in the repository root.
fn setup(command: &[&str]) -> ExternalSetup {
    ExternalSetup {
        name: "kotlin".to_owned(),
        cmd: command.iter().copied().map(str::to_owned).collect(),
        files: vec!["**/*.kt".to_owned()],
        version: "1".to_owned(),
        env: Vec::new(),
        provides: Vec::new(),
        options: json!({}),
    }
}

fn kotlin(timeout: Duration) -> External {
    External::new(&setup(&["sh", "kotlin.sh"]))
        .unwrap()
        .with_timeout(timeout)
}

/// A shell `case` arm: the pattern a request line is matched against, and the shell that answers it. The quotes keep the shell from reading the JSON of the pattern as globs.
type Arm = (String, String);

fn when(pattern: &str, action: impl Into<String>) -> Arm {
    (pattern.to_owned(), action.into())
}

fn request(kind: &str) -> String {
    format!(r#"*'"type":"{kind}"'*"#)
}

fn parse_of(path: &str) -> String {
    format!(r#"*'"type":"parse","path":"{path}"'*"#)
}

fn resolve_of(path: &str, spec: &str) -> String {
    format!(r#"*'"type":"resolve","from":"{path}","import":{{"spec":"{spec}"'*"#)
}

/// The shell that writes one message line.
fn say(message: &Value) -> String {
    format!(r"printf '%s\n' '{message}'")
}

fn ready() -> Value {
    json!({ "type": "ready", "name": "kotlin", "version": "0.1.0" })
}

fn facts_of(path: &str, facts: &FileFacts) -> Value {
    json!({ "type": "facts", "path": path, "facts": facts })
}

/// The `resolution` of the import of `spec` on `line` of `from`, as a process sends it: with the file and the import it answers.
fn resolution(from: &str, spec: &str, line: u32, to: &Value) -> Value {
    let import = Import::new(spec, ImportKind::Static, line);
    json!({ "type": "resolution", "from": from, "import": import, "resolution": to })
}

fn imports(specs: &[&str]) -> FileFacts {
    FileFacts {
        imports: specs
            .iter()
            .zip(1..)
            .map(|(spec, line)| Import::new(*spec, ImportKind::Static, line))
            .collect(),
        ..FileFacts::default()
    }
}

/// The text of the fake process.
fn script(arms: &[Arm]) -> String {
    let mut text = String::from(
        "#!/bin/sh\ntouch started\nwhile IFS= read -r line; do\n  printf '%s\\n' \"$line\" >> requests.log\n  case \"$line\" in\n",
    );
    for (pattern, action) in arms {
        writeln!(text, "    {pattern}) {action} ;;").unwrap();
    }
    text.push_str("  esac\ndone\n");
    text
}

/// The arm that answers the parse of `path` with `facts`.
fn parses(path: &str, facts: &FileFacts) -> Arm {
    when(&parse_of(path), say(&facts_of(path, facts)))
}

/// The answers of a process that does everything right for the Kotlin files of [`repository`].
fn well_behaved() -> Vec<Arm> {
    vec![
        when(&request("open"), say(&ready())),
        parses("src/A.kt", &imports(&["./B"])),
        parses(
            "src/B.kt",
            &imports(&["okio", "kotlin.io", "./gen", "./ghost"]),
        ),
        parses("src/C.kt", &FileFacts::default()),
        when(
            &resolve_of("src/A.kt", "./B"),
            say(&resolution(
                "src/A.kt",
                "./B",
                1,
                &json!({ "to": "file", "path": "src/B.kt" }),
            )),
        ),
        when(
            &resolve_of("src/B.kt", "okio"),
            say(&resolution(
                "src/B.kt",
                "okio",
                1,
                &json!({ "to": "external", "package": "okio" }),
            )),
        ),
        when(
            &resolve_of("src/B.kt", "kotlin.io"),
            say(&resolution(
                "src/B.kt",
                "kotlin.io",
                2,
                &json!({ "to": "builtin" }),
            )),
        ),
        // Out of order and twice, as a process may send it; gsttt sorts and dedups.
        when(
            &resolve_of("src/B.kt", "./gen"),
            say(&resolution(
                "src/B.kt",
                "./gen",
                3,
                &json!({ "to": "files", "paths": ["src/B.kt", "src/A.kt", "src/B.kt", "src/gone.kt"] }),
            )),
        ),
        when(
            &resolve_of("src/B.kt", "./ghost"),
            say(&resolution(
                "src/B.kt",
                "./ghost",
                4,
                &json!({ "to": "unresolved", "reason": "not-found" }),
            )),
        ),
        when(&request("done"), "exit 0"),
    ]
}

/// A repository of Kotlin files, a text file and the fake process.
fn repository(arms: &[Arm]) -> Tree {
    let tree = Tree::empty();
    tree.write("src/A.kt", "import B\n");
    tree.write("src/B.kt", "import okio\n");
    tree.write("src/C.kt", "class C\n");
    tree.write("notes/a.txt", "one\ntwo");
    tree.write("kotlin.sh", &script(arms));
    tree
}

/// A run over a repository with one thread, so that the files of the external are asked in path order.
struct Run {
    tree: Tree,
    repo: RepoView,
    output: Output,
}

impl Run {
    fn index(tree: Tree, external: External) -> Self {
        let repo = RepoView::scan(tree.root().to_str().unwrap()).unwrap();
        let plain = Erased(Plain);
        let kotlin = Erased(external);
        let modules = [
            Module {
                connector: &plain,
                options: Value::Null,
                packs: Vec::new(),
            },
            Module {
                connector: &kotlin,
                options: Value::Null,
                packs: Vec::new(),
            },
        ];
        let output = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap()
            .install(|| run(&repo, &modules, LIMIT))
            .unwrap();
        Self { tree, repo, output }
    }

    fn facts(&self, path: &str) -> &FileFacts {
        self.output
            .files
            .iter()
            .find(|file| {
                self.repo
                    .path(file.id)
                    .is_some_and(|found| found.as_str() == path)
            })
            .map(|file| &file.facts)
            .unwrap()
    }

    /// Where the imports of a file point, as the output writes them.
    fn targets(&self, path: &str) -> Vec<Value> {
        let json = self.output.to_json(&self.repo);
        json["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["path"] == path)
            .unwrap()["imports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|import| import["to"].clone())
            .collect()
    }

    /// The lines the process read, in order.
    fn requests(&self) -> Vec<String> {
        fs::read_to_string(self.tree.root().join("requests.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn started(&self) -> bool {
        self.tree.root().join("started").exists()
    }
}

/// The message of a file that failed as a panic.
fn panic_message(facts: &FileFacts) -> &str {
    let ParseStatus::Failed(ParseFailure::Panicked { message }) = &facts.status else {
        return "";
    };
    message
}

/// A child of a fake process, found by the id the process wrote to `grandchild.pid` with `sleep 60 & echo $! > grandchild.pid`. It is killed when the test ends, so that a failing test does not leave a sleep behind that holds a pipe for a minute.
struct Grandchild(String);

impl Grandchild {
    fn of(run: &Run) -> Self {
        let id = fs::read_to_string(run.tree.root().join("grandchild.pid")).unwrap();
        Self(id.trim().to_owned())
    }

    /// Whether it still runs. A zombie does not: nothing holds a pipe open through it any more.
    fn runs(&self) -> bool {
        let output = Command::new("ps")
            .args(["-o", "stat=", "-p", &self.0])
            .output()
            .unwrap();
        let state = String::from_utf8_lossy(&output.stdout);
        !state.trim().is_empty() && !state.trim_start().starts_with('Z')
    }

    /// Whether it is gone within the few seconds a kill takes to be seen.
    fn is_gone_soon(&self) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.runs() {
            if Instant::now() > deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(20));
        }
        true
    }
}

impl Drop for Grandchild {
    fn drop(&mut self) {
        if self.runs() {
            Command::new("kill").args(["-KILL", &self.0]).output().ok();
        }
    }
}

#[gsttt_scenario::scenario("config", "An unused external starts nothing")]
#[test]
fn an_unused_external_starts_nothing() {
    let tree = repository(&well_behaved());
    let repo = RepoView::scan(tree.root().to_str().unwrap()).unwrap();
    let connector = Erased(kotlin(External::TIMEOUT));

    // A run whose files are all answered from the snapshot opens the connector and asks a project what it needs to
    // key the cache, and nothing else: it parses and resolves nothing.
    assert!(connector.detect(&repo));
    let projects = connector.open(&repo, &Value::Null, &[]).unwrap();
    assert_eq!(projects.len(), 1);
    let project = projects.first().unwrap();
    assert_eq!(project.root(), "");
    assert!(project.matches(&RepoPath::normalize("src/A.kt").unwrap()));
    assert!(!project.matches(&RepoPath::normalize("notes/a.txt").unwrap()));
    assert_ne!(project.salt(), 0);
    assert_eq!(project.provides(), Vec::new());
    assert!(
        !tree.root().join("started").exists(),
        "the command ran without a request"
    );

    // The control: a request starts it, in the repository root.
    let id = repo.lookup("src/A.kt").unwrap();
    let path = RepoPath::normalize("src/A.kt").unwrap();
    project.parse(&SourceFile {
        id,
        path: &path,
        text: "import B\n",
    });
    assert!(tree.root().join("started").exists());
}

#[gsttt_scenario::scenario("config", "A process sees only the environment it was given")]
#[test]
fn a_process_sees_only_the_environment_it_was_given() {
    // Cargo gives every test its package name and manifest directory. The first is listed in `env`; the second is neither listed nor inherited.
    let (listed, withheld) = ("CARGO_PKG_NAME", "CARGO_MANIFEST_DIR");
    for name in ["PATH", listed, withheld] {
        assert!(
            env::var_os(name).is_some(),
            "{name} is not set: run the tests through cargo"
        );
    }
    let tree = repository(&[
        when(
            &request("open"),
            format!("env > environment.log; {}", say(&ready())),
        ),
        parses("src/A.kt", &FileFacts::default()),
    ]);
    let repo = RepoView::scan(tree.root().to_str().unwrap()).unwrap();
    let external = External::new(&ExternalSetup {
        env: vec![listed.to_owned()],
        ..setup(&["sh", "kotlin.sh"])
    })
    .unwrap();
    let connector = Erased(external);
    let projects = connector.open(&repo, &Value::Null, &[]).unwrap();
    let path = RepoPath::normalize("src/A.kt").unwrap();
    projects.first().unwrap().parse(&SourceFile {
        id: repo.lookup("src/A.kt").unwrap(),
        path: &path,
        text: "",
    });

    // The process wrote its environment before it said `ready`, so the file is complete once the parse has an answer.
    let log = fs::read_to_string(tree.root().join("environment.log")).unwrap();
    let names: Vec<&str> = log
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(name, _)| name)
        .collect();
    assert!(names.contains(&"PATH"), "inherited: {names:?}");
    assert!(names.contains(&listed), "listed: {names:?}");
    assert!(!names.contains(&withheld), "withheld: {names:?}");
}

#[test]
fn a_repository_without_the_files_of_an_external_opens_no_project() {
    let tree = Tree::empty();
    tree.write("notes/a.txt", "one");
    let repo = RepoView::scan(tree.root().to_str().unwrap()).unwrap();
    let connector = Erased(kotlin(External::TIMEOUT));
    assert!(!connector.detect(&repo));
    assert!(connector.open(&repo, &Value::Null, &[]).unwrap().is_empty());
}

/// A `g.external` that declares what the connector provides and the options the process reads.
const DECLARED: &str = r#"
local g = require("gsttt")
g.external {
  name = "kotlin",
  cmd = { "sh", "kotlin.sh" },
  files = { "**/*.kt" },
  provides = { { kind = "http", role = "serves", via = { "ktor" }, blind_spots = { "routes built at run time" } } },
  options = { strict = true, depth = 3 },
}
"#;

/// The external the config of `tree` declares.
fn declared(tree: &Tree) -> External {
    tree.write(".gsttt/init.lua", DECLARED);
    let setup = tree.evaluate().unwrap();
    External::new(setup.externals.first().unwrap()).unwrap()
}

#[gsttt_scenario::scenario("config", "Declared provides need no process")]
#[test]
fn declared_provides_need_no_process() {
    let tree = repository(&well_behaved());
    let repo = RepoView::scan(tree.root().to_str().unwrap()).unwrap();
    let connector = Erased(declared(&tree));
    let projects = connector.open(&repo, &Value::Null, &[]).unwrap();
    assert_eq!(
        projects.first().unwrap().provides(),
        vec![Provide {
            kind: ContractKind::Http,
            role: Role::Serves,
            via: vec!["ktor".to_owned()],
            blind_spots: vec!["routes built at run time".to_owned()],
        }]
    );
    assert!(
        !tree.root().join("started").exists(),
        "the command ran to say what it provides"
    );
}

#[gsttt_scenario::scenario("config", "Options reach the process")]
#[test]
fn options_reach_the_process() {
    let tree = repository(&well_behaved());
    let external = declared(&tree);
    let run = Run::index(tree, external);
    // The keys come sorted from the config, so the line is the same on every machine.
    assert_eq!(
        run.requests().first().map(String::as_str),
        Some(r#"{"type":"open","protocol":1,"root":"","options":{"depth":3,"strict":true}}"#)
    );
}

#[gsttt_scenario::scenario("config", "A well-behaved process indexes its files")]
#[test]
fn a_well_behaved_process_indexes_its_files() {
    let run = Run::index(repository(&well_behaved()), kotlin(External::TIMEOUT));

    assert_eq!(
        run.facts("src/A.kt").imports,
        imports(&["./B"]).imports,
        "the facts it sent"
    );
    assert_eq!(
        run.facts("src/B.kt").imports,
        imports(&["okio", "kotlin.io", "./gen", "./ghost"]).imports
    );
    assert_eq!(run.targets("src/A.kt"), [json!({ "file": "src/B.kt" })]);
    assert_eq!(
        run.targets("src/B.kt"),
        [
            json!({ "external": "okio" }),
            json!("builtin"),
            json!({ "files": ["src/A.kt", "src/B.kt"] }),
            json!({ "unresolved": { "reason": "not-found" } }),
        ]
    );
    assert!(run.output.problems.is_empty(), "{:?}", run.output.problems);
    insta::assert_json_snapshot!(run.output.to_json(&run.repo));

    // What the process was sent: the lines of the architecture, in the order a run asks them.
    assert!(run.started());
    let requests = run.requests();
    assert_eq!(requests.first().map(String::as_str), Some(OPEN_LINE));
    assert_eq!(
        requests.get(1).map(String::as_str),
        Some(r#"{"type":"parse","path":"src/A.kt","text":"import B\n"}"#)
    );
    assert!(requests.contains(
        &r#"{"type":"resolve","from":"src/A.kt","import":{"spec":"./B","kind":"static","line":1,"flags":0,"names":[]}}"#
            .to_owned()
    ));
    assert_eq!(requests.last().map(String::as_str), Some(DONE_LINE));
    assert!(
        !requests.iter().any(|line| line.contains("notes/a.txt")),
        "the files of other connectors stay with them"
    );
}

#[gsttt_scenario::scenario("config", "A hung process fails its files and nothing else")]
#[test]
fn a_hung_process_fails_its_files_and_nothing_else() {
    assert_eq!(
        External::TIMEOUT,
        Duration::from_secs(10),
        "the timeout of the spec"
    );
    let tree = repository(&[
        when(&request("open"), say(&ready())),
        parses("src/A.kt", &imports(&["./B"])),
        when(&parse_of("src/B.kt"), "sleep 60"),
    ]);
    // Long enough for a loaded machine to start the shell, short enough to keep the test quick.
    let run = Run::index(tree, kotlin(Duration::from_secs(3)));

    for path in ["src/B.kt", "src/C.kt"] {
        let message = panic_message(run.facts(path));
        assert!(
            message.contains("`kotlin`") && message.contains("src/B.kt"),
            "{path}: {message:?}"
        );
        assert!(message.contains("no answer within"), "{message:?}");
    }
    assert_eq!(
        panic_message(run.facts("src/B.kt")),
        panic_message(run.facts("src/C.kt")),
        "a stopped process is not started again"
    );
    assert_eq!(
        run.facts("src/A.kt").status,
        ParseStatus::Ok,
        "answered before the hang"
    );

    // The other connectors' files are indexed.
    assert_eq!(run.facts("notes/a.txt").status, ParseStatus::Ok);
    assert_eq!(run.facts("notes/a.txt").imports.len(), 2);
    assert_eq!(
        run.targets("notes/a.txt"),
        [json!("builtin"), json!("builtin")]
    );

    // The imports of a file it did parse cannot be resolved by a process that is gone, and say why.
    let targets = run.targets("src/A.kt");
    let reason = targets.first().unwrap()["unresolved"]["detail"]
        .as_str()
        .unwrap();
    assert!(reason.contains("`kotlin`"), "{targets:?}");

    // The request that hung was the last one it was sent.
    let requests = run.requests();
    assert!(
        requests
            .last()
            .is_some_and(|line| line.contains("src/B.kt")),
        "{requests:?}"
    );
}

#[gsttt_scenario::scenario("config", "A killed process takes its children with it")]
#[test]
fn a_killed_process_takes_its_children_with_it() {
    // A wrapper, as `npx` or `gradle` are: the sleep is a child of the process and holds the pipes it was given.
    let tree = repository(&[
        when(&request("open"), say(&ready())),
        parses("src/A.kt", &FileFacts::default()),
        when(
            &parse_of("src/B.kt"),
            "sleep 60 & echo $! > grandchild.pid; wait",
        ),
    ]);
    let run = Run::index(tree, kotlin(Duration::from_secs(3)));
    assert!(panic_message(run.facts("src/B.kt")).contains("no answer within"));
    let child = Grandchild::of(&run);
    assert!(
        child.is_gone_soon(),
        "the child {} of the killed process still runs",
        child.0
    );
}

#[gsttt_scenario::scenario("config", "A process that leaves takes its children with it")]
#[test]
fn a_process_that_leaves_takes_its_children_with_it() {
    // It leaves at once on `done`, as it should, and leaves a sleep behind.
    let tree = repository(&[
        when(&request("open"), say(&ready())),
        parses("src/A.kt", &FileFacts::default()),
        parses("src/B.kt", &FileFacts::default()),
        parses("src/C.kt", &FileFacts::default()),
        when(
            &request("done"),
            "sleep 60 & echo $! > grandchild.pid; exit 0",
        ),
    ]);
    let run = Run::index(tree, kotlin(External::TIMEOUT));
    let child = Grandchild::of(&run);
    assert!(
        child.is_gone_soon(),
        "the child {} of the process that left still runs",
        child.0
    );
}

/// Runs the Kotlin files against a process that answers `src/A.kt` well and `src/B.kt` with `action`, and returns the messages of `src/B.kt` and `src/C.kt`.
fn fails_on_b(action: &str) -> (Run, String, String) {
    let tree = repository(&[
        when(&request("open"), say(&ready())),
        parses("src/A.kt", &imports(&["./B"])),
        when(&parse_of("src/B.kt"), action),
        parses("src/C.kt", &FileFacts::default()),
    ]);
    let run = Run::index(tree, kotlin(External::TIMEOUT));
    let b = panic_message(run.facts("src/B.kt")).to_owned();
    let c = panic_message(run.facts("src/C.kt")).to_owned();
    (run, b, c)
}

#[test]
fn a_process_that_exits_fails_the_files_it_did_not_answer() {
    let (run, b, c) = fails_on_b("exit 3");
    assert!(
        b.contains("`kotlin`") && b.contains("the process exited"),
        "{b:?}"
    );
    assert_eq!(b, c, "the process is not started again");
    assert_eq!(run.facts("src/A.kt").status, ParseStatus::Ok);
    assert_eq!(run.facts("notes/a.txt").status, ParseStatus::Ok);
    assert!(
        !run.requests().iter().any(|line| line.contains("src/C.kt")),
        "nothing is sent to a process that stopped"
    );
}

#[test]
fn a_process_that_answers_out_of_turn_fails_the_files_it_did_not_answer() {
    let wrong_path = say(&facts_of("src/Z.kt", &FileFacts::default()));
    let wrong_type = say(&resolution(
        "src/B.kt",
        "./x",
        7,
        &json!({ "to": "builtin" }),
    ));
    for (answer, expected) in [
        (
            wrong_path,
            "facts for `src/Z.kt` where facts for `src/B.kt` was due",
        ),
        (
            wrong_type,
            "resolution of `./x` at line 7 in `src/B.kt` where facts for `src/B.kt` was due",
        ),
    ] {
        let (run, b, c) = fails_on_b(&answer);
        assert!(
            b.contains("`kotlin`") && b.contains("out of turn") && b.contains(expected),
            "{b:?}"
        );
        assert_eq!(b, c);
        assert_eq!(run.facts("notes/a.txt").status, ParseStatus::Ok);
        assert!(!run.requests().iter().any(|line| line.contains("src/C.kt")));
    }
}

#[gsttt_scenario::scenario("config", "A resolution of another import is out of turn")]
#[test]
fn a_resolution_of_another_import_is_out_of_turn() {
    // Both imports are in `src/A.kt`, so the answers are lines of the right file and the right shape, for the wrong import.
    let (due, other) = ("./B", "./C");
    let (due_at_line, other_at_line) = (1, 2);
    let stale = resolution(
        "src/A.kt",
        other,
        other_at_line,
        &json!({ "to": "builtin" }),
    );
    let elsewhere = resolution("src/B.kt", due, due_at_line, &json!({ "to": "builtin" }));
    for answer in [stale, elsewhere] {
        let tree = repository(&[
            when(&request("open"), say(&ready())),
            parses("src/A.kt", &imports(&[due, other])),
            parses("src/B.kt", &FileFacts::default()),
            parses("src/C.kt", &FileFacts::default()),
            when(&resolve_of("src/A.kt", due), say(&answer)),
            when(&resolve_of("src/A.kt", other), say(&answer)),
        ]);
        let run = Run::index(tree, kotlin(External::TIMEOUT));
        let targets = run.targets("src/A.kt");
        let detail = targets
            .first()
            .and_then(|target| target["unresolved"]["detail"].as_str())
            .unwrap_or_default();
        assert!(
            targets
                .first()
                .is_some_and(|target| target["unresolved"]["reason"] == "unsupported"),
            "{targets:?}"
        );
        assert!(
            detail.contains("`kotlin`")
                && detail.contains("out of turn")
                && detail.contains("where resolution of `./B` at line 1 in `src/A.kt` was due"),
            "{detail:?}"
        );
        // The process is stopped: the second import gets the same failure and the process never hears of it.
        assert_eq!(targets.get(1), targets.first());
        assert!(
            !run.requests()
                .iter()
                .any(|line| line.contains(r#""spec":"./C""#)),
            "nothing is sent to a process that stopped"
        );
    }
}

#[test]
fn a_process_that_sends_a_line_that_is_not_a_message_fails_the_files_it_did_not_answer() {
    let (run, b, c) = fails_on_b("echo 'this is not json'");
    assert!(
        b.contains("`kotlin`") && b.contains("not a message") && b.contains("this is not json"),
        "{b:?}"
    );
    assert_eq!(b, c);
    assert_eq!(run.facts("notes/a.txt").status, ParseStatus::Ok);
}

#[test]
fn a_command_that_cannot_run_fails_every_file_it_claims() {
    let tree = repository(&[]);
    let external = External::new(&setup(&["gsttt-no-such-command"])).unwrap();
    let run = Run::index(tree, external);
    for path in ["src/A.kt", "src/B.kt", "src/C.kt"] {
        let message = panic_message(run.facts(path));
        assert!(
            message.contains("`kotlin`") && message.contains("gsttt-no-such-command"),
            "{path}: {message:?}"
        );
    }
    assert_eq!(run.facts("notes/a.txt").status, ParseStatus::Ok);
    assert!(!run.started());
}

#[cfg(unix)]
#[test]
fn a_program_with_a_slash_is_found_from_the_repository_root() {
    use std::os::unix::fs::PermissionsExt;

    let tree = repository(&well_behaved());
    fs::set_permissions(
        tree.root().join("kotlin.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let run = Run::index(tree, External::new(&setup(&["./kotlin.sh"])).unwrap());
    assert_eq!(run.facts("src/A.kt").imports, imports(&["./B"]).imports);
    assert_eq!(run.targets("src/A.kt"), [json!({ "file": "src/B.kt" })]);
}

#[test]
fn an_unresolved_reason_of_the_process_reaches_the_output_with_its_detail() {
    let tree = repository(&[
        when(&request("open"), say(&ready())),
        parses("src/A.kt", &imports(&["./B"])),
        parses("src/B.kt", &FileFacts::default()),
        parses("src/C.kt", &FileFacts::default()),
        when(
            &resolve_of("src/A.kt", "./B"),
            say(&resolution(
                "src/A.kt",
                "./B",
                1,
                &json!({ "to": "unresolved", "reason": "unsupported", "detail": "kotlin script" }),
            )),
        ),
        when(&request("done"), "exit 0"),
    ]);
    let run = Run::index(tree, kotlin(External::TIMEOUT));
    assert_eq!(
        run.targets("src/A.kt"),
        [json!({ "unresolved": { "reason": "unsupported", "detail": "kotlin script" } })]
    );
}

#[test]
fn an_external_needs_a_command_and_globs_that_parse() {
    let mut empty = setup(&["x"]);
    empty.cmd.clear();
    let error = External::new(&empty).unwrap_err().to_string();
    assert!(
        error.contains("kotlin") && error.contains("`cmd`"),
        "{error}"
    );

    let mut broken = setup(&["x"]);
    broken.files = vec!["src/[".to_owned()];
    let error = External::new(&broken).unwrap_err().to_string();
    assert!(
        error.contains("kotlin") && error.contains("src/["),
        "{error}"
    );
}

#[test]
fn an_environment_name_must_be_the_name_of_a_variable() {
    for name in ["", "A=B", "A\0B"] {
        let mut broken = setup(&["x"]);
        broken.env = vec![name.to_owned()];
        let error = External::new(&broken).unwrap_err().to_string();
        assert!(
            error.contains("kotlin") && error.contains("`env`"),
            "{name:?}: {error}"
        );
    }
}

#[gsttt_scenario::scenario("config", "A name is bounded")]
#[test]
fn a_name_is_one_to_sixty_four_letters_digits_dashes_and_underscores() {
    let named = |name: &str| ExternalSetup {
        name: name.to_owned(),
        ..setup(&["x"])
    };
    for name in ["k", "a-b_C9", &"x".repeat(64)] {
        External::new(&named(name)).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    for name in [
        "",
        "bad name",
        "dotted.name",
        "naïve",
        "tab\t",
        &"x".repeat(65),
    ] {
        let error = External::new(&named(name)).unwrap_err().to_string();
        assert!(
            error.starts_with("options of `")
                && error.contains("the name must be 1 to 64 letters, digits, `-` or `_`"),
            "{name:?}: {error}"
        );
    }
    // A name can be as long as the config likes, and the message holds only the start of it.
    let error = External::new(&named(&"x\u{1b}".repeat(500_000)))
        .unwrap_err()
        .to_string();
    assert!(
        error.len() < 200 && !error.contains('\u{1b}'),
        "{} bytes: {error:?}",
        error.len()
    );
}

#[test]
fn a_name_is_kept_once_however_often_the_connector_is_built() {
    let name = || Erased(External::new(&setup(&["x"])).unwrap()).meta().name;
    assert!(std::ptr::eq(name(), name()));
}

#[test]
fn options_must_be_a_table_of_named_values() {
    for options in [json!([1, 2]), json!("x"), json!(3), json!(true)] {
        let mut broken = setup(&["x"]);
        broken.options = options.clone();
        let error = External::new(&broken).unwrap_err().to_string();
        assert!(
            error.contains("kotlin") && error.contains("`options`"),
            "{options}: {error}"
        );
    }
    // An empty table reaches the setup as an empty list, and a setup written by hand may leave the options null.
    for options in [json!({}), json!([]), json!(null), json!({ "a": [1, 2] })] {
        let mut fine = setup(&["x"]);
        fine.options = options.clone();
        External::new(&fine).unwrap_or_else(|error| panic!("{options}: {error}"));
    }
}

#[gsttt_scenario::scenario("config", "A changed declaration drops the cached facts")]
#[test]
fn the_cache_key_follows_the_declaration() {
    let tree = repository(&[]);
    let repo = RepoView::scan(tree.root().to_str().unwrap()).unwrap();
    let salt = |setup: &ExternalSetup| {
        let connector = Erased(External::new(setup).unwrap());
        let projects = connector.open(&repo, &Value::Null, &[]).unwrap();
        projects.first().unwrap().salt()
    };
    let base = setup(&["sh", "kotlin.sh"]);
    let mut newer = base.clone();
    newer.version = "2".to_owned();
    let mut other = base.clone();
    other.cmd.push("--strict".to_owned());
    let mut listed = base.clone();
    listed.env.push("JAVA_HOME".to_owned());
    let mut provides = base.clone();
    provides.provides.push(Provide {
        kind: ContractKind::Http,
        role: Role::Serves,
        via: Vec::new(),
        blind_spots: Vec::new(),
    });
    let mut options = base.clone();
    options.options = json!({ "strict": true });
    assert_eq!(salt(&base), salt(&base.clone()));
    for changed in [&newer, &other, &listed, &provides, &options] {
        assert_ne!(salt(&base), salt(changed), "{changed:?}");
    }

    // The order the keys were written in is not a change.
    let (mut first, mut second) = (base.clone(), base.clone());
    first.options = json!({ "a": 1, "b": { "c": 2, "d": 3 } });
    second.options = json!({ "b": { "d": 3, "c": 2 }, "a": 1 });
    assert_eq!(salt(&first), salt(&second));
}

/// The `type` of each variant of a tagged enum in the schema's definitions.
fn tags(schema: &Value, definition: &str) -> Vec<String> {
    schema["$defs"][definition]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|variant| {
            variant["properties"]["type"]["const"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

#[gsttt_scenario::scenario("config", "The protocol has a schema")]
#[test]
fn the_protocol_has_a_schema() {
    let schema = protocol_schema();
    let value = schema.as_value();
    assert_eq!(tags(value, "Request"), ["open", "parse", "resolve", "done"]);
    assert_eq!(tags(value, "Response"), ["ready", "facts", "resolution"]);
    insta::assert_json_snapshot!(schema);
}
