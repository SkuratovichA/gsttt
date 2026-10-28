//! Repositories on disk for the tests.

use std::fs;
use std::path::{Path, PathBuf};

use gsttt_config::{ConfigError, evaluate};
use gsttt_core::Setup;
use tempfile::TempDir;

use crate::fakes::Registry;

/// A repository root in a temporary directory.
pub(crate) struct Tree {
    dir: TempDir,
}

impl Tree {
    pub(crate) fn empty() -> Self {
        Self {
            dir: TempDir::new().unwrap(),
        }
    }

    /// A repository whose `.gsttt/init.lua` is `init`.
    pub(crate) fn with_init(init: &str) -> Self {
        let tree = Self::empty();
        tree.write(".gsttt/init.lua", init);
        tree
    }

    pub(crate) fn write(&self, relative: &str, text: &str) {
        let path = self.dir.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    pub(crate) fn root(&self) -> &Path {
        self.dir.path()
    }

    pub(crate) fn evaluate(&self) -> Result<Setup, ConfigError> {
        evaluate_at(self.root())
    }
}

/// Evaluates the config under `root` against the stand-in registry.
pub(crate) fn evaluate_at(root: &Path) -> Result<Setup, ConfigError> {
    let registry = Registry::standard();
    evaluate(root, &registry.connectors())
}

/// A fixture tree of `tests/fixtures/`.
pub(crate) fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The message of an `init.lua` that must not evaluate.
pub(crate) fn failure(init: &str) -> String {
    Tree::with_init(init).evaluate().unwrap_err().to_string()
}
