//! The file set of a work tree and the content id of each file.
//!
//! In git the set is the tracked files that are present plus the untracked files git does not ignore, minus
//! submodules, symbolic links and skip-worktree entries. Outside git it is every file the ignore files inside the
//! root leave in. `Setup.exclude` comes off either way.
//!
//! A file is identified by the git blob id of its content. One above `Setup.max_file_bytes` is never parsed, so it
//! is not read for its id either: its size and modification time stand in for it.
//!
//! A file that is not clean in the git index keeps the id of the last run when its size and modification time are
//! still the ones that run saw, unless it was written too close to that run's listing for the pair to prove
//! anything ([`Stamp::is_settled`]).

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs::{self, Metadata};
use std::ops::ControlFlow;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use camino::{Utf8Path, Utf8PathBuf};
use gix::dir::entry::{Kind as DiskKind, Status as WalkStatus};
use gix::dir::walk::{Action, Delegate, ForDeletionMode};
use gix::index::entry::{Flags, Mode, Stat, stat};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use rayon::prelude::*;

use super::workspace::Workspace;
use crate::path::RepoPath;
use crate::repo::RepoView;

/// The git blob id of a file's content: the same 20 bytes `git hash-object` prints.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
#[rkyv(compare(PartialEq))]
pub struct ContentId([u8; 20]);

impl ContentId {
    /// The id of a file that could not be read. It is never a real blob id.
    pub const UNKNOWN: Self = Self([0; 20]);

    /// The id of a file that is not read, made of its size, the seconds of its modification time and their
    /// nanoseconds, each little-endian: 8, 8 and 4 bytes. It changes whenever the file is saved, and never equals
    /// the blob id git gives the file, so a touched file above the size limit shows as modified against git.
    pub fn from_stat(size: u64, mtime_secs: i64, mtime_nanos: u32) -> Self {
        let mut id = [0_u8; 20];
        let parts = size
            .to_le_bytes()
            .into_iter()
            .chain(mtime_secs.to_le_bytes())
            .chain(mtime_nanos.to_le_bytes());
        for (slot, byte) in id.iter_mut().zip(parts) {
            *slot = byte;
        }
        Self(id)
    }

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

/// The coarsest resolution of a modification time among the file systems a work tree lives on: FAT's two seconds.
const RACY_SECONDS: i64 = 2;

/// What the engine knows about a file's content without parsing it: the id, and the size and modification time
/// that let a later run skip hashing outside git.
#[derive(Clone, Copy, Debug, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[rkyv(compare(PartialEq))]
pub struct Stamp {
    pub id: ContentId,
    pub size: u64,
    pub mtime_secs: i64,
    pub mtime_nanos: u32,
}

impl Stamp {
    /// The stamp of a file with the content id `id` and the metadata `meta`.
    pub fn of(id: ContentId, meta: &Metadata) -> Self {
        let (size, mtime_secs, mtime_nanos) = stat_of(meta);
        Self {
            id,
            size,
            mtime_secs,
            mtime_nanos,
        }
    }

    /// The id of the file `meta` belongs to, when the file still has the size and the modification time it was
    /// stamped with and the stamp knows its id. It is what lets a run skip hashing.
    pub fn id_of_unchanged(&self, meta: &Metadata) -> Option<ContentId> {
        (self.id != ContentId::UNKNOWN && self.describes(meta)).then_some(self.id)
    }

    /// The stamp of `bytes`, which were read from `path` after the file was stamped as `self`; `None` while the file
    /// is still as stamped. `before` is the file's metadata from just before the read. A file that changed since
    /// the stamp gets the id of the bytes read. One that changed while it was being read has no id, because the
    /// bytes may be a mix of two saves: nothing is filed under it and the next listing hashes the file again.
    pub fn restamp(&self, path: &Utf8Path, before: &Metadata, bytes: &[u8]) -> Option<Self> {
        let Ok(after) = fs::metadata(path) else {
            return Some(Self::of(ContentId::UNKNOWN, before));
        };
        let steady = stat_of(before) == stat_of(&after);
        if steady && self.describes(&after) {
            return None;
        }
        let id = if steady {
            blob_id(bytes)
        } else {
            ContentId::UNKNOWN
        };
        Some(Self::of(id, &after))
    }

