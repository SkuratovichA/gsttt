//! Running `git` in a repository.

use std::path::Path;

use xshell::{Shell, cmd};

/// Runs `git <args>` in `root` and returns what it printed, without the trailing newline.
pub(crate) fn git(root: &Path, args: &[&str]) -> anyhow::Result<String> {
    let shell = Shell::new()?;
    shell.change_dir(root);
    Ok(cmd!(shell, "git {args...}").read()?)
}
