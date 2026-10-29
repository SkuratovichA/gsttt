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

/// The directory of the files that every run of the task shares and that a run has no use for: the copy of the binary under test and the files of the setups. It is beside the checkouts and not in them, so that a run cannot find it by looking at the parent directory of its checkout.
pub(crate) fn task_dir() -> PathBuf {
    std::env::temp_dir().join("gsttt-agents-task")
}

/// The directory of the link to `node`, which is all it holds. A run that is denied [`task_dir`] still needs `node`, so the link is beside that directory and not in it.
pub(crate) fn node_dir() -> PathBuf {
    std::env::temp_dir().join("gsttt-agents-node")
}

/// The directory the temporary directory of every run is made in, one directory per run.
pub(crate) fn tmp_root() -> PathBuf {
    std::env::temp_dir().join("gsttt-agents-tmp")
}

/// Every directory the task makes itself and in which it may end processes: the directories of the checkouts and the temporary directories of the runs. A directory that is not below one of them is not the task's to sweep.
pub(crate) fn made_by_the_task() -> [PathBuf; 3] {
    [root(Purpose::Draw), root(Purpose::Runs), tmp_root()]
}

/// `dirs` by the name each is given and by its canonical name, when it has one. The temporary directory of macOS is behind a link, and a process may open, report or match either name of a directory in it.
pub(super) fn with_canonical_names(dirs: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    dirs.into_iter()
        .flat_map(|dir| {
            let canonical = fs::canonicalize(&dir).ok();
            [Some(dir), canonical].into_iter().flatten()
        })
        .collect()
}

/// The first CLAUDE.md or AGENTS.md in `dir` or in a directory above it. Claude Code reads a CLAUDE.md in every parent directory of the directory it runs in, and it ships a plugin, `cc-plugin-agents-md`, that reads an AGENTS.md, so either found here would reach every run.
pub(crate) fn claude_md_above(dir: &Path) -> Option<PathBuf> {
    const NAMES: [&str; 4] = [
        "CLAUDE.md",
        "CLAUDE.local.md",
        ".claude/CLAUDE.md",
        "AGENTS.md",
    ];
    with_canonical_names([dir.to_path_buf()])
        .into_iter()
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

    #[test]
    fn an_agents_md_above_the_checkouts_is_found() {
        let dir = tempfile::tempdir().unwrap();
        let runs = dir.path().join("a/b/runs");
        fs::create_dir_all(&runs).unwrap();
        assert_eq!(claude_md_above(&runs), None);
        fs::write(dir.path().join("a/AGENTS.md"), "").unwrap();
        assert_eq!(claude_md_above(&runs), Some(dir.path().join("a/AGENTS.md")));
        // The nearest one wins, here too, and the directory itself counts.
        fs::write(runs.join("AGENTS.md"), "").unwrap();
        assert_eq!(claude_md_above(&runs), Some(runs.join("AGENTS.md")));
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "The binary and the files of the setups are outside the checkouts"
    )]
    #[test]
    fn what_the_task_keeps_for_itself_is_beside_the_checkouts_and_not_in_them() {
        let all = [
            task_dir(),
            node_dir(),
            tmp_root(),
            root(Purpose::Draw),
            root(Purpose::Runs),
        ];
        for dir in &all {
            for other in &all {
                assert!(
                    dir == other || !dir.starts_with(other),
                    "{} is in {}",
                    dir.display(),
                    other.display()
                );
            }
        }
        // Every one of them is a place the sweep of the task may end processes in, bar the two the runs are not made in.
        let made = made_by_the_task();
        assert!(made.contains(&tmp_root()) && made.contains(&root(Purpose::Runs)));
        assert!(!made.contains(&task_dir()) && !made.contains(&node_dir()));
    }
}