    /// Whether the stamp can tell this file from a later save. File systems keep a modification time to one second
    /// (HFS+, ext3) or two (FAT) at best, so a file written within that of `listed_at`, the unix second at which
    /// the listing began, can be saved again with the same size and the same time. The stamp of such a file proves
    /// nothing: it is git's "racy" rule applied to the engine's own stamps.
    pub fn is_settled(&self, listed_at: i64) -> bool {
        self.mtime_secs < listed_at.saturating_sub(RACY_SECONDS)
    }

    /// Whether `meta` is the file as this stamp saw it: the same size and the same modification time.
    fn describes(&self, meta: &Metadata) -> bool {
        (self.size, self.mtime_secs, self.mtime_nanos) == stat_of(meta)
    }
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
        self.matches(path, false)
    }

    /// Whether a pattern leaves the directory `path`, or one above it, out.
    pub fn excludes_dir(&self, path: &RepoPath) -> bool {
        self.matches(path, true)
    }

    fn matches(&self, path: &RepoPath, is_dir: bool) -> bool {
        self.0
            .matched_path_or_any_parents(path.as_str(), is_dir)
            .is_ignore()
    }
}

/// What a scan found.
#[derive(Debug)]
pub struct Scanned {
    /// The files of the work tree in path order, each with its stamp.
    pub files: Vec<FileState>,
    /// The directories the process cannot read, in path order, less the ones `Setup.exclude` leaves out: `.` is the
    /// root. The scan goes on without them, so files under them may be missing from `files`.
    pub unreadable: Vec<String>,
}

/// The files of the work tree in path order, each with its stamp. `previous` gives the stamp a file had in the
/// last snapshot, when that stamp can be trusted ([`Stamp::is_settled`]); a file whose size and modification time
/// still match it keeps its id unread, unless the git index vouches for the file. A file above `max_file_bytes` is
/// not read: its id is made of its size and modification time ([`ContentId::from_stat`]).
pub fn scan(
    workspace: &Workspace,
    exclude: &[String],
    max_file_bytes: u64,
    previous: &(dyn Fn(&RepoPath) -> Option<Stamp> + Sync),
) -> Result<Scanned, FilesError> {
    let excluder = Excluder::new(workspace.root(), exclude)?;
    let mut scanned = if workspace.git().is_some() {
        scan_git(workspace.root(), &excluder, max_file_bytes, previous)?
    } else {
        scan_plain(workspace.root(), &excluder, max_file_bytes, previous)
    };
    scanned.files.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    scanned.unreadable.sort_unstable();
    Ok(scanned)
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
    excluder: &Excluder,
    max_file_bytes: u64,
    previous: &(dyn Fn(&RepoPath) -> Option<Stamp> + Sync),
) -> Result<Scanned, FilesError> {
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
    let walked = walk_untracked(root, &repo, &index, &known)?;
    candidates.extend(walked.files.into_iter().map(|path| (path, None)));
    candidates.retain(|(path, _)| !excluder.excludes(path));
    let unreadable = walked
        .unreadable
        .into_iter()
        .filter(|path| !excluder.excludes_dir(path))
        .map(String::from)
        .collect();

    // The id git's index gives a file that is clean, without reading it.
    let clean_oid = |path: &RepoPath, tracked: Option<&Tracked>| {
        let tracked = tracked?;
        // The index compares its own stat type, which only gix can build from a path.
        let metadata =
            gix::index::fs::Metadata::from_path_no_follow(root.join(path.as_str()).as_std_path())
                .ok()?;
        let current = Stat::from_fs(&metadata).ok()?;
        // A stat that matches is only proof of a clean file when the index was written well after the file was:
        // git's "racy" rule.
        (tracked.stat.matches(&current, options) && !tracked.stat.is_racy(timestamp, options))
            .then_some(tracked.oid)
    };
    let files = candidates
        .into_par_iter()
        .filter_map(|(path, tracked)| {
            stamp_file(root, &path, max_file_bytes, |meta| {
                // A file that is not clean in the index is the same as in the last run when its stamp says so.
                clean_oid(&path, tracked.as_ref())
                    .or_else(|| previous(&path)?.id_of_unchanged(meta))
            })
        })
        .collect();
    Ok(Scanned { files, unreadable })
}

