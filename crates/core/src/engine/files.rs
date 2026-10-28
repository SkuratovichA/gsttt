//! The file set of a work tree and the content id of each file.
//!
//! In git the set is the tracked files that are present plus the untracked files git does not ignore, minus
//! submodules, symbolic links and skip-worktree entries. Outside git it is every file the ignore files inside the
//! root leave in. `Setup.exclude` comes off either way.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs::{self, Metadata};
use std::time::UNIX_EPOCH;

use camino::{Utf8Path, Utf8PathBuf};
use gix::index::entry::{Flags, Mode, Stat, stat};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use rayon::prelude::*;

use super::workspace::{GitDirs, Workspace};
use crate::path::RepoPath;
use crate::repo::RepoView;

/// Files above this size get no content id: reading them whole costs more than the answer is worth, and the
/// engine leaves such files unparsed anyway.
const HASH_LIMIT: u64 = 256 * 1024 * 1024;

/// The git blob id of a file's content: the same 20 bytes `git hash-object` prints.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ContentId([u8; 20]);

impl ContentId {
    /// The id of a file that could not be read or is too big to hash. It is never a real blob id.
    pub const UNKNOWN: Self = Self([0; 20]);

    /// The id held by `bytes`, `None` unless they are exactly 20.
    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        <[u8; 20]>::try_from(bytes).ok().map(Self)
    }

    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    /// The id as 40 lower-case hex digits.
    pub fn to_hex(self) -> String {
        let mut hex = String::with_capacity(40);
        for byte in self.0 {
            // Writing to a String cannot fail.
            let _ = write!(hex, "{byte:02x}");
        }
        hex
    }
}

/// What the engine knows about a file's content without parsing it: the id, and the size and modification time
/// that let a later run skip hashing outside git.
#[derive(Clone, Copy, Debug, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Stamp {
    pub id: ContentId,
    pub size: u64,
    pub mtime_secs: i64,
    pub mtime_nanos: u32,
}

/// A file of the set with its stamp.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileState {
    pub path: RepoPath,
    pub stamp: Stamp,
}

/// Why the file set could not be listed.
#[derive(Debug, thiserror::Error)]
pub enum FilesError {
    #[error("cannot read the git index of `{root}`: {message}")]
    Index { root: Utf8PathBuf, message: String },
    #[error("cannot list the files under `{root}`: {message}")]
    List { root: Utf8PathBuf, message: String },
    #[error("the `exclude` pattern `{pattern}` is not a gitignore pattern: {message}")]
    Exclude { pattern: String, message: String },
}

/// `Setup.exclude`, compiled.
#[derive(Debug)]
pub struct Excluder(Gitignore);

impl Excluder {
    /// Compiles gitignore-style `patterns`, matched against paths relative to `root`.
    pub fn new(root: &Utf8Path, patterns: &[String]) -> Result<Self, FilesError> {
        let mut builder = GitignoreBuilder::new(root);
        for pattern in patterns {
            builder
                .add_line(None, pattern)
                .map_err(|error| FilesError::Exclude {
                    pattern: pattern.clone(),
                    message: error.to_string(),
                })?;
        }
        let gitignore = builder.build().map_err(|error| FilesError::Exclude {
            pattern: patterns.join(", "),
            message: error.to_string(),
        })?;
        Ok(Self(gitignore))
    }

    /// Whether a pattern leaves `path`, or a directory above it, out.
    pub fn excludes(&self, path: &RepoPath) -> bool {
        self.0
            .matched_path_or_any_parents(path.as_str(), false)
            .is_ignore()
    }
}

