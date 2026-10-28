//! Where a run happens: the work tree root, the git directories and the directory the engine keeps its state in.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use camino::{Utf8Path, Utf8PathBuf};
use xxhash_rust::xxh3::xxh3_64;

/// The name of the snapshot file inside a state directory.
const SNAPSHOT_FILE: &str = "snapshot.gsttt";

/// The git directories of a work tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitDirs {
    /// The directory of this work tree's own git data: `.git` of a main work tree, `<common dir>/worktrees/<name>` of a linked one.
    pub git_dir: Utf8PathBuf,
    /// The directory every work tree of the repository shares: `.git` of the main work tree.
    pub common_dir: Utf8PathBuf,
}

/// A work tree and the directory where the engine keeps what it learned about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    root: Utf8PathBuf,
    git: Option<GitDirs>,
    state_dir: Utf8PathBuf,
}

/// Why a directory is not a workspace.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error(
        "`{path}` belongs to a bare repository, which has no work tree to index: run gsttt in a checkout"
    )]
    Bare { path: Utf8PathBuf },
    #[error("cannot resolve `{path}`: {message}")]
    Path { path: String, message: String },
    #[error("cannot open the git repository around `{path}`: {message}")]
    Git { path: Utf8PathBuf, message: String },
    #[error(
        "`{path}` is not in a git repository and neither XDG_CACHE_HOME nor HOME is set, so there is nowhere to keep its state"
    )]
    NoCacheHome { path: Utf8PathBuf },
}

impl Workspace {
    /// Finds the workspace around `start`: the work tree it is in, or `start` itself when no repository encloses it, in which case the state goes under `$XDG_CACHE_HOME` (`~/.cache` when unset).
    pub fn locate(start: &Utf8Path) -> Result<Self, WorkspaceError> {
        Self::locate_with(start, UserDir::Cache.locate())
    }

    /// [`Workspace::locate`] with the cache home given instead of read from the environment.
    pub fn locate_with(
        start: &Utf8Path,
        cache_home: Option<Utf8PathBuf>,
    ) -> Result<Self, WorkspaceError> {
        let start = canonical(start.as_std_path())?;
        match gix::discover(start.as_std_path()) {
            Ok(repo) => Self::in_repo(&repo),
            // `discover` does not tell "no repository here" from a broken one, so a `.git` above the start decides.
            Err(error) if start.ancestors().any(|dir| dir.join(".git").exists()) => {
                Err(WorkspaceError::Git {
                    path: start,
                    message: error.to_string(),
                })
            }
            Err(_) => {
                let cache_home = cache_home.ok_or_else(|| WorkspaceError::NoCacheHome {
                    path: start.clone(),
                })?;
                let hash = xxh3_64(start.as_str().as_bytes());
                let state_dir = cache_home.join("gsttt").join(format!("{hash:016x}"));
                Ok(Self {
                    root: start,
                    git: None,
                    state_dir,
                })
            }
        }
    }

    fn in_repo(repo: &gix::Repository) -> Result<Self, WorkspaceError> {
        let git_dir = canonical(repo.git_dir())?;
        let Some(workdir) = repo.workdir().filter(|_| !repo.is_bare()) else {
            return Err(WorkspaceError::Bare { path: git_dir });
        };
        let root = canonical(workdir)?;
        let common_dir = canonical(repo.common_dir())?;
        let state_dir = git_dir.join("gsttt");
        Ok(Self {
            root,
            git: Some(GitDirs {
                git_dir,
                common_dir,
            }),
            state_dir,
        })
    }

    /// The top directory of the work tree.
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    /// The git directories, `None` outside git.
    pub fn git(&self) -> Option<&GitDirs> {
        self.git.as_ref()
    }

    /// Where the engine keeps its state for this work tree.
    pub fn state_dir(&self) -> &Utf8Path {
        &self.state_dir
    }

    /// The snapshot of this work tree.
    pub fn snapshot_path(&self) -> Utf8PathBuf {
        self.state_dir.join(SNAPSHOT_FILE)
    }

