//! Listing one directory, in one place for the reader of the `openspec/` tree and for the tasks of `xtask` that walk the repository, so that no two listings can differ in their order or in what they do with a symbolic link.

use std::fs::{self, FileType};
use std::io;
use std::path::{Path, PathBuf};

use crate::error::Error;

/// An entry of a directory.
#[derive(Debug)]
pub struct Entry {
    /// The directory as [`read_directory`] was given it, joined with the name of the entry.
    pub path: PathBuf,
    /// The type of the entry itself: a symbolic link is a link, whatever it points to, so that a walk which goes into the directories it finds never follows one into a cycle.
    pub kind: FileType,
}

/// What is in `directory`, or `None` when there is no such directory, so that a caller can tell a missing directory from an empty one, and a caller that does not care takes `unwrap_or_default()`.
///
/// The entries are sorted by path, so that everything a caller emits has a fixed order. The paths of the entries of one directory differ only in their last component, so this is also the order of their names, compared as the operating system spells them and not as text.
///
/// # Errors
///
/// Fails with [`Error::Read`] when `directory` exists and cannot be listed, and when the type of one of its entries cannot be read. The error names the path as it was given here, which for an entry is `directory` joined with its name.
pub fn read_directory(directory: &Path) -> Result<Option<Vec<Entry>>, Error> {
    let failed = |path: &Path, source: io::Error| Error::Read {
        path: path.to_path_buf(),
        source,
    };
    let listing = match fs::read_dir(directory) {
        Ok(listing) => listing,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(failed(directory, source)),
    };
    let mut entries = Vec::new();
    for item in listing {
        let item = item.map_err(|source| failed(directory, source))?;
        let path = item.path();
        let kind = item.file_type().map_err(|source| failed(&path, source))?;
        entries.push(Entry { path, kind });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(Some(entries))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

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
    fn the_order_of_the_paths_is_the_order_of_the_names() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["b", "a.rs", "a-b", "a b", "a", "Z", "ab", "a_b", "a.b"] {
            fs::write(dir.path().join(name), "").unwrap();
        }

        let entries = read_directory(dir.path()).unwrap().unwrap();

        let listed: Vec<&OsStr> = entries
            .iter()
            .map(|entry| entry.path.file_name().unwrap())
            .collect();
        let mut by_name = listed.clone();
        by_name.sort();
        assert_eq!(listed, by_name);
        assert_eq!(listed.len(), 9);
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

        assert!(
            matches!(&error, Error::Read { path, .. } if *path == file),
            "{error}"
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