/// The files of the work tree in path order, each with its stamp. `previous` gives the stamp a file had in the
/// last snapshot; outside git a file whose size and modification time still match keeps that id unread.
pub fn scan(
    workspace: &Workspace,
    exclude: &[String],
    previous: &(dyn Fn(&RepoPath) -> Option<Stamp> + Sync),
) -> Result<Vec<FileState>, FilesError> {
    let excluder = Excluder::new(workspace.root(), exclude)?;
    let mut states = match workspace.git() {
        Some(git) => scan_git(workspace.root(), git, &excluder)?,
        None => scan_plain(workspace.root(), &excluder, previous)?,
    };
    states.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    Ok(states)
}

/// The git blob id of `bytes`.
pub fn blob_id(bytes: &[u8]) -> ContentId {
    gix::objs::compute_hash(gix::hash::Kind::Sha1, gix::objs::Kind::Blob, bytes)
        .ok()
        .and_then(|id| ContentId::from_slice(id.as_slice()))
        .unwrap_or(ContentId::UNKNOWN)
}

/// What the git index says about a tracked file.
struct Tracked {
    oid: ContentId,
    stat: Stat,
}

fn scan_git(
    root: &Utf8Path,
    git: &GitDirs,
    excluder: &Excluder,
) -> Result<Vec<FileState>, FilesError> {
    let index_error = |message: String| FilesError::Index {
        root: root.to_owned(),
        message,
    };
    let repo = gix::open(root.as_std_path()).map_err(|error| index_error(error.to_string()))?;
    let index = repo
        .index_or_empty()
        .map_err(|error| index_error(error.to_string()))?;
    let timestamp = index.timestamp();
    let options = stat::Options::default();

    // Every path of the index is known, so the walk below does not list a symbolic link, a skip-worktree entry or
    // a conflicted file as untracked.
    let mut known: HashSet<RepoPath> = HashSet::with_capacity(index.entries().len());
    let mut candidates: Vec<(RepoPath, Option<Tracked>)> =
        Vec::with_capacity(index.entries().len());
    for entry in index.entries() {
        let Ok(text) = std::str::from_utf8(entry.path_in(index.path_backing())) else {
            continue;
        };
        let Ok(path) = RepoPath::normalize(text) else {
            continue;
        };
        let left_out = entry.mode == Mode::SYMLINK
            || entry.mode == Mode::COMMIT
            || entry.mode == Mode::DIR
            || entry.flags.contains(Flags::SKIP_WORKTREE);
        if !left_out {
            // A conflicted path has up to three entries, none of them the file as it is now: it is listed once and
            // hashed, so it stays in the index while the conflict is resolved.
            let staged = entry.stage_raw() == 0;
            if staged || !known.contains(&path) {
                let tracked = ContentId::from_slice(entry.id.as_slice())
                    .filter(|_| staged)
                    .map(|oid| Tracked {
                        oid,
                        stat: entry.stat,
                    });
                candidates.push((path.clone(), tracked));
            }
        }
        known.insert(path);
    }
    candidates.extend(
        walk_untracked(root, git, &known)
            .into_iter()
            .map(|path| (path, None)),
    );
    candidates.retain(|(path, _)| !excluder.excludes(path));

    Ok(candidates
        .into_par_iter()
        .filter_map(|(path, tracked)| {
            stamp_file(root, &path, |_| {
                let tracked = tracked.as_ref()?;
                // The index compares its own stat type, which only gix can build from a path.
                let metadata = gix::index::fs::Metadata::from_path_no_follow(
                    root.join(path.as_str()).as_std_path(),
                )
                .ok()?;
                let current = Stat::from_fs(&metadata).ok()?;
                // A stat that matches is only proof of a clean file when the index was written well after the
                // file was: git's "racy" rule.
                (tracked.stat.matches(&current, options)
                    && !tracked.stat.is_racy(timestamp, options))
                .then_some(tracked.oid)
            })
        })
        .collect())
}

