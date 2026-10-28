//! `Repository`: the one way the command line and the MCP server read a repository.
//!
//! Every test works in a git repository of its own, because outside git the state goes to the cache directory
//! the process environment names, and a test in this process must not touch the cache of whoever runs it.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use gsttt::repository::{Repository, RepositoryError};
use gsttt::root::RootError;
use gsttt_core::Freshness;
use gsttt_core::engine::changes::ChangeKind;
use gsttt_core::path::OutsideRoot;
use tempfile::TempDir;

use super::support::{commit_all, init_repo, write};

/// A committed repository holding `app/a.ts`, which imports `lib/util.ts`, and `lib/util.ts`.
struct Project {
    /// Keeps the directory alive.
    _dir: TempDir,
    /// The canonical root.
    root: Utf8PathBuf,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();
        init_repo(root.as_std_path());
        write(&root, "lib/util.ts", "export const util = 1;\n");
        write(
            &root,
            "app/a.ts",
            "import { util } from '../lib/util';\nexport const a = util;\n",
        );
        commit_all(root.as_std_path());
        Self { _dir: dir, root }
    }

    fn open(&self) -> Repository {
        Repository::open(&self.root).unwrap()
    }
}

#[test]
fn nothing_is_stored_before_the_first_refresh() {
    let project = Project::new();
    assert!(project.open().stored().unwrap().is_none());
}

#[test]
fn a_refresh_builds_the_index_and_stores_it() {
    let project = Project::new();
    let repository = project.open();

    let first = repository.refreshed().unwrap();
    assert_eq!(first.loaded.freshness, Freshness::Fresh);
    assert!(!first.unchanged);
    assert_eq!((first.parsed, first.reused), (2, 0));
    assert_eq!(first.loaded.index.graph.file_count(), 2);

    let stored = repository.stored().unwrap().unwrap();
    assert_eq!(stored.freshness, Freshness::Fresh);
    assert_eq!(stored.index.graph.file_count(), 2);
}

#[test]
fn a_refresh_of_an_unchanged_tree_parses_nothing() {
    let project = Project::new();
    let repository = project.open();
    repository.refreshed().unwrap();

    let second = repository.refreshed().unwrap();
    assert!(second.unchanged);
    assert_eq!(second.loaded.freshness, Freshness::Fresh);
}

#[test]
fn a_refresh_after_an_edit_parses_the_edited_file_and_reuses_the_rest() {
    let project = Project::new();
    let repository = project.open();
    repository.refreshed().unwrap();

    write(&project.root, "app/a.ts", "export const a = 12345;\n");
    let second = repository.refreshed().unwrap();
    assert!(!second.unchanged);
    assert_eq!((second.parsed, second.reused), (1, 1));
}

#[test]
fn the_stored_index_says_how_far_the_tree_moved_without_refreshing_it() {
    let project = Project::new();
    let repository = project.open();
    repository.refreshed().unwrap();
    write(&project.root, "lib/util.ts", "export const util = 22222;\n");

    for _ in 0..2 {
        let stored = repository.stored().unwrap().unwrap();
        assert_eq!(
            stored.freshness,
            Freshness::Stale {
                added: 0,
                removed: 0,
                changed: 1
            }
        );
    }
}

#[test]
fn a_repository_opened_again_reads_what_the_first_one_stored() {
    let project = Project::new();
    project.open().refreshed().unwrap();
    let stored = project.open().stored().unwrap().unwrap();
    assert_eq!(stored.freshness, Freshness::Fresh);
}

#[test]
fn an_argument_is_read_against_its_base_and_says_whether_the_file_is_there() {
    let project = Project::new();
    let repository = project.open();
    let root = &project.root;

    let target = repository
        .argument(&root.join("app"), "../lib/util.ts")
        .unwrap();
    assert_eq!(target.path.as_str(), "lib/util.ts");
    assert!(target.on_disk);

    let absolute = repository
        .argument(&root.join("app"), root.join("lib/util.ts").as_str())
        .unwrap();
    assert_eq!(absolute.path.as_str(), "lib/util.ts");

    let gone = repository.argument(root, "lib/gone.ts").unwrap();
    assert_eq!(gone.path.as_str(), "lib/gone.ts");
    assert!(!gone.on_disk);
}

#[test]
fn an_argument_that_leaves_the_repository_is_refused_with_a_sentence() {
    let project = Project::new();
    let repository = project.open();

    let error = repository
        .argument(&project.root.join("app"), "../../elsewhere.ts")
        .unwrap_err();
    assert!(
        matches!(&error, RepositoryError::Path(OutsideRoot::Leaves { .. })),
        "{error:?}"
    );
    let said = error.to_string();
    assert!(
        said.starts_with("`../../elsewhere.ts` is outside the repository `"),
        "{said}"
    );
    assert!(
        said.ends_with("; name a file inside the repository"),
        "{said}"
    );

    let error = repository.argument(&project.root, ".").unwrap_err();
    assert!(
        matches!(&error, RepositoryError::Path(OutsideRoot::IsRoot { .. })),
        "{error:?}"
    );
}

#[test]
fn the_changed_files_are_the_difference_from_head_untracked_files_included() {
    let project = Project::new();
    let repository = project.open();
    write(&project.root, "lib/util.ts", "export const util = 2;\n");
    write(&project.root, "lib/new.ts", "export const n = 1;\n");
    fs::remove_file(project.root.join("app/a.ts")).unwrap();

    let changed: Vec<(String, ChangeKind)> = repository
        .changed(None)
        .unwrap()
        .into_iter()
        .map(|change| (change.path.as_str().to_owned(), change.kind))
        .collect();
    let change = |path: &str, kind| (path.to_owned(), kind);
    assert_eq!(
        changed,
        [
            change("app/a.ts", ChangeKind::Deleted),
            change("lib/new.ts", ChangeKind::Added),
            change("lib/util.ts", ChangeKind::Modified),
        ]
    );
}

#[test]
fn a_base_that_is_no_revision_is_an_error_that_names_it() {
    let project = Project::new();
    let error = project.open().changed(Some("no-such-branch")).unwrap_err();
    let said = error.to_string();
    assert!(said.contains("`no-such-branch`"), "{said}");
    assert!(said.contains("not a revision"), "{said}");
}

#[test]
fn the_changed_files_need_a_git_repository() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();
    let error = Repository::open(&root).unwrap().changed(None).unwrap_err();
    assert!(matches!(&error, RepositoryError::NoGit { .. }), "{error:?}");
    let said = error.to_string();
    assert!(said.contains(root.as_str()), "{said}");
    assert!(said.contains("not in a git repository"), "{said}");
    assert!(said.contains("name the files instead"), "{said}");
}

#[test]
fn the_filesystem_root_is_refused_before_anything_is_read() {
    let error = Repository::open(Utf8Path::new("/")).unwrap_err();
    assert!(
        matches!(&error, RepositoryError::Root(RootError::FilesystemRoot)),
        "{error:?}"
    );
}
