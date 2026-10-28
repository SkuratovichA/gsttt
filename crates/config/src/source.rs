//! Reading the files of a repository that configure gsttt: `config.toml` and the rule packs next to it.
//!
//! gsttt reads these files on every index, and the repository did not ask to be trusted. A symbolic link can point anywhere on the disk, a named pipe blocks whoever opens it until something writes to it, and a file can be as large as the disk. So a file is resolved first, must lie inside the repository and inside the folder it may come from, must be a regular file, and is read only up to a limit.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use rustix::fs::{Mode, OFlags};

/// The largest config or pack file gsttt reads, in bytes. A config is a few kilobytes: this stops a file that is not one.
pub(crate) const MAX_SOURCE_BYTES: u64 = 1024 * 1024;

/// Why a source file was not read. The reason only: the caller names the file and says what it was for.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SourceError {
    /// The file, once its symbolic links are resolved, is not inside the repository or not inside the folder it may come from. The text names which of the two it leaves.
    #[error("resolves outside {0}")]
    Outside(String),
    /// A named pipe, a device, a directory.
    #[error("is not a regular file")]
    NotRegular,
    #[error("is larger than {} MiB", MAX_SOURCE_BYTES / 1024 / 1024)]
    TooLarge,
    #[error("is not UTF-8 text")]
    NotText,
    #[error("{0}")]
    Io(#[from] io::Error),
}

/// The text of the file `relative` under `root`, or `None` when there is no such file, a dangling link included.
///
/// The file must lie, after the resolution of its links, inside `root` and, when `bound` is given, inside `bound`, a folder relative to `root`. The repository is checked first: a `bound` that is itself a link out of the repository would otherwise let every file of the folder it leads to through. It must be a regular file of at most [`MAX_SOURCE_BYTES`], valid UTF-8.
pub(crate) fn read_source(
    root: &Path,
    relative: &str,
    bound: Option<&str>,
) -> Result<Option<String>, SourceError> {
    let Some(real) = resolve(&root.join(relative))? else {
        return Ok(None);
    };
    let Some(repository) = resolve(root)? else {
        return Ok(None);
    };
    if !real.starts_with(&repository) {
        return Err(SourceError::Outside("the repository".to_owned()));
    }
    if let Some(bound) = bound {
        let Some(inside) = resolve(&root.join(bound))? else {
            return Ok(None);
        };
        if !real.starts_with(&inside) {
            return Err(SourceError::Outside(bound.to_owned()));
        }
    }
    // Opened without waiting for a writer, so that a named pipe does not block the open. What the file is gets decided on the descriptor, after the open, and a swap between a check and the open cannot change it.
    let descriptor = rustix::fs::open(
        &real,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    let file = File::from(descriptor);
    if !file.metadata()?.is_file() {
        return Err(SourceError::NotRegular);
    }
    // One byte more than the limit tells a file of exactly the limit from a longer one.
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_SOURCE_BYTES {
        return Err(SourceError::TooLarge);
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| SourceError::NotText)
}

/// `path` with its links resolved, or `None` when it does not exist.
fn resolve(path: &Path) -> io::Result<Option<PathBuf>> {
    match fs::canonicalize(path) {
        Ok(real) => Ok(Some(real)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    fn tree() -> (tempfile::TempDir, tempfile::TempDir) {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".gsttt")).unwrap();
        (root, outside)
    }

    #[test]
    fn a_bound_that_leads_out_of_the_repository_lets_nothing_through() {
        let (root, outside) = tree();
        fs::write(outside.path().join("x.toml"), "name = \"x\"\n").unwrap();
        symlink(outside.path(), root.path().join(".gsttt/packs")).unwrap();
        let error =
            read_source(root.path(), ".gsttt/packs/x.toml", Some(".gsttt/packs")).unwrap_err();
        assert_eq!(error.to_string(), "resolves outside the repository");
    }

    #[test]
    fn a_file_inside_the_repository_and_outside_the_bound_says_which_it_leaves() {
        let (root, _outside) = tree();
        fs::create_dir_all(root.path().join(".gsttt/packs")).unwrap();
        fs::write(root.path().join(".gsttt/config.toml"), "").unwrap();
        symlink(
            root.path().join(".gsttt/config.toml"),
            root.path().join(".gsttt/packs/x.toml"),
        )
        .unwrap();
        let error =
            read_source(root.path(), ".gsttt/packs/x.toml", Some(".gsttt/packs")).unwrap_err();
        assert_eq!(error.to_string(), "resolves outside .gsttt/packs");
        let read = read_source(root.path(), ".gsttt/config.toml", None).unwrap();
        assert_eq!(read.as_deref(), Some(""));
    }

    #[test]
    fn a_missing_file_is_none_whatever_the_bound() {
        let (root, _outside) = tree();
        let missing = read_source(root.path(), ".gsttt/packs/x.toml", Some(".gsttt/packs"));
        assert!(matches!(missing, Ok(None)));
        let missing = read_source(root.path(), ".gsttt/config.toml", None);
        assert!(matches!(missing, Ok(None)));
    }
}