/// The files under `root` that the index does not know and git does not ignore.
fn walk_untracked(root: &Utf8Path, git: &GitDirs, known: &HashSet<RepoPath>) -> Vec<RepoPath> {
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(false)
        .parents(false)
        .ignore(false)
        .git_ignore(true)
        .git_global(true)
        // A linked work tree has no `.git/info`, so the shared one is added by hand below.
        .git_exclude(false)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|entry| entry.file_name() != ".git" && !is_nested_repo(entry));
    // A missing exclude file is the common case, so a failure here is a debug line and not an error.
    if let Some(error) = builder.add_ignore(git.common_dir.join("info").join("exclude")) {
        tracing::debug!(%error, "the shared exclude file was not read");
    }

    let mut found = Vec::new();
    for entry in builder.build() {
        let Ok(entry) = entry else {
            continue;
        };
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root.as_std_path())
            .ok()
            .and_then(|relative| relative.to_str());
        let Some(path) = relative.and_then(|relative| RepoPath::normalize(relative).ok()) else {
            continue;
        };
        if !known.contains(&path) {
            found.push(path);
        }
    }
    found
}

/// A directory with its own `.git` is a submodule or a nested repository, which are not part of this tree.
fn is_nested_repo(entry: &ignore::DirEntry) -> bool {
    entry.depth() > 0
        && entry.file_type().is_some_and(|kind| kind.is_dir())
        && entry.path().join(".git").exists()
}

fn scan_plain(
    root: &Utf8Path,
    excluder: &Excluder,
    previous: &(dyn Fn(&RepoPath) -> Option<Stamp> + Sync),
) -> Result<Vec<FileState>, FilesError> {
    let view = RepoView::scan(root.to_owned()).map_err(|error| FilesError::List {
        root: root.to_owned(),
        message: error.to_string(),
    })?;
    let paths: Vec<&RepoPath> = view
        .files()
        .map(|(_, path)| path)
        .filter(|path| !excluder.excludes(path))
        .collect();
    Ok(paths
        .into_par_iter()
        .filter_map(|path| {
            let before = previous(path);
            stamp_file(root, path, |meta| {
                let before = before?;
                let (secs, nanos) = mtime(meta);
                (before.size == meta.len()
                    && (before.mtime_secs, before.mtime_nanos) == (secs, nanos))
                    .then_some(before.id)
            })
        })
        .collect())
}

/// Stamps a present regular file. `known` gives its id from what is already known, without reading it; when it
/// cannot, the file is hashed. A file that is gone or is not a regular file gives `None`.
fn stamp_file(
    root: &Utf8Path,
    path: &RepoPath,
    known: impl FnOnce(&Metadata) -> Option<ContentId>,
) -> Option<FileState> {
    let absolute = root.join(path.as_str());
    let meta = fs::symlink_metadata(&absolute).ok()?;
    if !meta.is_file() {
        return None;
    }
    let size = meta.len();
    let (mtime_secs, mtime_nanos) = mtime(&meta);
    let id = known(&meta).unwrap_or_else(|| hash_file(&absolute, size));
    Some(FileState {
        path: path.clone(),
        stamp: Stamp {
            id,
            size,
            mtime_secs,
            mtime_nanos,
        },
    })
}

fn hash_file(path: &Utf8Path, size: u64) -> ContentId {
    if size > HASH_LIMIT {
        return ContentId::UNKNOWN;
    }
    fs::read(path).map_or(ContentId::UNKNOWN, |bytes| blob_id(&bytes))
}

