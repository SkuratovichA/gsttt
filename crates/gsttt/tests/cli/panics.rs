//! A panic the engine contains prints nothing, and one that reaches `main` prints one line and exits with 101.
//!
//! The panic hook belongs to the process, so every case runs in a child process: the test starts this test binary
//! again with the one ignored test `child` and a mode, and reads how the child ended. The commands that ship never
//! panic, so the child runs `gsttt::panics::guard`, the function `main` runs everything in, around code that does.

use std::io;
use std::panic::{self, Location};
use std::path::Path;
use std::process::Command;

use camino::Utf8PathBuf;
use gsttt::{Io, Outcome, panics};
use gsttt_core::connector::{
    Connector, ConnectorError, DynConnector, Erased, Meta, Parsed, Project, Provide, SourceFile,
};
use gsttt_core::engine::refresh::Engine;
use gsttt_core::engine::workspace::Workspace;
use gsttt_core::fact::{FileFacts, Import, Resolution, UnresolvedReason};
use gsttt_core::query::coverage;
use gsttt_core::rules::Pack;
use gsttt_core::{Freshness, RepoPath, RepoView, Setup};
use gsttt_scenario::scenario;

use super::support::{Ran, Sandbox, ran};

const ISSUES: &str = "https://github.com/SkuratovichA/gsttt/issues";

/// Reads `.boom` files and panics on the ones that say `explode`.
struct Boom;

struct BoomProject;

impl Connector for Boom {
    type Options = ();
    type Project = BoomProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "boom",
            version: 1,
            summary: "a connector that panics on request",
            extensions: &["boom"],
        }
    }

    fn detect(&self, _: &RepoView) -> bool {
        true
    }

    fn open(&self, _: &RepoView, (): &(), _: &[Pack]) -> Result<Vec<BoomProject>, ConnectorError> {
        Ok(vec![BoomProject])
    }
}

impl Project for BoomProject {
    type Extra = ();

    fn root(&self) -> &'static str {
        ""
    }

    fn matches(&self, path: &RepoPath) -> bool {
        path.extension() == Some("boom")
    }

    fn salt(&self) -> u64 {
        0
    }

    fn provides(&self) -> Vec<Provide> {
        Vec::new()
    }

    fn parse(&self, file: &SourceFile<'_>) -> Parsed<()> {
        assert!(
            !file.text.contains("explode"),
            "the fake parser cannot read {}",
            file.path
        );
        Parsed::new(FileFacts::default())
    }

    fn resolve(&self, _: &RepoPath, _: &Import) -> Resolution {
        Resolution::Unresolved(UnresolvedReason::NotFound)
    }
}

/// Indexes a tree in which one file makes the connector panic, and writes the coverage to stdout.
fn index_the_panicking_tree(io: &mut Io<'_>) {
    let (tree, cache) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::write(tree.path().join("fine.boom"), "fine\n").unwrap();
    std::fs::write(tree.path().join("bad.boom"), "explode\n").unwrap();
    let utf8 = |dir: &Path| Utf8PathBuf::from_path_buf(dir.to_owned()).unwrap();
    let workspace = Workspace::locate_with(&utf8(tree.path()), Some(utf8(cache.path()))).unwrap();
    let connector = Erased(Boom);
    let connectors: [&dyn DynConnector; 1] = [&connector];
    let setup = Setup::default();
    let refresh = Engine::new(workspace, &connectors, &[], &setup)
        .refresh()
        .unwrap();
    writeln!(io.stdout, "{}", coverage(&refresh.index, Freshness::Fresh)).unwrap();
}

/// Panics, after saying on stdout where the panic will be reported, which is the line of the call.
#[track_caller]
fn bug(message: &str) -> Outcome {
    let place = Location::caller();
    println!("the panic is at {}:{}", place.file(), place.line());
    panic!("{message}")
}