    /// The snapshots of the other work trees of the same repository that exist, in path order: the main work tree's and every linked one's.
    pub fn sibling_snapshots(&self) -> Vec<Utf8PathBuf> {
        let Some(git) = &self.git else {
            return Vec::new();
        };
        let own = self.snapshot_path();
        let linked = git.common_dir.join("worktrees");
        let mut found = vec![git.common_dir.join("gsttt").join(SNAPSHOT_FILE)];
        if let Ok(entries) = fs::read_dir(&linked) {
            found.extend(
                entries
                    .flatten()
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .map(|name| linked.join(name).join("gsttt").join(SNAPSHOT_FILE)),
            );
        }
        found.retain(|path| *path != own && path.is_file());
        found.sort();
        found
    }
}

/// The absolute path with symbolic links resolved, so two spellings of one directory are one workspace.
fn canonical(path: &Path) -> Result<Utf8PathBuf, WorkspaceError> {
    let fail = |message: String| WorkspaceError::Path {
        path: path.display().to_string(),
        message,
    };
    let resolved = path
        .canonicalize()
        .map_err(|error| fail(error.to_string()))?;
    Utf8PathBuf::from_path_buf(resolved).map_err(|_| fail("the path is not valid UTF-8".to_owned()))
}

/// A folder of the user's home where a program keeps what it writes, by the XDG base directory rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserDir {
    /// What can be made again, such as the state of a directory outside git: `$XDG_CACHE_HOME`, or `$HOME/.cache` when that is unset or empty.
    Cache,
    /// What a person decided and the program must not lose, such as the trust in a connector: `$XDG_STATE_HOME`, or `$HOME/.local/state` when that is unset or empty.
    State,
}

impl UserDir {
    /// The folder as the environment of this process names it, `None` when neither its variable nor `HOME` is set to a path.
    pub fn locate(self) -> Option<Utf8PathBuf> {
        let (variable, from_home) = match self {
            Self::Cache => ("XDG_CACHE_HOME", ".cache"),
            Self::State => ("XDG_STATE_HOME", ".local/state"),
        };
        user_dir(env::var_os(variable), env::var_os("HOME"), from_home)
    }
}

