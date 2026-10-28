//! The checkout the questions are drawn from: cloned at the pinned commit under the temporary directory, then installed and built once.

use std::fs;
use std::path::PathBuf;

use anyhow::Context as _;
use xshell::Shell;

use crate::bench::pins::{Repository, RepositoryAgents};
use crate::bench::setup;
use crate::dist::git::git;

/// Where `name` is checked out for the agent benchmarks: under the temporary directory of the system, because a CLAUDE.md in a parent directory would reach every run.
pub(crate) fn directory(name: &str) -> PathBuf {
    std::env::temp_dir().join("gsttt-agents").join(name)
}

/// Where the draw keeps what must not be in the checkout: the reports and the logs of the test runs.
pub(crate) fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir()
        .join("gsttt-agents")
        .join(format!("{name}-draw"))
}

/// Puts the repository at its pinned commit in [`directory`], undoes what a run that was killed left in the tracked files, and runs the install command and then the build command, once for these commands.
///
/// Returns the canonical path of the checkout: Vitest reports canonical paths, and the temporary directory of macOS is behind a link.
pub(crate) fn prepare(
    repository: &Repository,
    agents: &RepositoryAgents,
) -> anyhow::Result<PathBuf> {
    let dir = directory(&repository.name);
    setup::checkout(&dir, repository)?;
    git(&dir, &["checkout", "--quiet", "--force", "--", "."])?;
    let dir =
        fs::canonicalize(&dir).with_context(|| format!("cannot resolve {}", dir.display()))?;

    // The marker holds the commands it was prepared with, so that another pin prepares again.
    let marker = dir.join(".git/gsttt-agents-prepared");
    let commands = format!("{:?}\n{:?}\n", agents.install, agents.build);
    if fs::read_to_string(&marker).is_ok_and(|done| done == commands) {
        return Ok(dir);
    }
    let shell = Shell::new()?;
    shell.change_dir(&dir);
    for (what, command) in [("install", &agents.install), ("build", &agents.build)] {
        let Some((program, args)) = command.as_deref().and_then(<[String]>::split_first) else {
            continue;
        };
        shell.cmd(program).args(args).run().with_context(|| {
            format!(
                "the {what} command of `{}` failed in {}",
                repository.name,
                dir.display()
            )
        })?;
    }
    fs::write(&marker, commands).with_context(|| format!("cannot write {}", marker.display()))?;
    Ok(dir)
}
