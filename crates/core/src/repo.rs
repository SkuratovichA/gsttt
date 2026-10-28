//! The file set of one run, as connectors see it.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;
use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use foldhash::fast::RandomState;

use crate::ids::{FileId, Id};
use crate::path::RepoPath;

/// The files of the repository that the run indexes: tracked files and untracked ones git does not ignore. Cheap to clone, so a project keeps one to resolve imports against.
///
/// Resolution looks files up here instead of on disk. A file git ignores (a build output, `node_modules`) is not in the view, so an import of it resolves to a package or to [`UnresolvedReason::Ignored`] rather than into generated code.
///
/// [`UnresolvedReason::Ignored`]: crate::fact::UnresolvedReason::Ignored
#[derive(Clone, Debug)]
pub struct RepoView {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    root: Utf8PathBuf,
    files: Vec<RepoPath>,
    ids: HashMap<Box<str>, FileId, RandomState>,
    dirs: HashSet<Box<str>, RandomState>,
}

impl RepoView {
    /// A view of `files` under the directory `root`. Duplicates go, and ids follow path order.
    pub fn new(root: impl Into<Utf8PathBuf>, mut files: Vec<RepoPath>) -> Self {
        files.sort_unstable();
        files.dedup();
        let mut ids = HashMap::with_capacity_and_hasher(files.len(), RandomState::default());
        let mut dirs = HashSet::with_hasher(RandomState::default());
        dirs.insert(Box::from(""));
        for (index, path) in files.iter().enumerate() {
            ids.insert(Box::from(path.as_str()), FileId::from_index(index));
            let mut dir = path.parent();
            while !dir.is_empty() && dirs.insert(Box::from(dir)) {
                dir = dir.rsplit_once('/').map_or("", |(parent, _)| parent);
            }
        }
        Self {
            inner: Arc::new(Inner {
                root: root.into(),
                files,
                ids,
                dirs,
            }),
        }
    }

    /// A view of every file under `root` that the ignore files inside `root` leave in, dotfiles included: the file set of a fixture tree in tests. Ignore files above `root`, the global gitignore, `.git` and symbolic links play no part, so the same tree gives the same view on every machine. A directory the walk cannot read is an error: [`RepoView::scan_reporting`] goes on without it.
    pub fn scan(root: impl Into<Utf8PathBuf>) -> io::Result<Self> {
        let root = root.into();
        let (files, failed) = walk(&root);
        match failed.into_iter().next() {
            Some((_, error)) => Err(io::Error::other(error)),
            None => Ok(Self::new(root, files)),
        }
    }

    /// The same view as [`RepoView::scan`], for the file set outside git, with the directories the walk could not read. The walk goes on without such a directory, so the files under it may be missing from the view. Each directory comes as its path under `root`, which is `.` for `root` itself and for an error that names no path.
    pub fn scan_reporting(root: impl Into<Utf8PathBuf>) -> (Self, Vec<String>) {
        let root = root.into();
        let (files, failed) = walk(&root);
        let unreadable = failed.into_iter().map(|(directory, _)| directory).collect();
        (Self::new(root, files), unreadable)
    }

    /// The absolute path of the repository root.
    pub fn root(&self) -> &Utf8Path {
        &self.inner.root
    }

    pub fn len(&self) -> usize {
        self.inner.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.files.is_empty()
    }

    /// Every file with its id, in path order.
    pub fn files(&self) -> impl ExactSizeIterator<Item = (FileId, &RepoPath)> {
        self.inner
            .files
            .iter()
            .enumerate()
            .map(|(index, path)| (FileId::from_index(index), path))
    }

    pub fn path(&self, file: FileId) -> Option<&RepoPath> {
        self.inner.files.get(file.index())
    }

    /// The file at a normalised repository path.
    pub fn lookup(&self, path: &str) -> Option<FileId> {
        self.inner.ids.get(path).copied()
    }

    /// Whether some file of the view lies in `dir`, at any depth. `""` is the root.
    pub fn is_dir(&self, dir: &str) -> bool {
        self.inner.dirs.contains(dir)
    }

