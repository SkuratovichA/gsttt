//! Paths inside the repository.

use std::borrow::Borrow;
use std::fmt;

use camino::{Utf8Path, Utf8PathBuf};
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

/// Why a path argument is not a path inside the repository.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OutsideRoot {
    /// The argument, read against its base, leaves the repository.
    #[error("`{argument}` is outside the repository `{root}`")]
    Leaves {
        /// The argument as it was given.
        argument: String,
        /// The root of the repository.
        root: Utf8PathBuf,
    },
    /// The argument is the root of the repository, which is a directory and no `RepoPath`.
    #[error("`{argument}` is the root of the repository, not a path inside it")]
    IsRoot {
        /// The argument as it was given.
        argument: String,
    },
}

/// A path argument, read against a directory inside the repository: the directory the command ran in for the command line, the root itself for the MCP server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// The path from the root.
    pub path: RepoPath,
    /// Whether anything is at that path. A path with nothing there is asked about as deleted, so that an answer can still list what used to import it.
    pub on_disk: bool,
}

/// What [`fold`] made of some segments.
struct Folded<'a> {
    /// The segments left, none of them empty, `.` or `..`.
    segments: Vec<&'a str>,
    /// Whether a `..` had no segment before it to remove.
    escaped: bool,
}

/// Drops the `.` and empty `segments` and lets each `..` remove the segment before it, by their text alone.
fn fold<'a>(segments: impl Iterator<Item = &'a str>) -> Folded<'a> {
    let mut folded = Folded {
        segments: Vec::new(),
        escaped: false,
    };
    for segment in segments {
        match segment {
            "" | "." => {}
            ".." => folded.escaped |= folded.segments.pop().is_none(),
            segment => folded.segments.push(segment),
        }
    }
    folded
}

