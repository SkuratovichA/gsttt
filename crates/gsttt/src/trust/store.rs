//! The file where the trust of the user is kept, outside every repository.

use std::collections::BTreeSet;
use std::fs;
use std::io;

use camino::{Utf8Path, Utf8PathBuf};
use gsttt_core::atomic;
use gsttt_core::engine::workspace::UserDir;

/// Why the store cannot be found, read or written.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The environment names no place to keep it.
    #[error(
        "cannot tell where to keep the trust: neither XDG_STATE_HOME nor HOME is set to a path"
    )]
    NoStateHome,

    /// The file cannot be read.
    #[error("cannot read the trust store `{path}`: {source}")]
    Read {
        /// The store.
        path: Utf8PathBuf,
        /// What the system said.
        source: io::Error,
    },

    /// The file, or the folder it lies in, cannot be written.
    #[error("cannot write the trust store `{path}`: {source}")]
    Write {
        /// The store.
        path: Utf8PathBuf,
        /// What the system said.
        source: io::Error,
    },

    /// A line holds one trust and ends at the line break, so a root with a line break in its path cannot be recorded.
    #[error("cannot record trust for `{root:?}`: its path holds a line break")]
    Root {
        /// The root.
        root: String,
    },
}

/// The trust of the user: one line per trusted external, `<key> <canonical root>`, sorted, in `$XDG_STATE_HOME/gsttt/trust` or `~/.local/state/gsttt/trust`.
///
/// A line is an opaque string to everything but the lookup of a root. The file is replaced through a temporary file in its own folder, so that a reader sees the old list or the new one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Store {
    file: Utf8PathBuf,
}

impl Store {
    /// The store the environment of this process names.
    ///
    /// # Errors
    ///
    /// When neither `XDG_STATE_HOME` nor `HOME` is set to a path.
    pub fn locate() -> Result<Self, StoreError> {
        let state = UserDir::State.locate().ok_or(StoreError::NoStateHome)?;
        Ok(Self::at(&state))
    }

    /// The store in the state folder `state`.
    pub fn at(state: &Utf8Path) -> Self {
        Self {
            file: state.join("gsttt").join("trust"),
        }
    }

    /// The keys trusted in `root`.
    ///
    /// # Errors
    ///
    /// When the file exists and cannot be read.
    pub fn trusted(&self, root: &Utf8Path) -> Result<BTreeSet<String>, StoreError> {
        Ok(self
            .lines()?
            .iter()
            .filter_map(|line| line.split_once(' '))
            .filter(|(_, trusted_root)| *trusted_root == root.as_str())
            .map(|(key, _)| key.to_owned())
            .collect())
    }

    /// Trusts the externals with `keys` in `root`. A key that is trusted already stays as it is.
    ///
    /// # Errors
    ///
    /// When the root cannot be recorded, or the file cannot be read or written.
    pub fn trust(&self, root: &Utf8Path, keys: &[String]) -> Result<(), StoreError> {
        if root.as_str().contains(['\n', '\r']) {
            return Err(StoreError::Root {
                root: root.to_string(),
            });
        }
        let mut lines = self.lines()?;
        lines.extend(keys.iter().map(|key| format!("{key} {root}")));
        self.replace(&lines)
    }

    /// Forgets every trust recorded for `root` and returns how many there were.
    ///
    /// # Errors
    ///
    /// When the file cannot be read or written.
    pub fn forget(&self, root: &Utf8Path) -> Result<usize, StoreError> {
        let mut lines = self.lines()?;
        let before = lines.len();
        lines.retain(|line| {
            line.split_once(' ')
                .is_none_or(|(_, kept)| kept != root.as_str())
        });
        let forgotten = before - lines.len();
        if forgotten > 0 {
            self.replace(&lines)?;
        }
        Ok(forgotten)
    }