/// `xdg`, or `home` joined with `from_home` when `xdg` is unset or empty.
fn user_dir(xdg: Option<OsString>, home: Option<OsString>, from_home: &str) -> Option<Utf8PathBuf> {
    let usable = |value: Option<OsString>| {
        value
            .filter(|value| !value.is_empty())
            .and_then(|value| Utf8PathBuf::from_path_buf(PathBuf::from(value)).ok())
    };
    usable(xdg).or_else(|| usable(home).map(|home| home.join(from_home)))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;
    use crate::engine::testing::{git, in_environment, init_repo};

    fn utf8(path: &Path) -> &Utf8Path {
        Utf8Path::from_path(path).unwrap()
    }

    /// The child side of [`locate_in`]: prints what `Workspace::locate` makes of the directory in `GSTTT_TEST_START`, in the environment the parent chose.
    #[test]
    #[ignore = "runs in a child process of the tests that set the environment"]
    fn locate_for_the_parent_process() {
        let Some(start) = env::var_os("GSTTT_TEST_START") else {
            return;
        };
        let start = Utf8PathBuf::from_path_buf(PathBuf::from(start)).unwrap();
        // The harness has not ended its own line yet.
        println!();
        match Workspace::locate(&start) {
            Ok(workspace) => {
                println!("root={}", workspace.root());
                println!("git={}", workspace.git().is_some());
                println!("state_dir={}", workspace.state_dir());
            }
            Err(error) => println!("error={error}"),
        }
    }

    /// What `Workspace::locate` prints for `start` in a process whose environment has `set` added and `remove` taken out, as `name=value` lines.
    fn locate_in(start: &Path, set: &[(&str, &str)], remove: &[&str]) -> String {
        let mut set: Vec<(&str, &OsStr)> = set
            .iter()
            .map(|&(key, value)| (key, OsStr::new(value)))
            .collect();
        set.push(("GSTTT_TEST_START", start.as_os_str()));
        in_environment(
            "engine::workspace::tests::locate_for_the_parent_process",
            &set,
            remove,
        )
    }

    /// The value printed under `name`.
    fn printed<'a>(output: &'a str, name: &str) -> &'a str {
        let prefix = format!("{name}=");
        output
            .lines()
            .find_map(|line| line.strip_prefix(prefix.as_str()))
            .unwrap_or_else(|| panic!("no `{name}` in {output}"))
    }

    /// The state dir `Workspace::locate` is meant to give a directory outside git: `cache_home/gsttt/<hash of the root>`.
    fn state_dir_under(cache_home: &str, dir: &Path) -> String {
        let root = canonical(dir).unwrap();
        let hash = format!("{:016x}", xxh3_64(root.as_str().as_bytes()));
        format!("{cache_home}/gsttt/{hash}")
    }

    #[gsttt_scenario::scenario("indexing", "Linked worktree has its own state dir")]
    #[test]
    fn a_linked_worktree_keeps_its_state_under_its_own_git_dir() {
        let main = init_repo(&[("a.txt", "a")]);
        let elsewhere = tempfile::tempdir().unwrap();
        let linked = elsewhere.path().join("feature");
        git(
            main.path(),
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feature",
                linked.to_str().unwrap(),
            ],
        );
        let subdirectory = linked.join("sub");
        fs::create_dir_all(&subdirectory).unwrap();

        let workspace = Workspace::locate_with(utf8(&subdirectory), None).unwrap();

        let common = canonical(&main.path().join(".git")).unwrap();
        assert_eq!(workspace.root(), canonical(&linked).unwrap());
        assert_eq!(
            workspace.state_dir(),
            common.join("worktrees").join("feature").join("gsttt")
        );
        assert_eq!(workspace.git().unwrap().common_dir, common);
    }

    #[gsttt_scenario::scenario("indexing", "Directory outside git uses the cache home")]
    #[test]
    fn a_directory_outside_git_keeps_its_state_in_the_cache_home() {
        let dir = tempfile::tempdir().unwrap();

        // HOME is set as well: the cache home wins over it.
        let output = locate_in(dir.path(), &[("XDG_CACHE_HOME", "/c"), ("HOME", "/h")], &[]);

        assert_eq!(printed(&output, "root"), canonical(dir.path()).unwrap());
        assert_eq!(
            printed(&output, "state_dir"),
            state_dir_under("/c", dir.path())
        );
        assert_eq!(printed(&output, "git"), "false");
    }

    #[test]
    fn home_keeps_the_state_when_the_cache_home_is_unset() {
        let dir = tempfile::tempdir().unwrap();
        let output = locate_in(dir.path(), &[("HOME", "/h")], &["XDG_CACHE_HOME"]);
        assert_eq!(
            printed(&output, "state_dir"),
            state_dir_under("/h/.cache", dir.path())
        );
    }

    #[test]
    fn a_directory_outside_git_needs_a_cache_home_or_home() {
        let dir = tempfile::tempdir().unwrap();
        let output = locate_in(dir.path(), &[], &["XDG_CACHE_HOME", "HOME"]);
        assert!(
            printed(&output, "error").contains("neither XDG_CACHE_HOME nor HOME is set"),
            "{output}"
        );
    }

    #[test]
    fn a_user_folder_falls_back_to_home_when_the_variable_is_unset_or_empty() {
        let os = |text: &str| Some(OsString::from(text));
        for (from_home, fallback) in [(".cache", "/h/.cache"), (".local/state", "/h/.local/state")]
        {
            assert_eq!(user_dir(os("/x"), os("/h"), from_home), Some("/x".into()));
            assert_eq!(user_dir(None, os("/h"), from_home), Some(fallback.into()));
            assert_eq!(user_dir(os(""), os("/h"), from_home), Some(fallback.into()));
            assert_eq!(user_dir(None, None, from_home), None);
        }
    }

    #[test]
    fn a_bare_repository_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q", "--bare"]);
        let error = Workspace::locate_with(utf8(dir.path()), None).unwrap_err();
        assert!(matches!(error, WorkspaceError::Bare { .. }), "{error}");
    }

    #[test]
    fn the_main_work_tree_keeps_state_in_its_git_dir() {
        let main = init_repo(&[("a.txt", "a")]);
        let workspace = Workspace::locate_with(utf8(main.path()), None).unwrap();
        let git_dir = canonical(&main.path().join(".git")).unwrap();
        assert_eq!(workspace.state_dir(), git_dir.join("gsttt"));
        assert_eq!(
            workspace.snapshot_path(),
            git_dir.join("gsttt").join("snapshot.gsttt")
        );
    }
}