fn mtime(meta: &Metadata) -> (i64, u32) {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or((0, 0), |elapsed| {
            (
                i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX),
                elapsed.subsec_nanos(),
            )
        })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::engine::testing::{git, init_repo, merge_with_conflict, write};

    fn scan_dir(dir: &Path, exclude: &[&str]) -> Vec<FileState> {
        let workspace = Workspace::locate_with(Utf8Path::from_path(dir).unwrap(), None).unwrap();
        let exclude: Vec<String> = exclude.iter().map(|&pattern| pattern.to_owned()).collect();
        scan(&workspace, &exclude, &|_| None).unwrap()
    }

    fn paths(states: &[FileState]) -> Vec<&str> {
        states.iter().map(|state| state.path.as_str()).collect()
    }

    #[cfg(unix)]
    #[gsttt_scenario::scenario("indexing", "Ignored, excluded and linked files stay out")]
    #[test]
    fn ignored_excluded_and_linked_files_stay_out() {
        let dir = init_repo(&[("src/a.ts", "a")]);
        // Untracked, and ignoring itself so that it is not a file of the set either.
        write(dir.path(), ".gitignore", "dist/\n.gitignore\n");
        write(dir.path(), "src/new.ts", "new");
        write(dir.path(), "dist/a.js", "built");
        write(dir.path(), "vendor/b.ts", "vendored");
        std::os::unix::fs::symlink("src/a.ts", dir.path().join("link.ts")).unwrap();

        let states = scan_dir(dir.path(), &["vendor/**"]);

        assert_eq!(paths(&states), ["src/a.ts", "src/new.ts"]);
    }

    #[gsttt_scenario::scenario("indexing", "A deleted tracked file is gone")]
    #[test]
    fn a_deleted_tracked_file_is_gone() {
        let dir = init_repo(&[("a.ts", "a"), ("b.ts", "b")]);
        fs::remove_file(dir.path().join("b.ts")).unwrap();

        let states = scan_dir(dir.path(), &[]);

        assert_eq!(paths(&states), ["a.ts"]);
    }

    #[gsttt_scenario::scenario("indexing", "A conflicted file stays in the set")]
    #[test]
    fn a_conflicted_file_stays_in_the_set() {
        let dir = init_repo(&[("a.ts", "base\n"), ("b.ts", "b\n")]);
        git(dir.path(), &["checkout", "-q", "-b", "topic"]);
        write(dir.path(), "a.ts", "topic\n");
        git(dir.path(), &["commit", "-q", "-am", "topic"]);
        git(dir.path(), &["checkout", "-q", "main"]);
        write(dir.path(), "a.ts", "main\n");
        git(dir.path(), &["commit", "-q", "-am", "main"]);
        merge_with_conflict(dir.path(), "topic");

        let states = scan_dir(dir.path(), &[]);

        assert_eq!(paths(&states), ["a.ts", "b.ts"]);
        let expected = git(dir.path(), &["hash-object", "a.ts"]);
        assert_eq!(states[0].stamp.id.to_hex(), expected.trim());
    }

    #[gsttt_scenario::scenario("indexing", "An edited file gets the id git would give it")]
    #[test]
    fn an_edited_file_gets_the_id_git_would_give_it() {
        let dir = init_repo(&[("a.ts", "one"), ("b.ts", "two")]);
        write(dir.path(), "a.ts", "one, edited");
        write(dir.path(), "c.ts", "untracked");

        let states = scan_dir(dir.path(), &[]);

        for state in &states {
            let expected = git(dir.path(), &["hash-object", state.path.as_str()]);
            assert_eq!(state.stamp.id.to_hex(), expected.trim(), "{}", state.path);
        }
        assert_eq!(states.len(), 3);
    }

    #[test]
    fn outside_git_a_file_with_the_same_size_and_time_keeps_its_stored_id() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        write(dir.path(), "a.txt", "content");
        let workspace = Workspace::locate_with(
            Utf8Path::from_path(dir.path()).unwrap(),
            Some(Utf8PathBuf::from_path_buf(cache.path().to_path_buf()).unwrap()),
        )
        .unwrap();
        let first = scan(&workspace, &[], &|_| None).unwrap();
        assert_eq!(first[0].stamp.id, blob_id(b"content"));

        let stored = Stamp {
            id: blob_id(b"stored"),
            ..first[0].stamp
        };
        let second = scan(&workspace, &[], &|_| Some(stored)).unwrap();

        assert_eq!(second[0].stamp.id, stored.id);
    }
}