impl RepoPath {
    /// Normalises `raw`: drops `.` and empty segments and lets `..` remove the segment before it.
    pub fn normalize(raw: &str) -> Result<Self, PathError> {
        if raw.starts_with('/') {
            return Err(PathError::Absolute(raw.to_owned()));
        }
        let Folded { segments, escaped } = fold(raw.split('/'));
        if escaped {
            return Err(PathError::EscapesRoot(raw.to_owned()));
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

    /// `argument` read against `base`, a directory inside `root`, as a path from `root`. An absolute `argument`
    /// stands on its own.
    ///
    /// The reading is by text alone: `.` is dropped and `..` removes the segment before it, and the disk is never
    /// touched, so a file that is gone can still be named. Links are not followed, which is why `root` and `base`
    /// should be canonical paths, and an absolute `argument` has to spell the root the way `root` does.
    ///
    /// # Errors
    ///
    /// When the argument is not under `root`, or is `root` itself.
    pub fn from_argument(
        root: &Utf8Path,
        base: &Utf8Path,
        argument: &str,
    ) -> Result<Self, OutsideRoot> {
        // A `..` above the top of an absolute path stays at the top, as it does on disk.
        let wanted = if argument.starts_with('/') {
            fold(argument.split('/'))
        } else {
            fold(base.as_str().split('/').chain(argument.split('/')))
        };
        let root_segments = fold(root.as_str().split('/')).segments;
        match wanted.segments.strip_prefix(root_segments.as_slice()) {
            None => Err(OutsideRoot::Leaves {
                argument: argument.to_owned(),
                root: root.to_owned(),
            }),
            Some([]) => Err(OutsideRoot::IsRoot {
                argument: argument.to_owned(),
            }),
            Some(inside) => Ok(Self(inside.join("/"))),
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
    use camino::Utf8Component;
    use proptest::prelude::*;

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

    proptest! {
        #[test]
        fn joins_never_panic(dir in "[a-z/.]{0,12}", relative in "[a-z/.]{0,12}") {
            drop(RepoPath::join_dir(&dir, &relative));
        }
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

    /// The repository the examples read paths in.
    const ROOT: &str = "/srv/repo";

    /// `argument` read against `base` in [`ROOT`].
    fn read(base: &str, argument: &str) -> Result<RepoPath, OutsideRoot> {
        RepoPath::from_argument(Utf8Path::new(ROOT), Utf8Path::new(base), argument)
    }

    #[test]
    fn a_relative_argument_is_read_from_its_base() {
        assert_eq!(
            read("/srv/repo/app", "../lib/util.ts").unwrap().as_str(),
            "lib/util.ts"
        );
        assert_eq!(
            read("/srv/repo", "./src//a/../b.ts").unwrap().as_str(),
            "src/b.ts"
        );
        assert_eq!(read("/srv/repo/app", "x.ts").unwrap().as_str(), "app/x.ts");
    }

    #[test]
    fn an_absolute_argument_inside_the_root_is_accepted() {
        assert_eq!(
            read("/srv/repo/app", "/srv/repo/lib/util.ts")
                .unwrap()
                .as_str(),
            "lib/util.ts"
        );
        assert_eq!(
            read("/elsewhere", "/srv/repo/./a/../b.ts")
                .unwrap()
                .as_str(),
            "b.ts"
        );
    }

    #[test]
    fn an_argument_that_leaves_the_root_is_refused() {
        let leaves = |argument: &str| OutsideRoot::Leaves {
            argument: argument.to_owned(),
            root: ROOT.into(),
        };
        for (base, argument) in [
            ("/srv/repo/app", "../../elsewhere.ts"),
            ("/srv/repo", "../repo-two/a.ts"),
            ("/srv/repo/app", "/srv/repo-two/a.ts"),
            ("/srv/repo", "/etc/passwd"),
            ("/srv/repo", "../../../../../../x"),
        ] {
            assert_eq!(read(base, argument), Err(leaves(argument)), "{argument}");
        }
        assert_eq!(
            leaves("../../elsewhere.ts").to_string(),
            "`../../elsewhere.ts` is outside the repository `/srv/repo`"
        );
    }

    #[test]
    fn an_argument_that_goes_out_and_comes_back_is_inside() {
        assert_eq!(
            read("/srv/repo/app", "../../repo/lib/a.ts")
                .unwrap()
                .as_str(),
            "lib/a.ts"
        );
    }

    #[test]
    fn the_root_itself_is_no_path() {
        for (base, argument) in [
            ("/srv/repo", "."),
            ("/srv/repo", ""),
            ("/srv/repo/app", ".."),
            ("/srv/repo/app", "/srv/repo/"),
        ] {
            assert_eq!(
                read(base, argument),
                Err(OutsideRoot::IsRoot {
                    argument: argument.to_owned()
                }),
                "{argument:?}"
            );
        }
        assert_eq!(
            OutsideRoot::IsRoot {
                argument: ".".to_owned()
            }
            .to_string(),
            "`.` is the root of the repository, not a path inside it"
        );
    }

    #[test]
    fn a_file_that_is_gone_can_still_be_named() {
        let read = |argument: &str| {
            RepoPath::from_argument(
                Utf8Path::new("/no/such/repo"),
                Utf8Path::new("/no/such/repo/app"),
                argument,
            )
        };
        assert_eq!(read("../gone.ts").unwrap().as_str(), "gone.ts");
    }

    #[test]
    fn a_parent_above_the_top_of_the_disk_stays_at_the_top() {
        let from_the_top = |argument: &str| {
            RepoPath::from_argument(Utf8Path::new("/"), Utf8Path::new("/"), argument)
        };
        assert_eq!(from_the_top("../x").unwrap().as_str(), "x");
        assert_eq!(from_the_top("/../../x").unwrap().as_str(), "x");
    }

    /// Up to `longest` segments of the kinds an argument is made of: names, the current and the parent
    /// directory, and the empty segment a doubled slash leaves. `srv` and `repo` let a path come back into the
    /// root after it left, and `repo2` is a neighbour whose name starts with the root's.
    fn segments(longest: usize) -> impl Strategy<Value = Vec<&'static str>> {
        prop::collection::vec(
            prop_oneof![
                Just("a"),
                Just("b"),
                Just("srv"),
                Just("repo"),
                Just("repo2"),
                Just("."),
                Just(".."),
                Just(".."),
                Just(""),
            ],
            0..=longest,
        )
    }

    /// An argument that is relative, absolute inside the root or absolute anywhere.
    fn argument() -> impl Strategy<Value = String> {
        (
            prop_oneof![Just(""), Just("/"), Just("/srv/repo/")],
            segments(6),
        )
            .prop_map(|(start, segments)| format!("{start}{}", segments.join("/")))
    }

    /// A directory inside the root, up to three levels down.
    fn base() -> impl Strategy<Value = Utf8PathBuf> {
        prop::collection::vec(prop_oneof![Just("a"), Just("b")], 0..=3).prop_map(|dirs| {
            dirs.iter()
                .fold(Utf8PathBuf::from(ROOT), |base, dir| base.join(dir))
        })
    }

    /// What `argument` names read against `base`, found another way than the code under test finds it: with the
    /// standard parser of components and a stack. `None` when it is not a path inside `root`.
    fn model(root: &Utf8Path, base: &Utf8Path, argument: &str) -> Option<String> {
        fn resolve(path: &Utf8Path) -> Vec<String> {
            let mut stack = Vec::new();
            for component in path.components() {
                match component {
                    Utf8Component::Normal(name) => stack.push(name.to_owned()),
                    Utf8Component::ParentDir => {
                        stack.pop();
                    }
                    Utf8Component::Prefix(_) | Utf8Component::RootDir | Utf8Component::CurDir => {}
                }
            }
            stack
        }
        let wanted = Utf8Path::new(argument);
        let full = if wanted.is_absolute() {
            wanted.to_owned()
        } else {
            base.join(wanted)
        };
        let root = resolve(root);
        let inside = resolve(&full).strip_prefix(root.as_slice())?.to_vec();
        (!inside.is_empty()).then(|| inside.join("/"))
    }

    proptest! {
        #[test]
        fn an_argument_never_reads_as_a_path_with_dots_or_outside_the_root(
            base in base(),
            argument in argument(),
        ) {
            let root = Utf8Path::new(ROOT);
            let read = RepoPath::from_argument(root, &base, &argument);
            let expected = model(root, &base, &argument);
            prop_assert_eq!(read.as_ref().ok().map(RepoPath::as_str), expected.as_deref());
            if let Ok(path) = &read {
                prop_assert!(
                    path.as_str().split('/').all(|segment| !matches!(segment, "" | "." | "..")),
                    "{:?}",
                    path
                );
            }
        }

        #[test]
        fn the_absolute_spelling_of_a_result_reads_back_as_itself(
            base in base(),
            argument in argument(),
        ) {
            let root = Utf8Path::new(ROOT);
            if let Ok(path) = RepoPath::from_argument(root, &base, &argument) {
                let absolute = root.join(path.as_str());
                prop_assert_eq!(RepoPath::from_argument(root, root, absolute.as_str()), Ok(path));
            }
        }

        #[test]
        fn reading_from_the_root_agrees_with_normalize(raw in segments(6).prop_map(|s| s.join("/"))) {
            if let Ok(path) = RepoPath::normalize(&raw) {
                let root = Utf8Path::new(ROOT);
                prop_assert_eq!(RepoPath::from_argument(root, root, &raw), Ok(path));
            }
        }

        #[test]
        fn a_normalized_path_has_no_dots_and_normalizes_to_itself(raw in segments(6).prop_map(|s| s.join("/"))) {
            if let Ok(path) = RepoPath::normalize(&raw) {
                prop_assert!(
                    path.as_str().split('/').all(|segment| !matches!(segment, "" | "." | "..")),
                    "{:?}",
                    path
                );
                prop_assert_eq!(RepoPath::normalize(path.as_str()), Ok(path));
            }
        }
    }
}