/// The files under `root` that the index does not know and git does not ignore, by git's own rules: the
/// `.gitignore` of every directory, `info/exclude` of the common directory, `core.excludesFile` and
/// `core.ignorecase`. A directory with a `.git` of its own is a submodule or a nested repository and is not
/// entered. A directory that cannot be read is not entered either: it comes back in `unreadable`.
///
/// The walk stops with an error at the first directory it cannot read, where git prints a warning and goes on, so
/// the directories have to be tried before it enters them. Trying every one of them slowed the walk down by a
/// third or more when measured, and only a tracked directory that cannot be read needs it: the untracked ones are
/// tried on the first walk, and when it fails the walk runs again with every directory tried.
fn walk_untracked<'k>(
    root: &'k Utf8Path,
    repo: &gix::Repository,
    index: &gix::index::State,
    known: &'k HashSet<RepoPath>,
) -> Result<Untracked<'k>, FilesError> {
    walk(root, repo, index, known, false).or_else(|_| walk(root, repo, index, known, true))
}

fn walk<'k>(
    root: &'k Utf8Path,
    repo: &gix::Repository,
    index: &gix::index::State,
    known: &'k HashSet<RepoPath>,
    try_tracked: bool,
) -> Result<Untracked<'k>, FilesError> {
    let list_error = |error: gix::Error| FilesError::List {
        root: root.to_owned(),
        message: error.to_string(),
    };
    let options = repo.dirwalk_options().map_err(list_error)?;
    let mut found = Untracked {
        root,
        known,
        try_tracked,
        files: Vec::new(),
        unreadable: Vec::new(),
    };
    repo.dirwalk(
        index,
        None::<&gix::bstr::BStr>,
        &AtomicBool::new(false),
        options,
        &mut found,
    )
    .map_err(list_error)?;
    Ok(found)
}

/// Takes the untracked files of a directory walk that are not known to the index, and the directories the walk
/// does not enter because the process cannot read them.
struct Untracked<'k> {
    root: &'k Utf8Path,
    known: &'k HashSet<RepoPath>,
    /// Whether a tracked directory is tried before the walk enters it, as an untracked one always is.
    try_tracked: bool,
    files: Vec<RepoPath>,
    unreadable: Vec<RepoPath>,
}

impl Delegate for Untracked<'_> {
    /// The directories git enters, less the ones that cannot be opened.
    fn can_recurse(
        &mut self,
        entry: gix::dir::EntryRef<'_>,
        for_deletion: Option<ForDeletionMode>,
        worktree_root_is_repository: bool,
    ) -> bool {
        if !entry.status.can_recurse(
            entry.disk_kind,
            entry.pathspec_match,
            for_deletion,
            worktree_root_is_repository,
        ) {
            return false;
        }
        if entry.status != WalkStatus::Untracked && !self.try_tracked {
            return true;
        }
        // A name that is not UTF-8 holds no file this tool can name.
        let Ok(path) = std::str::from_utf8(&entry.rela_path) else {
            return false;
        };
        let readable = fs::read_dir(self.root.join(path)).is_ok();
        if !readable && let Ok(path) = RepoPath::normalize(path) {
            self.unreadable.push(path);
        }
        readable
    }

    fn emit(
        &mut self,
        entry: gix::dir::EntryRef<'_>,
        _collapsed_directory_status: Option<WalkStatus>,
    ) -> Action {
        // A symbolic link or a nested repository is not a file of this tree, and an ignored entry is never emitted.
        if entry.status == WalkStatus::Untracked && entry.disk_kind == Some(DiskKind::File) {
            let path = std::str::from_utf8(&entry.rela_path)
                .ok()
                .and_then(|text| RepoPath::normalize(text).ok());
            if let Some(path) = path.filter(|path| !self.known.contains(path)) {
                self.files.push(path);
            }
        }
        ControlFlow::Continue(())
    }
}

