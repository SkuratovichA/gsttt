//! Which files differ from what git recorded: the work tree against HEAD, or against the best common ancestor of HEAD and another revision.

use std::collections::BTreeMap;

use camino::Utf8Path;
use gix::bstr::ByteSlice;

use super::files::{ContentId, Excluder};
use crate::path::RepoPath;

/// How a file differs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Modified,
    Added,
    Deleted,
}

/// A file that differs, and how.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub path: RepoPath,
    pub kind: ChangeKind,
}

/// The files of a tree with the blob id of each, in path order.
pub type Listing = BTreeMap<RepoPath, ContentId>;

/// Why a comparison with git history failed.
#[derive(Debug, thiserror::Error)]
pub enum ChangesError {
    #[error("cannot read the git history of `{root}`: {message}")]
    History { root: String, message: String },
    #[error("`{revision}` is not a revision of this repository: {message}")]
    Revision { revision: String, message: String },
    #[error("HEAD and `{revision}` have no common ancestor, so there is no base to compare with")]
    NoMergeBase { revision: String },
}

/// The differences from `from` to `to`, in path order.
pub fn diff(from: &Listing, to: &Listing) -> Vec<Change> {
    let mut changes = Vec::new();
    for (path, id) in to {
        match from.get(path) {
            None => changes.push(Change {
                path: path.clone(),
                kind: ChangeKind::Added,
            }),
            Some(before) if before != id => changes.push(Change {
                path: path.clone(),
                kind: ChangeKind::Modified,
            }),
            Some(_) => {}
        }
    }
    changes.extend(
        from.keys()
            .filter(|path| !to.contains_key(*path))
            .map(|path| Change {
                path: path.clone(),
                kind: ChangeKind::Deleted,
            }),
    );
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    changes
}

/// The commit HEAD points at, `None` before the first commit. A HEAD git cannot resolve is an error: it is not the same as a branch without commits.
pub fn head_commit(root: &Utf8Path) -> Result<Option<String>, ChangesError> {
    let repo = open(root)?;
    let commit = head_commit_of(&repo, root)?;
    Ok(commit.map(|commit| commit.id.to_string()))
}

/// The files of the tree HEAD points at, `None` before the first commit.
pub fn head_tree(root: &Utf8Path, excluder: &Excluder) -> Result<Option<Listing>, ChangesError> {
    let repo = open(root)?;
    match head_commit_of(&repo, root)? {
        Some(commit) => flatten(root, &commit, excluder).map(Some),
        None => Ok(None),
    }
}

/// The commit HEAD points at, `None` while the branch it names has no commits.
fn head_commit_of<'r>(
    repo: &'r gix::Repository,
    root: &Utf8Path,
) -> Result<Option<gix::Commit<'r>>, ChangesError> {
    let history = |message: String| ChangesError::History {
        root: root.to_string(),
        message,
    };
    let mut head = repo.head().map_err(|error| history(error.to_string()))?;
    if head.is_unborn() {
        return Ok(None);
    }
    head.peel_to_commit()
        .map(Some)
        .map_err(|error| history(error.to_string()))
}

/// The files of the tree of the best common ancestor of HEAD and `revision`.
pub fn base_tree(
    root: &Utf8Path,
    revision: &str,
    excluder: &Excluder,
) -> Result<Listing, ChangesError> {
    let repo = open(root)?;
    let history = |message: String| ChangesError::History {
        root: root.to_string(),
        message,
    };
    let target = repo
        .rev_parse_single(revision.as_bytes().as_bstr())
        .map_err(|error| ChangesError::Revision {
            revision: revision.to_owned(),
            message: error.to_string(),
        })?
        .detach();
    let head = repo
        .head_id()
        .map_err(|error| history(error.to_string()))?
        .detach();
    let base = repo
        .merge_base(head, target)
        .map_err(|error| history(error.to_string()))?
        .ok_or_else(|| ChangesError::NoMergeBase {
            revision: revision.to_owned(),
        })?
        .detach();
    let commit = repo
        .find_commit(base)
        .map_err(|error| history(error.to_string()))?;
    flatten(root, &commit, excluder)
}

fn open(root: &Utf8Path) -> Result<gix::Repository, ChangesError> {
    gix::open(root.as_std_path()).map_err(|error| ChangesError::History {
        root: root.to_string(),
        message: error.to_string(),
    })
}

/// The regular files of a commit's tree. Symbolic links, submodules and files the setup excludes are left out, the way the listing of the work tree leaves them out.
fn flatten(
    root: &Utf8Path,
    commit: &gix::Commit<'_>,
    excluder: &Excluder,
) -> Result<Listing, ChangesError> {
    let history = |message: String| ChangesError::History {
        root: root.to_string(),
        message,
    };
    let tree = commit.tree().map_err(|error| history(error.to_string()))?;
    let mut recorder = gix::traverse::tree::Recorder::default();
    tree.traverse()
        .breadthfirst(&mut recorder)
        .map_err(|error| history(error.to_string()))?;
    let mut files = Listing::new();
    for entry in recorder.records {
        if !entry.mode.is_blob() {
            continue;
        }
        let Ok(text) = std::str::from_utf8(&entry.filepath) else {
            continue;
        };
        let Ok(path) = RepoPath::normalize(text) else {
            continue;
        };
        let Some(id) = ContentId::from_slice(entry.oid.as_slice()) else {
            continue;
        };
        if !excluder.excludes(&path) {
            files.insert(path, id);
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::files::blob_id;

    fn listing(files: &[(&str, &str)]) -> Listing {
        files
            .iter()
            .map(|(path, text)| (RepoPath::normalize(path).unwrap(), blob_id(text.as_bytes())))
            .collect()
    }

    #[test]
    fn a_difference_is_a_modification_an_addition_or_a_deletion() {
        let from = listing(&[("a", "1"), ("b", "1"), ("c", "1")]);
        let to = listing(&[("a", "1"), ("b", "2"), ("d", "1")]);
        let kinds: Vec<_> = diff(&from, &to)
            .into_iter()
            .map(|change| (change.path.as_str().to_owned(), change.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                ("b".to_owned(), ChangeKind::Modified),
                ("c".to_owned(), ChangeKind::Deleted),
                ("d".to_owned(), ChangeKind::Added),
            ]
        );
    }
}
