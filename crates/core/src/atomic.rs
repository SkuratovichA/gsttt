//! Replacing a file as a whole, so that a reader sees the old file or the new one and never half of either.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// The ending of the temporary file [`write`] makes, which [`is_temporary`] looks for.
const TEMP_SUFFIX: &str = ".tmp";

/// Makes `bytes` the file at `path`, through a temporary file in the same folder named `temp_prefix`, random letters and `.tmp`, and a rename. The folder is created when it is missing, and the temporary file is synced before the rename: the rename makes the new file the one at `path` at once, and without the sync a crash after it can leave a file whose blocks never reached the disk. When a step fails, `path` keeps what it held and the temporary file is removed.
///
/// # Errors
///
/// When the folder or the temporary file cannot be created, written or synced, or the temporary file cannot be renamed over `path`.
pub fn write(path: &Path, temp_prefix: &str, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let mut file = builder(temp_prefix).tempfile_in(dir)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}

/// Whether `name` is the name of a temporary file that [`write`] makes with `temp_prefix`: the prefix, random letters and the ending `.tmp`. A caller that wants to clear the leftovers of a process that died between creating the file and renaming it asks this of the names in the folder, so that the test of a name sits beside the code that makes it and the two cannot drift apart.
pub fn is_temporary(name: &str, temp_prefix: &str) -> bool {
    name.starts_with(temp_prefix) && name.ends_with(TEMP_SUFFIX)
}

/// The settings of the temporary file of [`write`], kept in one place so that the tests can make a file named the way [`write`] names its own.
fn builder(temp_prefix: &str) -> tempfile::Builder<'_, 'static> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(temp_prefix).suffix(TEMP_SUFFIX);
    builder
}

#[cfg(test)]
mod tests {
    use std::io::Read;

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

    /// A reader that has the file open keeps the bytes it opened, because the new file is renamed over the name and the old one is not written over.
    #[cfg(unix)]
    #[test]
    fn a_write_replaces_the_file_by_a_rename_and_not_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        write(&file, "t.", b"old bytes").unwrap();
        let mut reader = fs::File::open(&file).unwrap();

        write(&file, "t.", b"new bytes").unwrap();

        let mut seen = Vec::new();
        reader.read_to_end(&mut seen).unwrap();
        assert_eq!(seen, b"old bytes");
        assert_eq!(fs::read(&file).unwrap(), b"new bytes");
    }

    #[test]
    fn the_temporary_file_of_a_write_is_told_from_the_file_it_writes() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("file"), "t.", b"text").unwrap();
        let written = names(dir.path());
        let temporary = builder("t.").tempfile_in(dir.path()).unwrap();
        let temporary_name = temporary.path().file_name().unwrap().to_str().unwrap();

        assert_eq!(written, ["file"]);
        assert!(!is_temporary(&written[0], "t."), "{written:?}");
        assert!(is_temporary(temporary_name, "t."), "{temporary_name}");
    }

    #[test]
    fn a_name_needs_the_prefix_and_the_ending_to_be_taken_for_a_temporary_file() {
        // The ending is spelled out because files an earlier version left behind carry it.
        assert!(is_temporary("t.abc123.tmp", "t."));
        assert!(!is_temporary("t.abc123", "t."), "no ending");
        assert!(!is_temporary("other.abc123.tmp", "t."), "another prefix");
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