fn scan_plain(
    root: &Utf8Path,
    excluder: &Excluder,
    max_file_bytes: u64,
    previous: &(dyn Fn(&RepoPath) -> Option<Stamp> + Sync),
) -> Scanned {
    let (view, mut unreadable) = RepoView::scan_reporting(root.to_owned());
    // The root is not a path of the tree, so no pattern leaves it out.
    unreadable.retain(|directory| {
        RepoPath::normalize(directory).map_or(true, |directory| !excluder.excludes_dir(&directory))
    });
    let paths: Vec<&RepoPath> = view
        .files()
        .map(|(_, path)| path)
        .filter(|path| !excluder.excludes(path))
        .collect();
    let files = paths
        .into_par_iter()
        .filter_map(|path| {
            stamp_file(root, path, max_file_bytes, |meta| {
                previous(path)?.id_of_unchanged(meta)
            })
        })
        .collect();
    Scanned { files, unreadable }
}

/// Stamps a present regular file. `known` gives its id from what is already known, without reading it; when it
/// cannot, the file is hashed, unless it is above `max_file_bytes` and so never parsed. A file that is gone or is
/// not a regular file gives `None`.
fn stamp_file(
    root: &Utf8Path,
    path: &RepoPath,
    max_file_bytes: u64,
    known: impl FnOnce(&Metadata) -> Option<ContentId>,
) -> Option<FileState> {
    let absolute = root.join(path.as_str());
    let meta = fs::symlink_metadata(&absolute).ok()?;
    if !meta.is_file() {
        return None;
    }
    let id = known(&meta).unwrap_or_else(|| {
        if meta.len() > max_file_bytes {
            let (size, secs, nanos) = stat_of(&meta);
            ContentId::from_stat(size, secs, nanos)
        } else {
            hash_file(&absolute)
        }
    });
    Some(FileState {
        path: path.clone(),
        stamp: Stamp::of(id, &meta),
    })
}

fn hash_file(path: &Utf8Path) -> ContentId {
    fs::read(path).map_or(ContentId::UNKNOWN, |bytes| blob_id(&bytes))
}

/// The size and the modification time of a file, which tell an edited file from an untouched one.
fn stat_of(meta: &Metadata) -> (u64, i64, u32) {
    let (secs, nanos) = mtime(meta);
    (meta.len(), secs, nanos)
}

