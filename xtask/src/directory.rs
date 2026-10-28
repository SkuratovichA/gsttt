//! Listing a directory, the same way for the checks of a release and for the walk of the spec gate.

use std::fs::{self, FileType};
use std::io;
use std::path::{Path, PathBuf};

use anyhow::Context as _;

/// An entry of a directory.
#[derive(Debug)]
pub(crate) struct Entry {
    pub(crate) path: PathBuf,
    /// The type of the entry itself: a symbolic link is a link, whatever it points to, so that a walk which goes into the directories it finds never follows one into a cycle.
    pub(crate) kind: FileType,
}

/// What is in `directory`, sorted by path, or `None` when there is no such directory.
pub(crate) fn read_directory(directory: &Path) -> anyhow::Result<Option<Vec<Entry>>> {
    let listing = || format!("listing {}", directory.display());
    let reader = match fs::read_dir(directory) {
        Ok(reader) => reader,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(listing),
    };
    let mut entries = Vec::new();
    for entry in reader {
        let entry = entry.with_context(listing)?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .with_context(|| format!("reading the type of {}", path.display()))?;
        entries.push(Entry { path, kind });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(Some(entries))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names in `entries`, each with whether it is a directory.
    fn names(entries: &[Entry]) -> Vec<(&str, bool)> {
        entries
            .iter()
            .map(|entry| {
                let name = entry.path.file_name().unwrap().to_str().unwrap();
                (name, entry.kind.is_dir())
            })
            .collect()
    }

    #[test]
    fn the_entries_are_sorted_by_path_whatever_order_they_were_made_in() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("b"), "").unwrap();
        fs::create_dir(dir.path().join("c")).unwrap();
        fs::write(dir.path().join("a.rs"), "").unwrap();
        fs::write(dir.path().join("a"), "").unwrap();

        let entries = read_directory(dir.path()).unwrap().unwrap();

        assert_eq!(
            names(&entries),
            [("a", false), ("a.rs", false), ("b", false), ("c", true)]
        );
        assert_eq!(entries[0].path, dir.path().join("a"));
    }

    #[test]
    fn a_directory_without_entries_is_listed_empty() {
        let dir = tempfile::tempdir().unwrap();

        let entries = read_directory(dir.path()).unwrap().unwrap();

        assert!(entries.is_empty());
    }

    #[test]
    fn a_directory_that_does_not_exist_is_none() {
        let dir = tempfile::tempdir().unwrap();

        assert!(
            read_directory(&dir.path().join("missing"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_path_that_is_no_directory_is_an_error_naming_it() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("plain");
        fs::write(&file, "").unwrap();

        let error = read_directory(&file).unwrap_err();

        let message = format!("{error:#}");
        assert!(
            message.contains("listing ") && message.contains(&file.display().to_string()),
            "{message}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_is_a_link_and_not_what_it_points_to() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("real")).unwrap();
        fs::write(dir.path().join("plain"), "").unwrap();
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("to-directory"))
            .unwrap();
        std::os::unix::fs::symlink(dir.path().join("plain"), dir.path().join("to-file")).unwrap();

        let entries = read_directory(dir.path()).unwrap().unwrap();

        let kind_of = |name: &str| {
            entries
                .iter()
                .find(|entry| entry.path.ends_with(name))
                .unwrap()
                .kind
        };
        assert!(kind_of("real").is_dir() && kind_of("plain").is_file());
        for link in ["to-directory", "to-file"] {
            let kind = kind_of(link);
            assert!(
                kind.is_symlink() && !kind.is_dir() && !kind.is_file(),
                "{link}"
            );
        }
    }
}