/// What the child of a test does, chosen by the `PANICS_MODE` the test sets. It ends the process with the code
/// that `guard` returns, as `main` does.
#[test]
#[ignore = "runs in a child process of the tests below"]
fn child() {
    let mode = std::env::var("PANICS_MODE").unwrap();
    let (mut stdout, mut stderr) = (io::stdout(), io::stderr());
    let mut io = Io::new(&mut stdout, &mut stderr);
    let code = panics::guard(&mut io, |io| match mode.as_str() {
        "contained" => {
            index_the_panicking_tree(io);
            Outcome::Answered
        }
        "contained-then-bug" => {
            index_the_panicking_tree(io);
            bug("a later bug")
        }
        "bug" => bug("the index is out of bounds"),
        // A worker that panics hands the panic to the thread that waits for it with `resume_unwind`, which does not
        // run the hook again: this is how a panic on one of rayon's threads reaches `main`.
        "worker" => match std::thread::spawn(|| bug("a worker broke")).join() {
            Ok(outcome) => outcome,
            Err(payload) => panic::resume_unwind(payload),
        },
        other => panic!("the child has no mode {other}"),
    });
    std::process::exit(i32::from(code));
}

/// Runs the child in `mode`, with `RUST_BACKTRACE` set to `backtrace` when that is given, and returns how it ended.
fn in_child(mode: &str, backtrace: Option<&str>) -> Ran {
    let sandbox = Sandbox::new();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--ignored",
            "--exact",
            "panics::child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("PANICS_MODE", mode)
        .env("HOME", sandbox.home.path())
        .env("XDG_CACHE_HOME", sandbox.cache.path())
        .env("XDG_STATE_HOME", sandbox.state.path())
        .env_remove("RUST_BACKTRACE")
        .current_dir(sandbox.work.path());
    if let Some(value) = backtrace {
        command.env("RUST_BACKTRACE", value);
    }
    ran(&mut command)
}

/// The line `main` prints for the panic the child said it raised.
fn the_line(child: &Ran, message: &str) -> String {
    let place = child
        .stdout
        .lines()
        // The harness has already written `test panics::child ... ` on the line the child's output continues.
        .find_map(|line| line.split_once("the panic is at ").map(|(_, place)| place))
        .unwrap_or_else(|| panic!("the child named no place: {child:?}"));
    format!("gsttt stopped on a bug at {place}: {message}. Please report it at {ISSUES}\n")
}

#[scenario("cli", "A contained panic prints nothing")]
#[test]
fn a_contained_panic_prints_nothing() {
    // With `RUST_BACKTRACE` set a panic that reached the hook would print the standard output, so this is the run
    // in which a contained panic that leaks shows.
    let child = in_child("contained", Some("1"));
    assert_eq!(child.stderr, "", "{child:?}");
    assert_eq!(child.code, Some(0), "{child:?}");
    assert!(child.stdout.contains("bad.boom"), "{child:?}");
    assert!(child.stdout.contains("the parser panicked"), "{child:?}");
}

#[scenario("cli", "An uncontained panic prints one line")]
#[test]
fn an_uncontained_panic_prints_one_line() {
    let child = in_child("bug", None);
    assert_eq!(child.code, Some(101), "{child:?}");
    assert_eq!(
        child.stderr,
        the_line(&child, "the index is out of bounds"),
        "{child:?}"
    );
}

#[test]
fn a_panic_on_a_worker_is_reported_where_the_worker_panicked() {
    let child = in_child("worker", None);
    assert_eq!(child.code, Some(101), "{child:?}");
    assert_eq!(
        child.stderr,
        the_line(&child, "a worker broke"),
        "{child:?}"
    );
}

#[test]
fn a_contained_panic_is_not_mistaken_for_the_bug() {
    let child = in_child("contained-then-bug", None);
    assert!(child.stdout.contains("bad.boom"), "{child:?}");
    assert_eq!(child.code, Some(101), "{child:?}");
    assert_eq!(child.stderr, the_line(&child, "a later bug"), "{child:?}");
}

#[test]
fn rust_backtrace_adds_the_standard_output() {
    let off = in_child("bug", Some("0"));
    assert_eq!(
        off.stderr,
        the_line(&off, "the index is out of bounds"),
        "0 means off: {off:?}"
    );

    let on = in_child("bug", Some("1"));
    assert_eq!(on.code, Some(101), "{on:?}");
    assert!(on.stderr.contains("panicked at"), "{on:?}");
    assert!(
        on.stderr
            .ends_with(&the_line(&on, "the index is out of bounds")),
        "the line comes last: {on:?}"
    );
}