/// The current time in seconds since the Unix epoch, on the clock that stamps a file with its modification time.
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
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
    use std::time::Duration;

    use super::*;
    #[cfg(unix)]
    use crate::engine::testing::Locked;
    use crate::engine::testing::{add_worktree, git, init_repo, merge_with_conflict, touch, write};

    fn scan_dir(dir: &Path, exclude: &[&str]) -> Vec<FileState> {
        scan_limited(dir, exclude, u64::MAX)
    }

    fn scan_limited(dir: &Path, exclude: &[&str], max_file_bytes: u64) -> Vec<FileState> {
        scanned(dir, exclude, max_file_bytes).files
    }

    fn scanned(dir: &Path, exclude: &[&str], max_file_bytes: u64) -> Scanned {
        let workspace = Workspace::locate_with(Utf8Path::from_path(dir).unwrap(), None).unwrap();
        let exclude: Vec<String> = exclude.iter().map(|&pattern| pattern.to_owned()).collect();
        scan(&workspace, &exclude, max_file_bytes, &|_| None).unwrap()
    }

    fn paths(states: &[FileState]) -> Vec<&str> {
        states.iter().map(|state| state.path.as_str()).collect()
    }

    #[cfg(unix)]
    fn unreadable(scanned: &Scanned) -> Vec<&str> {
        scanned.unreadable.iter().map(String::as_str).collect()
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

    #[gsttt_scenario::scenario("indexing", "Git's own ignore settings apply")]
    #[test]
    fn the_ignore_settings_of_the_repository_apply() {
        let dir = init_repo(&[("src/a.ts", "a")]);
        let excludes = dir.path().join(".git").join("personal-excludes");
        fs::write(&excludes, "*.skip\n").unwrap();
        git(
            dir.path(),
            &["config", "core.excludesFile", excludes.to_str().unwrap()],
        );
        git(dir.path(), &["config", "core.ignorecase", "true"]);
        // Untracked, and ignoring itself so that it is not a file of the set either. The pattern differs from the
        // directory in case.
        write(dir.path(), ".gitignore", "Build/\n.gitignore\n");
        write(dir.path(), "src/new.ts", "new");
        write(dir.path(), "src/b.skip", "skipped");
        write(dir.path(), "build/out.js", "built");

        let states = scan_dir(dir.path(), &[]);

        assert_eq!(paths(&states), ["src/a.ts", "src/new.ts"]);
    }

    #[test]
    fn the_exclude_file_of_the_repository_reaches_a_linked_work_tree() {
        let dir = init_repo(&[("src/a.ts", "a")]);
        fs::create_dir_all(dir.path().join(".git").join("info")).unwrap();
        fs::write(
            dir.path().join(".git").join("info").join("exclude"),
            "*.local\n",
        )
        .unwrap();
        let (_holder, linked) = add_worktree(dir.path(), "feature");
        write(linked.as_std_path(), "src/b.local", "local");
        write(linked.as_std_path(), "src/c.ts", "c");

        let states = scan_dir(linked.as_std_path(), &[]);

        assert_eq!(paths(&states), ["src/a.ts", "src/c.ts"]);
    }

    #[test]
    fn a_nested_repository_is_not_entered() {
        let dir = init_repo(&[("src/a.ts", "a")]);
        write(dir.path(), "vendor/lib/x.ts", "x");
        git(
            &dir.path().join("vendor").join("lib"),
            &["init", "-q", "-b", "main"],
        );
        write(dir.path(), "src/new.ts", "new");

        let states = scan_dir(dir.path(), &[]);

        assert_eq!(paths(&states), ["src/a.ts", "src/new.ts"]);
    }

    #[cfg(unix)]
    #[gsttt_scenario::scenario(
        "indexing",
        "An untracked directory that cannot be read is left out"
    )]
    #[test]
    fn an_untracked_directory_that_cannot_be_read_is_left_out() {
        let dir = init_repo(&[("src/a.ts", "a")]);
        write(dir.path(), "src/new.ts", "new");
        write(dir.path(), "locked/b.ts", "b");
        let Some(_locked) = Locked::new(&dir.path().join("locked")) else {
            return;
        };

        let scanned = scanned(dir.path(), &[], u64::MAX);

        assert_eq!(paths(&scanned.files), ["src/a.ts", "src/new.ts"]);
        assert_eq!(unreadable(&scanned), ["locked"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_tracked_directory_that_cannot_be_read_is_left_out_and_reported() {
        let dir = init_repo(&[("src/a.ts", "a"), ("locked/b.ts", "b")]);
        write(dir.path(), "src/new.ts", "new");
        write(dir.path(), "vendor/c.ts", "c");
        let Some(_locked) = Locked::new(&dir.path().join("locked")) else {
            return;
        };
        // Excluded, so it is none of the engine's business that it cannot be read.
        let _vendor = Locked::new(&dir.path().join("vendor"));

        let scanned = scanned(dir.path(), &["vendor/"], u64::MAX);

        assert_eq!(paths(&scanned.files), ["src/a.ts", "src/new.ts"]);
        assert_eq!(unreadable(&scanned), ["locked"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_outside_git_that_cannot_be_read_is_left_out_and_reported() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        write(dir.path(), "src/a.ts", "a");
        write(dir.path(), "locked/deep/b.ts", "b");
        write(dir.path(), "vendor/c.ts", "c");
        let Some(_locked) = Locked::new(&dir.path().join("locked")) else {
            return;
        };
        // Excluded, so it is none of the engine's business that it cannot be read.
        let _vendor = Locked::new(&dir.path().join("vendor"));
        let workspace = Workspace::locate_with(
            Utf8Path::from_path(dir.path()).unwrap(),
            Some(Utf8PathBuf::from_path_buf(cache.path().to_path_buf()).unwrap()),
        )
        .unwrap();

        let scanned = scan(&workspace, &["vendor/".to_owned()], u64::MAX, &|_| None).unwrap();

        assert_eq!(paths(&scanned.files), ["src/a.ts"]);
        assert_eq!(unreadable(&scanned), ["locked"]);
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
        let first = scan(&workspace, &[], u64::MAX, &|_| None).unwrap().files;
        assert_eq!(first[0].stamp.id, blob_id(b"content"));

        let stored = Stamp {
            id: blob_id(b"stored"),
            ..first[0].stamp
        };
        let second = scan(&workspace, &[], u64::MAX, &|_| Some(stored))
            .unwrap()
            .files;

        assert_eq!(second[0].stamp.id, stored.id);
    }

    #[test]
    fn a_stat_id_is_made_of_the_size_and_the_time() {
        let id = ContentId::from_stat(1, 2, 3);
        assert_eq!(
            id.as_bytes(),
            &[1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0]
        );
        assert_eq!(
            &ContentId::from_stat(0, -1, 0).as_bytes()[8..16],
            &[0xff; 8]
        );
        assert_ne!(id, ContentId::from_stat(1, 2, 4));
    }

    #[cfg(unix)]
    #[gsttt_scenario::scenario("indexing", "A file above the size limit is not read for its id")]
    #[test]
    fn a_file_above_the_limit_is_not_read_for_its_id() {
        use std::os::unix::fs::PermissionsExt;

        let dir = init_repo(&[("a.ts", "a")]);
        write(dir.path(), "dump.sql", "0123456789");
        let dump = dir.path().join("dump.sql");
        fs::set_permissions(&dump, fs::Permissions::from_mode(0o000)).unwrap();
        // A process that can read the file anyway cannot show that it was not read.
        if fs::read(&dump).is_ok() {
            return;
        }
        let dump_id = |limit: u64| {
            let states = scan_limited(dir.path(), &[], limit);
            let state = states
                .iter()
                .find(|state| state.path.as_str() == "dump.sql")
                .unwrap();
            state.stamp.id
        };

        let (size, secs, nanos) = stat_of(&fs::metadata(&dump).unwrap());
        assert_eq!(dump_id(4), ContentId::from_stat(size, secs, nanos));
        // Under the limit the file is read for its id, and this one cannot be.
        assert_eq!(dump_id(100), ContentId::UNKNOWN);
    }

    #[test]
    fn a_stamp_is_settled_two_seconds_before_the_listing() {
        let stamp = |mtime_secs| Stamp {
            id: blob_id(b"content"),
            size: 7,
            mtime_secs,
            mtime_nanos: 0,
        };

        assert!(stamp(97).is_settled(100));
        assert!(!stamp(98).is_settled(100));
        assert!(!stamp(100).is_settled(100));
        assert!(!stamp(101).is_settled(100));
        assert!(!stamp(i64::MAX).is_settled(i64::MIN));
    }

    #[test]
    fn a_file_the_index_does_not_vouch_for_keeps_its_stored_id() {
        let dir = init_repo(&[("tracked.ts", "tracked")]);
        write(dir.path(), "tracked.ts", "edited");
        write(dir.path(), "new.ts", "new");
        let workspace =
            Workspace::locate_with(Utf8Path::from_path(dir.path()).unwrap(), None).unwrap();
        let first = scan(&workspace, &[], u64::MAX, &|_| None).unwrap().files;
        assert_eq!(paths(&first), ["new.ts", "tracked.ts"]);
        let stored = |path: &RepoPath| {
            let stamp = first.iter().find(|state| &state.path == path)?.stamp;
            Some(Stamp {
                id: blob_id(path.as_str().as_bytes()),
                ..stamp
            })
        };

        let second = scan(&workspace, &[], u64::MAX, &stored).unwrap().files;

        // The ids are the ones the previous run stored, not those of the contents.
        for state in &second {
            assert_eq!(state.stamp.id, blob_id(state.path.as_str().as_bytes()));
        }
    }

    #[test]
    fn a_clean_tracked_file_takes_its_id_from_the_index() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q", "-b", "main"]);
        write(dir.path(), "old.ts", "old");
        write(dir.path(), "future.ts", "future");
        // The index can tell a file from its stat only when the file is older than the index: one written in the
        // second `git add` runs in, or after it, may have been saved again within that second.
        let now = SystemTime::now();
        touch(&dir.path().join("old.ts"), now - Duration::from_secs(3_600));
        touch(
            &dir.path().join("future.ts"),
            now + Duration::from_secs(3_600),
        );
        git(dir.path(), &["add", "-A"]);
        git(dir.path(), &["commit", "-q", "-m", "initial"]);
        let workspace =
            Workspace::locate_with(Utf8Path::from_path(dir.path()).unwrap(), None).unwrap();
        let first = scan(&workspace, &[], u64::MAX, &|_| None).unwrap().files;
        // Stored stamps that fit both files and carry an id that is not the one of the content.
        let stored = |path: &RepoPath| {
            let stamp = first.iter().find(|state| &state.path == path)?.stamp;
            Some(Stamp {
                id: blob_id(b"stored"),
                ..stamp
            })
        };

        let second = scan(&workspace, &[], u64::MAX, &stored).unwrap().files;

        let id_of = |name: &str| {
            let state = second.iter().find(|state| state.path.as_str() == name);
            state.unwrap().stamp.id
        };
        assert_eq!(id_of("old.ts"), blob_id(b"old"));
        assert_eq!(id_of("future.ts"), blob_id(b"stored"));
    }

    #[test]
    fn a_stamp_without_an_id_does_not_stand_for_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        write(dir.path(), "a.txt", "content");
        let workspace = Workspace::locate_with(
            Utf8Path::from_path(dir.path()).unwrap(),
            Some(Utf8PathBuf::from_path_buf(cache.path().to_path_buf()).unwrap()),
        )
        .unwrap();
        let first = scan(&workspace, &[], u64::MAX, &|_| None).unwrap().files;
        let unknown = Stamp {
            id: ContentId::UNKNOWN,
            ..first[0].stamp
        };

        let second = scan(&workspace, &[], u64::MAX, &|_| Some(unknown))
            .unwrap()
            .files;

        assert_eq!(second[0].stamp.id, blob_id(b"content"));
    }

    /// A file with `text`, and its metadata.
    fn stamped(dir: &Path, text: &str) -> (Utf8PathBuf, Metadata, Stamp) {
        write(dir, "a.txt", text);
        let path = Utf8PathBuf::from_path_buf(dir.join("a.txt")).unwrap();
        let meta = fs::metadata(&path).unwrap();
        let stamp = Stamp::of(blob_id(text.as_bytes()), &meta);
        (path, meta, stamp)
    }

    #[test]
    fn a_file_still_as_stamped_keeps_its_stamp() {
        let dir = tempfile::tempdir().unwrap();
        let (path, meta, stamp) = stamped(dir.path(), "one");

        assert_eq!(stamp.restamp(&path, &meta, b"one"), None);
    }

    #[test]
    fn a_file_saved_since_it_was_stamped_gets_the_id_of_the_bytes_read() {
        let dir = tempfile::tempdir().unwrap();
        let (path, _, stamp) = stamped(dir.path(), "one");
        write(dir.path(), "a.txt", "one, saved");
        let before = fs::metadata(&path).unwrap();

        let restamped = stamp.restamp(&path, &before, b"one, saved").unwrap();

        assert_eq!(restamped.id, blob_id(b"one, saved"));
        assert_eq!(restamped.size, 10);
    }

    #[test]
    fn a_file_saved_while_it_was_read_has_no_id() {
        let dir = tempfile::tempdir().unwrap();
        let (path, before, stamp) = stamped(dir.path(), "one");
        write(dir.path(), "a.txt", "one, saved meanwhile");

        let restamped = stamp.restamp(&path, &before, b"one").unwrap();

        assert_eq!(restamped.id, ContentId::UNKNOWN);
        assert_eq!(restamped.size, 20);
    }

    #[test]
    fn a_file_removed_while_it_was_read_has_no_id() {
        let dir = tempfile::tempdir().unwrap();
        let (path, before, stamp) = stamped(dir.path(), "one");
        fs::remove_file(&path).unwrap();

        let restamped = stamp.restamp(&path, &before, b"one").unwrap();

        assert_eq!(restamped.id, ContentId::UNKNOWN);
    }
}
