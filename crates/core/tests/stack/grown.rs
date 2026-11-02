use std::hint::black_box;

use camino::Utf8PathBuf;
use gsttt_core::connector::{
    Connector, ConnectorError, DynConnector, Erased, Meta, Parsed, Project, Provide, SourceFile,
};
use gsttt_core::engine::refresh::Engine;
use gsttt_core::engine::workspace::Workspace;
use gsttt_core::fact::{FileFacts, Import, ImportKind, Resolution, UnresolvedReason};
use gsttt_core::rules::Pack;
use gsttt_core::{RepoPath, RepoView, Setup};
use gsttt_scenario::scenario;

/// How many bytes the deep file has: at 2 KiB of stack for each, a worker's 2 MiB is gone at the thousandth.
const DEEP_BYTES: usize = 100_000;

/// Reads `.deep` files, and recurses once for each byte of the text, as a parser of nested text does.
struct Deep;

struct DeepProject;

impl Connector for Deep {
    type Options = ();
    type Project = DeepProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "deep",
            version: 1,
            summary: "a connector whose parse recurses once for each byte",
            extensions: &["deep"],
        }
    }

    fn detect(&self, _: &RepoView) -> bool {
        true
    }

    fn open(&self, _: &RepoView, (): &(), _: &[Pack]) -> Result<Vec<DeepProject>, ConnectorError> {
        Ok(vec![DeepProject])
    }
}

/// Takes 2 KiB of stack and calls itself for the rest of `bytes`, which is a frame within what the engine reserves for each byte.
fn descend(bytes: &[u8]) -> usize {
    let Some((_, rest)) = bytes.split_first() else {
        return 0;
    };
    let mut frame = [0u8; 2048];
    frame[rest.len() % 2048] = 1;
    black_box(&mut frame);
    descend(rest) + usize::from(frame[0])
}

impl Project for DeepProject {
    type Extra = ();

    fn root(&self) -> &'static str {
        ""
    }

    fn matches(&self, path: &RepoPath) -> bool {
        path.extension() == Some("deep")
    }

    fn salt(&self) -> u64 {
        0
    }

    fn provides(&self) -> Vec<Provide> {
        Vec::new()
    }

    fn parse(&self, file: &SourceFile<'_>) -> Parsed<()> {
        black_box(descend(file.text.as_bytes()));
        let mut facts = FileFacts::default();
        facts
            .imports
            .push(Import::new("somewhere".to_owned(), ImportKind::Static, 1));
        Parsed::new(facts)
    }

    fn resolve(&self, _: &RepoPath, _: &Import) -> Resolution {
        Resolution::Unresolved(UnresolvedReason::NotFound)
    }
}

#[scenario(
    "indexing",
    "A parse that needs more stack than a thread has still ends with the facts of its file"
)]
#[test]
fn a_parse_deeper_than_a_workers_stack_ends_with_the_facts_of_its_file() {
    let (tree, cache) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::write(tree.path().join("fine.deep"), "x\n").unwrap();
    std::fs::write(tree.path().join("deep.deep"), "x".repeat(DEEP_BYTES)).unwrap();
    let utf8 = |dir: &std::path::Path| Utf8PathBuf::from_path_buf(dir.to_owned()).unwrap();
    let workspace = Workspace::locate_with(&utf8(tree.path()), Some(utf8(cache.path()))).unwrap();
    let connector = Erased(Deep);
    let connectors: [&dyn DynConnector; 1] = [&connector];
    let setup = Setup::default();

    let refresh = Engine::new(workspace, &connectors, &[], &setup)
        .refresh()
        .unwrap();

    assert_eq!(refresh.parsed, 2);
    let report = gsttt_core::query::coverage(&refresh.index, gsttt_core::Freshness::Fresh);
    assert!(
        report.to_string().contains("deep read 2 of 2 files"),
        "{report}"
    );
}