    /// The files under `dir` at any depth, in path order.
    pub fn under<'a>(
        &'a self,
        dir: &str,
    ) -> impl Iterator<Item = (FileId, &'a RepoPath)> + use<'a> {
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let start = self
            .inner
            .files
            .partition_point(|path| path.as_str() < prefix.as_str());
        self.inner
            .files
            .iter()
            .enumerate()
            .skip(start)
            .take_while(move |(_, path)| path.as_str().starts_with(prefix.as_str()))
            .map(|(index, path)| (FileId::from_index(index), path))
    }

    /// The files directly in `dir`.
    pub fn dir_files<'a>(
        &'a self,
        dir: &str,
    ) -> impl Iterator<Item = (FileId, &'a RepoPath)> + use<'a> {
        let depth = if dir.is_empty() { 0 } else { dir.len() + 1 };
        self.under(dir).filter(move |(_, path)| {
            path.as_str()
                .get(depth..)
                .is_some_and(|name| !name.contains('/'))
        })
    }

    /// The absolute path of a repository path.
    pub fn absolute(&self, path: &str) -> Utf8PathBuf {
        self.inner.root.join(path)
    }

    /// Reads a file of the working tree, in the view or not: a connector reads `tsconfig.json` or `Cargo.toml` this way.
    pub fn read_to_string(&self, path: &str) -> io::Result<String> {
        std::fs::read_to_string(self.absolute(path))
    }

    /// Whether `path` exists in the working tree, in the view or not. It tells an import of a git-ignored file from an import of nothing.
    pub fn exists_on_disk(&self, path: &str) -> bool {
        self.absolute(path).symlink_metadata().is_ok()
    }
}

/// The files under `root` that the walk finds, and each error that kept it from reading something, with the directory it is about as a path under `root` (`.` for `root` itself).
fn walk(root: &Utf8Path) -> (Vec<RepoPath>, Vec<(String, ignore::Error)>) {
    let mut files = Vec::new();
    let mut failed = Vec::new();
    let walk = ignore::WalkBuilder::new(root)
        .hidden(false)
        .parents(false)
        .git_global(false)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|entry| entry.file_name() != ".git")
        .build();
    for entry in walk {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                failed.push((failed_directory(root, &error), error));
                continue;
            }
        };
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .ok()
            .and_then(|path| path.to_str());
        if let Some(path) = relative.and_then(|relative| RepoPath::normalize(relative).ok()) {
            files.push(path);
        }
    }
    (files, failed)
}

/// The directory an error of the walk is about, as a path under `root`: `.` when the error names `root` itself, a path outside it or no path at all.
fn failed_directory(root: &Utf8Path, error: &ignore::Error) -> String {
    named_path(error)
        .and_then(|path| path.strip_prefix(root).ok())
        .and_then(|relative| relative.to_str())
        .and_then(|relative| RepoPath::normalize(relative).ok())
        .map_or_else(|| ".".to_owned(), String::from)
}

/// The path an error of the walk names, if it names one.
fn named_path(error: &ignore::Error) -> Option<&Path> {
    match error {
        ignore::Error::WithPath { path, .. } => Some(path),
        ignore::Error::WithDepth { err, .. } | ignore::Error::WithLineNumber { err, .. } => {
            named_path(err)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(names: &[&str]) -> RepoView {
        RepoView::new(
            "/repo",
            names
                .iter()
                .map(|name| RepoPath::normalize(name).unwrap())
                .collect(),
        )
    }

    #[test]
    fn lookups_and_directories() {
        let repo = view(&["src/b.ts", "src/a/x.ts", "README.md", "src/b.ts"]);
        assert_eq!(repo.len(), 3);
        assert_eq!(repo.lookup("src/b.ts").map(FileId::raw), Some(2));
        assert_eq!(repo.lookup("src/c.ts"), None);
        assert!(
            repo.is_dir("")
                && repo.is_dir("src")
                && repo.is_dir("src/a")
                && !repo.is_dir("src/b.ts")
        );
    }

    #[test]
    fn under_and_dir_files_respect_segment_boundaries() {
        let repo = view(&["a/b.ts", "a/b/c.ts", "a/c.ts", "ab/d.ts", "e.ts"]);
        let under: Vec<&str> = repo.under("a").map(|(_, path)| path.as_str()).collect();
        assert_eq!(under, ["a/b.ts", "a/b/c.ts", "a/c.ts"]);
        let direct: Vec<&str> = repo.dir_files("a").map(|(_, path)| path.as_str()).collect();
        assert_eq!(direct, ["a/b.ts", "a/c.ts"]);
        let root: Vec<&str> = repo.dir_files("").map(|(_, path)| path.as_str()).collect();
        assert_eq!(root, ["e.ts"]);
        assert_eq!(repo.under("").count(), 5);
    }

    #[test]
    fn scan_keeps_dotfiles_and_obeys_only_the_tree_own_ignore_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        for (path, text) in [
            (".gitignore", "dist/\n"),
            (".env.example", "API_URL=\n"),
            ("src/a.ts", ""),
            ("dist/a.js", ""),
            (".git/HEAD", ""),
        ] {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        let repo = RepoView::scan(root).unwrap();
        let paths: Vec<&str> = repo.files().map(|(_, path)| path.as_str()).collect();
        assert_eq!(paths, [".env.example", ".gitignore", "src/a.ts"]);
    }
}