    /// The lines of the file, sorted and without repeats. A file that is not there holds none.
    fn lines(&self) -> Result<BTreeSet<String>, StoreError> {
        match fs::read_to_string(&self.file) {
            Ok(text) => Ok(text
                .lines()
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(BTreeSet::new()),
            Err(source) => Err(StoreError::Read {
                path: self.file.clone(),
                source,
            }),
        }
    }

    /// Makes `lines` the file, one to a line, with [`atomic::write`].
    fn replace(&self, lines: &BTreeSet<String>) -> Result<(), StoreError> {
        let text = lines.iter().fold(String::new(), |mut text, line| {
            text.push_str(line);
            text.push('\n');
            text
        });
        atomic::write(self.file.as_std_path(), "trust.", text.as_bytes()).map_err(|source| {
            StoreError::Write {
                path: self.file.clone(),
                source,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use tempfile::TempDir;

    use super::*;

    /// A store in a folder of its own.
    fn store() -> (TempDir, Store) {
        let dir = TempDir::new().unwrap();
        let state = Utf8PathBuf::from_path_buf(dir.path().to_owned()).unwrap();
        let store = Store::at(&state);
        (dir, store)
    }

    fn keys(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|key| (*key).to_owned()).collect()
    }

    fn set(keys: &[&str]) -> BTreeSet<String> {
        keys.iter().map(|key| (*key).to_owned()).collect()
    }

    #[test]
    fn a_store_that_is_not_there_trusts_and_forgets_nothing_and_stays_absent() {
        let (_dir, store) = store();
        let root = Utf8Path::new("/work/app");

        assert!(store.trusted(root).unwrap().is_empty());
        assert_eq!(store.forget(root).unwrap(), 0);

        assert!(!store.file.exists());
    }

    #[test]
    fn trust_belongs_to_the_root_it_was_given_in() {
        let (_dir, store) = store();
        store
            .trust(Utf8Path::new("/work/a"), &keys(&["k1", "k2"]))
            .unwrap();
        store
            .trust(Utf8Path::new("/work/b"), &keys(&["k1"]))
            .unwrap();

        assert_eq!(
            store.trusted(Utf8Path::new("/work/a")).unwrap(),
            set(&["k1", "k2"])
        );
        assert_eq!(
            store.trusted(Utf8Path::new("/work/b")).unwrap(),
            set(&["k1"])
        );
        assert!(store.trusted(Utf8Path::new("/work")).unwrap().is_empty());
        assert!(
            store
                .trusted(Utf8Path::new("/work/a/b"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn the_file_holds_one_sorted_line_per_trust() {
        let (_dir, store) = store();
        let root = Utf8Path::new("/work/app");
        store.trust(root, &keys(&["k2", "k1"])).unwrap();
        store
            .trust(Utf8Path::new("/work/Zed"), &keys(&["k1"]))
            .unwrap();
        store.trust(root, &keys(&["k1", "k3"])).unwrap();

        let text = fs::read_to_string(&store.file).unwrap();

        assert_eq!(
            text,
            "k1 /work/Zed\nk1 /work/app\nk2 /work/app\nk3 /work/app\n"
        );
    }

    #[test]
    fn forgetting_a_root_keeps_the_others_and_counts_what_it_removed() {
        let (_dir, store) = store();
        store
            .trust(Utf8Path::new("/work/a"), &keys(&["k1", "k2"]))
            .unwrap();
        store
            .trust(Utf8Path::new("/work/b"), &keys(&["k1"]))
            .unwrap();

        assert_eq!(store.forget(Utf8Path::new("/work/a")).unwrap(), 2);

        assert!(store.trusted(Utf8Path::new("/work/a")).unwrap().is_empty());
        assert_eq!(
            store.trusted(Utf8Path::new("/work/b")).unwrap(),
            set(&["k1"])
        );
        assert_eq!(store.forget(Utf8Path::new("/work/a")).unwrap(), 0);
    }

    #[test]
    fn a_root_with_spaces_is_kept_whole() {
        let (_dir, store) = store();
        let root = Utf8Path::new("/home/me/my projects/app");
        store.trust(root, &keys(&["k1"])).unwrap();

        assert_eq!(store.trusted(root).unwrap(), set(&["k1"]));
        assert!(
            store
                .trusted(Utf8Path::new("/home/me/my"))
                .unwrap()
                .is_empty()
        );
        assert_eq!(store.forget(root).unwrap(), 1);
    }

    #[test]
    fn a_root_with_a_line_break_is_refused_and_nothing_is_written() {
        let (_dir, store) = store();

        for root in ["/work/a\nb", "/work/a\rb"] {
            let refused = store.trust(Utf8Path::new(root), &keys(&["k1"]));
            assert!(matches!(refused, Err(StoreError::Root { .. })), "{root:?}");
        }

        assert!(!store.file.exists());
    }

    #[test]
    fn lines_that_are_blank_or_hold_no_root_are_ignored_by_the_lookup() {
        let (_dir, store) = store();
        fs::create_dir_all(store.file.parent().unwrap()).unwrap();
        fs::write(&store.file, "garbage\n\nk1 /work/a\n").unwrap();

        assert_eq!(
            store.trusted(Utf8Path::new("/work/a")).unwrap(),
            set(&["k1"])
        );
        assert!(store.trusted(Utf8Path::new("garbage")).unwrap().is_empty());
    }

    #[test]
    fn no_temporary_file_stays_in_the_folder() {
        let (_dir, store) = store();
        let root = Utf8Path::new("/work/a");
        store.trust(root, &keys(&["k1"])).unwrap();
        store.forget(root).unwrap();
        store.trust(root, &keys(&["k2"])).unwrap();

        let folder = store.file.parent().unwrap();
        let names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();

        assert_eq!(names, ["trust"]);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        /// Whatever roots and keys go in, the lookup finds exactly the keys of its root, and forgetting a root removes exactly its lines.
        #[test]
        fn what_is_trusted_is_found_again_under_its_root_only(
            roots in proptest::collection::btree_set("/[a-z ]{1,8}(/[a-z ]{1,8}){0,2}", 1..4),
            keys in proptest::collection::btree_set("[0-9a-f]{64}", 1..4),
        ) {
            let (_dir, store) = store();
            let keys: Vec<String> = keys.into_iter().collect();
            for root in &roots {
                store.trust(Utf8Path::new(root), &keys).unwrap();
            }
            let all = keys.iter().cloned().collect::<BTreeSet<_>>();
            for root in &roots {
                prop_assert_eq!(&store.trusted(Utf8Path::new(root)).unwrap(), &all);
            }
            let first = roots.iter().next().unwrap();
            prop_assert_eq!(store.forget(Utf8Path::new(first)).unwrap(), keys.len());
            prop_assert!(store.trusted(Utf8Path::new(first)).unwrap().is_empty());
            for root in roots.iter().skip(1) {
                prop_assert_eq!(&store.trusted(Utf8Path::new(root)).unwrap(), &all);
            }
        }
    }
}
