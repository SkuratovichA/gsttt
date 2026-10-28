//! The root of a run: the directory a command works on, found from the current directory or from `-C`.

use std::io;

use camino::{Utf8Path, Utf8PathBuf};
use gsttt_core::engine::workspace::{Workspace, WorkspaceError};

/// Why a run has no root to work on.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum RootError {
    /// `-C` names something that is not a directory.
    #[error("`-C` names `{path}`, which {problem}")]
    #[diagnostic(help("name a directory, as in `gsttt -C path/to/project index`"))]
    Directory {
        /// The directory as it was typed.
        path: Utf8PathBuf,
        /// What is wrong with it, finishing the sentence "`-C` names `path`, which …".
        problem: String,
    },

    /// There is no usable current directory to start from.
    #[error("cannot read the current directory: {reason}")]
    CurrentDirectory {
        /// Why it cannot be read.
        reason: String,
    },

    /// The engine cannot make a workspace of the directory.
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),

    /// The root is the home directory, which holds every file the user owns.
    #[error("gsttt does not index the home directory (`{root}`)")]
    #[diagnostic(help("run gsttt inside a project, or name one with `-C <dir>`"))]
    Home {
        /// The home directory, by its canonical path.
        root: Utf8PathBuf,
    },

    /// The root is `/`, which holds every file on the machine.
    #[error("gsttt does not index `/`, which holds every file on the machine")]
    #[diagnostic(help("run gsttt inside a project, or name one with `-C <dir>`"))]
    FilesystemRoot,
}

/// The directory a run starts in, by its canonical path, so that a path argument can be read against it and
/// compared with the root: the directory `-C` names, or the current directory when there is no `-C`.
///
/// # Errors
///
/// When `-C` names something that is not a directory, or the current directory cannot be read.
pub fn start(directory: Option<&Utf8Path>) -> Result<Utf8PathBuf, RootError> {
    let Some(directory) = directory else {
        return current_directory();
    };
    let problem = match directory.metadata() {
        Ok(metadata) if metadata.is_dir() => match directory.canonicalize_utf8() {
            Ok(canonical) => return Ok(canonical),
            Err(error) => format!("cannot be read: {error}"),
        },
        Ok(_) => "is not a directory".to_owned(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => "does not exist".to_owned(),
        Err(error) => format!("cannot be read: {error}"),
    };
    Err(RootError::Directory {
        path: directory.to_owned(),
        problem,
    })
}

/// The workspace around `start`, unless its root is a directory gsttt must not index: the home directory, whose
/// files belong to the user, or `/`. Both are compared by canonical path, so a link to the home directory is the
/// home directory.
///
/// # Errors
///
/// When the engine cannot make a workspace of `start`, or the root is refused.
pub fn locate(start: &Utf8Path) -> Result<Workspace, RootError> {
    let workspace = Workspace::locate(start)?;
    refuse(workspace.root(), home().as_deref())?;
    Ok(workspace)
}

fn current_directory() -> Result<Utf8PathBuf, RootError> {
    let fail = |reason: String| RootError::CurrentDirectory { reason };
    let current = std::env::current_dir().map_err(|error| fail(error.to_string()))?;
    let current = Utf8PathBuf::from_path_buf(current)
        .map_err(|_| fail("its path is not valid UTF-8".to_owned()))?;
    current
        .canonicalize_utf8()
        .map_err(|error| fail(error.to_string()))
}

/// The home directory by its canonical path, `None` when there is none or it cannot be resolved.
fn home() -> Option<Utf8PathBuf> {
    let home = Utf8PathBuf::from_path_buf(std::env::home_dir()?).ok()?;
    home.canonicalize_utf8().ok()
}

/// Whether gsttt may work on `root`, both it and `home` being canonical.
fn refuse(root: &Utf8Path, home: Option<&Utf8Path>) -> Result<(), RootError> {
    if root.parent().is_none() {
        Err(RootError::FilesystemRoot)
    } else if home == Some(root) {
        Err(RootError::Home {
            root: root.to_owned(),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf8(path: &str) -> &Utf8Path {
        Utf8Path::new(path)
    }

    #[test]
    fn the_filesystem_root_is_refused() {
        assert!(matches!(
            refuse(utf8("/"), Some(utf8("/home/ann"))),
            Err(RootError::FilesystemRoot)
        ));
        assert!(matches!(
            refuse(utf8("/"), None),
            Err(RootError::FilesystemRoot)
        ));
    }

    #[test]
    fn the_home_directory_is_refused() {
        let Err(RootError::Home { root }) = refuse(utf8("/home/ann"), Some(utf8("/home/ann")))
        else {
            panic!("the home directory was accepted");
        };
        assert_eq!(root, "/home/ann");
    }

    #[test]
    fn a_directory_inside_or_beside_home_is_accepted() {
        for root in ["/home/ann/project", "/home/anna", "/home", "/srv/project"] {
            assert!(
                refuse(utf8(root), Some(utf8("/home/ann"))).is_ok(),
                "{root}"
            );
        }
        assert!(refuse(utf8("/home/ann"), None).is_ok());
    }

    #[test]
    fn a_start_that_does_not_exist_is_named() {
        let Err(RootError::Directory { path, problem }) = start(Some(utf8("does/not/exist")))
        else {
            panic!("a missing directory was accepted");
        };
        assert_eq!(path, "does/not/exist");
        assert_eq!(problem, "does not exist");
    }

    #[test]
    fn a_start_that_is_a_file_is_named() {
        let manifest = Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let Err(RootError::Directory { problem, .. }) = start(Some(&manifest)) else {
            panic!("a file was accepted");
        };
        assert_eq!(problem, "is not a directory");
    }

    #[test]
    fn the_start_is_canonical() {
        let here = start(None).unwrap();
        assert_eq!(start(Some(utf8("."))).unwrap(), here);
        assert_eq!(start(Some(&here.join("src").join(".."))).unwrap(), here);
    }
}
