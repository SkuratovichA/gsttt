//! Where the repository starts.

use std::path::PathBuf;

use anyhow::Context as _;

use crate::dist::git::git;

/// The top directory of the git repository that holds the current directory.
///
/// A task is run from the repository root as `cargo xtask <task>`, but a shell can be in any directory of the repository, and a task that reads `Cargo.toml` or `.github/workflows` must not depend on which one. The directory `xtask` was built in is no answer: it says where the code came from, not which repository a task works on.
pub(crate) fn repository_root() -> anyhow::Result<PathBuf> {
    let here = std::env::current_dir().context("cannot read the current directory")?;
    let top = git(&here, &["rev-parse", "--show-toplevel"])
        .with_context(|| format!("{} is not inside a git repository", here.display()))?;
    Ok(PathBuf::from(top))
}
