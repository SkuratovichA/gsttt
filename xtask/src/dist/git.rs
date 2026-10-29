//! Running `git` in a repository.

use std::path::Path;

use xshell::{Shell, cmd};

/// Runs `git <args>` in `root` and returns what it printed, without the trailing newline. No hook and no file system monitor of the repository runs: the repository may be a checkout that a run could write, and what it names would run outside the sandbox, with the rights of the user. No caller of the task wants either.
pub(crate) fn git(root: &Path, args: &[&str]) -> anyhow::Result<String> {
    let shell = Shell::new()?;
    shell.change_dir(root);
    Ok(cmd!(
        shell,
        "git -c core.fsmonitor=false -c core.hooksPath=/dev/null {args...}"
    )
    .read()?)
}
