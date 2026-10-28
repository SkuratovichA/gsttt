//! Trust for the external connectors of a repository.
//!
//! An external connector is a command that the config names, so reading a repository that someone else wrote would run what they chose. Nothing runs until the user says so in a terminal: the answer is recorded in a [`Store`] outside every repository, under a [`key`] that covers the command and the files it runs, so that a change to any of them asks again.

mod key;
mod store;

use camino::Utf8Path;
use gsttt_core::setup::ExternalSetup;

pub use key::{KeyError, key};
pub use store::{Store, StoreError};

/// An `[[external]]` of the config, and whether the user trusts it in this repository.
#[derive(Debug)]
pub struct Declared {
    setup: ExternalSetup,
    key: Result<String, KeyError>,
    trusted: bool,
}

impl Declared {
    /// The declaration as the config has it.
    pub const fn setup(&self) -> &ExternalSetup {
        &self.setup
    }

    /// The name of the connector.
    pub fn name(&self) -> &str {
        &self.setup.name
    }

    /// The command as a shell would take it: arguments that need it are quoted.
    pub fn command(&self) -> String {
        shlex::try_join(self.setup.cmd.iter().map(String::as_str))
            .unwrap_or_else(|_| self.setup.cmd.join(" "))
    }

    /// Whether the user trusts this declaration in this repository, which it was asked about with this very command and these very files.
    pub const fn trusted(&self) -> bool {
        self.trusted
    }

    /// The key to record when the user trusts it, or why there is none.
    pub fn key(&self) -> Result<&str, &KeyError> {
        self.key.as_deref()
    }

    /// What `gsttt config check` and the coverage of every answer say about a connector that is not trusted.
    pub fn problem(&self) -> String {
        format!(
            "the external connector `{}` ({}) is not trusted in this repository; run `gsttt config trust` in a terminal to allow it",
            self.name(),
            self.command()
        )
    }
}

/// The trust of each of `externals` in `root`, a canonical path, in the order of the config. Without a `store`, or with one that cannot be read, nothing is trusted: the user is asked again, and the command that asks reports why it cannot record the answer.
pub fn declare(
    root: &Utf8Path,
    externals: &[ExternalSetup],
    store: Option<&Store>,
) -> Vec<Declared> {
    if externals.is_empty() {
        return Vec::new();
    }
    let trusted = store
        .map(|store| store.trusted(root))
        .transpose()
        .unwrap_or_else(|error| {
            tracing::error!(%error, "no external connector is trusted, because the trust store cannot be read");
            None
        })
        .unwrap_or_default();
    externals
        .iter()
        .map(|setup| {
            let key = key(root, setup);
            let trusted = key.as_ref().is_ok_and(|key| trusted.contains(key));
            Declared {
                setup: setup.clone(),
                key,
                trusted,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    fn external(cmd: &[&str]) -> ExternalSetup {
        serde_json::from_value(json!({"name": "kotlin", "cmd": cmd, "files": ["**/*.kt"]})).unwrap()
    }

    /// A root, which is a canonical path, and a store beside it.
    fn folders() -> (TempDir, Utf8PathBuf, Store) {
        let dir = TempDir::new().unwrap();
        let base = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();
        let root = base.join("repo");
        std::fs::create_dir(&root).unwrap();
        let store = Store::at(&base.join("state"));
        (dir, root, store)
    }

    #[test]
    fn the_store_decides_for_the_command_it_was_given_for_and_the_root() {
        let (_dir, root, store) = folders();
        let asked = external(&["gsttt-kotlin", "--stdio"]);
        store.trust(&root, &[key(&root, &asked).unwrap()]).unwrap();

        let same = declare(&root, std::slice::from_ref(&asked), Some(&store));
        let edited = declare(
            &root,
            &[external(&["gsttt-kotlin", "--again"])],
            Some(&store),
        );
        let elsewhere = declare(
            Utf8Path::new("/elsewhere"),
            std::slice::from_ref(&asked),
            Some(&store),
        );

        assert!(same.first().is_some_and(Declared::trusted));
        assert!(edited.first().is_some_and(|declared| !declared.trusted()));
        assert!(
            elsewhere
                .first()
                .is_some_and(|declared| !declared.trusted())
        );
    }

    #[test]
    fn without_a_store_nothing_is_trusted_and_the_order_of_the_config_stays() {
        let (_dir, root, _store) = folders();
        let declared = declare(&root, &[external(&["a"]), external(&["b"])], None);

        let commands: Vec<String> = declared.iter().map(Declared::command).collect();

        assert_eq!(commands, ["a", "b"]);
        assert!(declared.iter().all(|declared| !declared.trusted()));
    }

    #[test]
    fn the_command_is_quoted_so_that_an_argument_with_a_space_stays_one_argument() {
        let (_dir, root, _store) = folders();
        let declared = declare(&root, &[external(&["gsttt-kotlin", "--root dir"])], None);

        assert_eq!(
            declared.first().map(Declared::problem).as_deref(),
            Some(
                "the external connector `kotlin` (gsttt-kotlin '--root dir') is not trusted in this repository; run `gsttt config trust` in a terminal to allow it"
            )
        );
    }
}
