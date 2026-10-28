//! Replacing a file as a whole, so that a reader sees the old file or the new one and never half of either.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// The ending of the temporary file [`write`] makes. A caller that wants to clear the leftovers of a process that died between creating the file and renaming it looks for its own prefix and this suffix.
pub const TEMP_SUFFIX: &str = ".tmp";

/// Makes `bytes` the file at `path`, through a temporary file in the same folder named `temp_prefix`, random letters and [`TEMP_SUFFIX`], and a rename. The folder is created when it is missing, and the temporary file is synced before the rename: the rename makes the new file the one at `path` at once, and without the sync a crash after it can leave a file whose blocks never reached the disk. When a step fails, `path` keeps what it held and the temporary file is removed.
///
/// # Errors
///
/// When the folder or the temporary file cannot be created, written or synced, or the temporary file cannot be renamed over `path`.
pub fn write(path: &Path, temp_prefix: &str, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let mut file = tempfile::Builder::new()
        .prefix(temp_prefix)
        .suffix(TEMP_SUFFIX)
        .tempfile_in(dir)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names in `dir`, sorted.
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_write_makes_the_folder_and_holds_the_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a").join("b").join("file");

        write(&file, "t.", b"first").unwrap();

        assert_eq!(fs::read(&file).unwrap(), b"first");
    }

    #[test]
    fn a_write_replaces_the_whole_file_and_leaves_nothing_else_in_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        write(&file, "t.", b"a first text that is long").unwrap();

        write(&file, "t.", b"short").unwrap();

        assert_eq!(fs::read(&file).unwrap(), b"short");
        assert_eq!(names(dir.path()), ["file"]);
    }

    #[test]
    fn a_write_that_cannot_rename_returns_the_error_and_removes_its_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        fs::create_dir(&target).unwrap();

        let result = write(&target, "t.", b"text");

        assert!(result.is_err());
        assert!(target.is_dir());
        assert_eq!(names(dir.path()), ["target"]);
    }
}
