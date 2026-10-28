//! Paths inside the repository.

use std::borrow::Borrow;
use std::fmt;

use serde::{Deserialize, Serialize};

/// A file path relative to the repository root: `/`-separated, with no `.`, `..` or empty segments. It is the
/// form git stores, so two spellings of one file are one `RepoPath`.
#[derive(
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
#[rkyv(derive(PartialEq, Eq, PartialOrd, Ord, Hash))]
pub struct RepoPath(String);

/// Why a string is not a [`RepoPath`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("empty path")]
    Empty,
    #[error("`{0}` is absolute; a repository path is relative to the repository root")]
    Absolute(String),
    #[error("`{0}` leaves the repository")]
    EscapesRoot(String),
}

impl RepoPath {
    /// Normalises `raw`: drops `.` and empty segments and lets `..` remove the segment before it.
    pub fn normalize(raw: &str) -> Result<Self, PathError> {
        if raw.starts_with('/') {
            return Err(PathError::Absolute(raw.to_owned()));
        }
        let mut segments: Vec<&str> = Vec::new();
        for segment in raw.split('/') {
            match segment {
                "" | "." => {}
                ".." => {
                    if segments.pop().is_none() {
                        return Err(PathError::EscapesRoot(raw.to_owned()));
                    }
                }
                segment => segments.push(segment),
            }
        }
        if segments.is_empty() {
            return Err(PathError::Empty);
        }
        Ok(Self(segments.join("/")))
    }

    /// `relative` taken from the directory `dir`, where `""` is the repository root.
    pub fn join(dir: &str, relative: &str) -> Result<Self, PathError> {
        if relative.starts_with('/') {
            return Err(PathError::Absolute(relative.to_owned()));
        }
        if dir.is_empty() {
            Self::normalize(relative)
        } else {
            Self::normalize(&format!("{dir}/{relative}"))
        }
    }

    /// [`RepoPath::join`] for a directory: `""` is the repository root in `dir` and in the result. A `RepoPath` names
    /// a file, so `join` fails with [`PathError::Empty`] for the root, which is what `.` from the root and a `..`
    /// that climbs back to it come to; this returns `Some("")` for it. `None` when `relative` is absolute or leaves
    /// the repository.
    pub fn join_dir(dir: &str, relative: &str) -> Option<String> {
        match Self::join(dir, relative) {
            Ok(path) => Some(path.0),
            Err(PathError::Empty) => Some(String::new()),
            Err(PathError::Absolute(_) | PathError::EscapesRoot(_)) => None,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The last segment: `b.ts` for `src/a/b.ts`.
    pub fn file_name(&self) -> &str {
        self.0
            .rsplit_once('/')
            .map_or(self.0.as_str(), |(_, name)| name)
    }

    /// The directory holding the file, `""` at the root.
    pub fn parent(&self) -> &str {
        self.0.rsplit_once('/').map_or("", |(dir, _)| dir)
    }

    /// The text after the last `.` of the file name: `ts` for `a.d.ts`, `None` for `Makefile` and `.env`.
    pub fn extension(&self) -> Option<&str> {
        let name = self.file_name();
        match name.rsplit_once('.') {
            Some((stem, extension)) if !stem.is_empty() => Some(extension),
            _ => None,
        }
    }

    /// The directories the file lies under, the deepest first and the root `""` last: `a/b`, `a` and `""` for
    /// `a/b/c.ts`.
    pub fn directories(&self) -> impl Iterator<Item = &str> {
        std::iter::successors(Some(self.parent()), |dir| {
            (!dir.is_empty()).then(|| dir.rsplit_once('/').map_or("", |(parent, _)| parent))
        })
    }

    /// Whether the file lies under `dir`, at any depth. Every file lies under `""`.
    pub fn is_under(&self, dir: &str) -> bool {
        dir.is_empty()
            || self
                .0
                .strip_prefix(dir)
                .is_some_and(|rest| rest.starts_with('/'))
    }
}

impl TryFrom<String> for RepoPath {
    type Error = PathError;

    fn try_from(raw: String) -> Result<Self, PathError> {
        Self::normalize(&raw)
    }
}

impl TryFrom<&str> for RepoPath {
    type Error = PathError;

    fn try_from(raw: &str) -> Result<Self, PathError> {
        Self::normalize(raw)
    }
}

impl From<RepoPath> for String {
    fn from(path: RepoPath) -> Self {
        path.0
    }
}

impl ArchivedRepoPath {
    /// The path as text, read in place from an archive.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for RepoPath {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for RepoPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_drops_dots_and_pops_parents() {
        assert_eq!(
            RepoPath::normalize("./src//a/../b.ts").unwrap().as_str(),
            "src/b.ts"
        );
        assert_eq!(
            RepoPath::join("src/a", "../b/c.ts").unwrap().as_str(),
            "src/b/c.ts"
        );
        assert_eq!(RepoPath::join("", "./x.py").unwrap().as_str(), "x.py");
    }

    #[test]
    fn join_dir_names_the_root_that_join_cannot() {
        assert_eq!(RepoPath::join_dir("", "src").as_deref(), Some("src"));
        assert_eq!(RepoPath::join_dir("a/b", "../c").as_deref(), Some("a/c"));
        for (dir, relative) in [("", "."), ("", "./"), ("fuzz", ".."), ("a/b", "../..")] {
            assert_eq!(RepoPath::join(dir, relative), Err(PathError::Empty));
            assert_eq!(RepoPath::join_dir(dir, relative).as_deref(), Some(""));
        }
        assert_eq!(RepoPath::join_dir("a", "../..").as_deref(), None);
        assert_eq!(RepoPath::join_dir("", "..").as_deref(), None);
        assert_eq!(RepoPath::join_dir("a", "/b").as_deref(), None);
    }

    #[test]
    fn normalize_refuses_paths_outside_the_repository() {
        assert_eq!(
            RepoPath::normalize("../a"),
            Err(PathError::EscapesRoot("../a".into()))
        );
        assert_eq!(
            RepoPath::normalize("/etc/passwd"),
            Err(PathError::Absolute("/etc/passwd".into()))
        );
        assert_eq!(RepoPath::normalize("./"), Err(PathError::Empty));
    }

    #[test]
    fn parts() {
        let path = RepoPath::normalize("src/types/a.d.ts").unwrap();
        assert_eq!(
            (path.parent(), path.file_name(), path.extension()),
            ("src/types", "a.d.ts", Some("ts"))
        );
        assert_eq!(RepoPath::normalize(".env").unwrap().extension(), None);
        assert!(path.is_under("src") && path.is_under("") && !path.is_under("sr"));
    }

    #[test]
    fn directories_run_from_the_deepest_to_the_root() {
        let directories = |path: &str| -> Vec<String> {
            RepoPath::normalize(path)
                .unwrap()
                .directories()
                .map(str::to_owned)
                .collect()
        };
        assert_eq!(directories("a/b/c.ts"), ["a/b", "a", ""]);
        assert_eq!(directories("a/c.ts"), ["a", ""]);
        assert_eq!(directories("c.ts"), [""]);
    }
}
