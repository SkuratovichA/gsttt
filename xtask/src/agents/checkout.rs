//! The checkouts of the agent benchmarks, cloned at the pinned commit under the temporary directory: one the questions are drawn from, and one the runs are made in.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use xshell::Shell;

use crate::bench::pins::{Repository, RepositoryAgents};
use crate::bench::setup;
use crate::dist::git::git;

/// What a checkout is for. Each purpose has checkouts of its own, so that what the draw builds and what the runs index never meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Purpose {
    /// The questions are drawn from it: installed and built, since the language service and the tests need the build output.
    Draw,
    /// The runs are made in it: installed and never built, so that an agent finds the sources and the dependencies and no build output beside them.
    Runs,
}

impl Purpose {
    fn directory_name(self) -> &'static str {
        match self {
            Self::Draw => "gsttt-agents",
            Self::Runs => "gsttt-agents-runs",
        }
    }
}

/// The directory the checkouts of `purpose` are in: under the temporary directory of the system, because a CLAUDE.md in a parent directory would reach every run.
pub(crate) fn root(purpose: Purpose) -> PathBuf {
    std::env::temp_dir().join(purpose.directory_name())
}

/// Where `name` is checked out for `purpose`.
pub(crate) fn directory(purpose: Purpose, name: &str) -> PathBuf {
    root(purpose).join(name)
}

/// Where the draw keeps what must not be in the checkout: the reports and the logs of the test runs.
pub(crate) fn scratch(name: &str) -> PathBuf {
    root(Purpose::Draw).join(format!("{name}-draw"))
}

/// The first CLAUDE.md in `dir` or in a directory above it. Claude Code reads one in every parent directory of the directory it runs in, so one found here would reach every run.
pub(crate) fn claude_md_above(dir: &Path) -> Option<PathBuf> {
    const NAMES: [&str; 3] = ["CLAUDE.md", "CLAUDE.local.md", ".claude/CLAUDE.md"];
    // The temporary directory of macOS is behind a link, and a run may see either name of it.
    let canonical = fs::canonicalize(dir).ok();
    [Some(dir.to_path_buf()), canonical]
        .into_iter()
        .flatten()
        .flat_map(|dir| dir.ancestors().map(Path::to_path_buf).collect::<Vec<_>>())
        .flat_map(|ancestor| NAMES.map(|name| ancestor.join(name)))
        .find(|file| file.is_file())
}

/// Puts the repository at its pinned commit in [`directory`], undoes what a run that was killed left in the tracked files, and runs the install command and, for the draw, the build command, once for these commands.
///
/// Returns the canonical path of the checkout: Vitest reports canonical paths, and the temporary directory of macOS is behind a link.
pub(crate) fn prepare(
    repository: &Repository,
    agents: &RepositoryAgents,
    purpose: Purpose,
) -> anyhow::Result<PathBuf> {
    let dir = directory(purpose, &repository.name);
    setup::checkout(&dir, repository)?;
    git(&dir, &["checkout", "--quiet", "--force", "--", "."])?;
    if purpose == Purpose::Runs {
        // What a run that was killed left, apart from the ignored files the install made.
        git(&dir, &["clean", "--force", "-d", "--quiet"])?;
    }
    let dir =
        fs::canonicalize(&dir).with_context(|| format!("cannot resolve {}", dir.display()))?;

    let build = match purpose {
        Purpose::Draw => agents.build.as_ref(),
        Purpose::Runs => None,
    };
    // The marker holds the commands it was prepared with, so that another pin prepares again.
    let marker = dir.join(".git/gsttt-agents-prepared");
    let commands = format!("{:?}\n{:?}\n", agents.install, build);
    if fs::read_to_string(&marker).is_ok_and(|done| done == commands) {
        return Ok(dir);
    }
    let shell = Shell::new()?;
    shell.change_dir(&dir);
    for (what, command) in [("install", agents.install.as_ref()), ("build", build)] {
        let Some((program, args)) = command.and_then(|command| command.split_first()) else {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_draw_and_the_runs_have_checkouts_of_their_own() {
        let draw = directory(Purpose::Draw, "vite");
        let runs = directory(Purpose::Runs, "vite");
        assert_ne!(draw, runs);
        assert_eq!(draw.file_name().unwrap(), "vite");
        assert_eq!(runs.file_name().unwrap(), "vite");
        assert!(!runs.starts_with(root(Purpose::Draw)));
        assert!(scratch("vite").starts_with(root(Purpose::Draw)));
    }

    #[test]
    fn a_claude_md_above_the_checkouts_is_found() {
        let dir = tempfile::tempdir().unwrap();
        let runs = dir.path().join("a/b/runs");
        fs::create_dir_all(&runs).unwrap();

        // The nearest one wins, and every place Claude Code reads one from counts.
        fs::create_dir_all(dir.path().join("a/.claude")).unwrap();
        fs::write(dir.path().join("a/.claude/CLAUDE.md"), "").unwrap();
        assert_eq!(
            claude_md_above(&runs),
            Some(dir.path().join("a/.claude/CLAUDE.md"))
        );
        fs::write(dir.path().join("a/b/CLAUDE.local.md"), "").unwrap();
        assert_eq!(
            claude_md_above(&runs),
            Some(dir.path().join("a/b/CLAUDE.local.md"))
        );
        fs::write(dir.path().join("a/b/runs/CLAUDE.md"), "").unwrap();
        assert_eq!(
            claude_md_above(&runs),
            Some(dir.path().join("a/b/runs/CLAUDE.md"))
        );
    }
}
