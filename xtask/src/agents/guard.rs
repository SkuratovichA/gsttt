//! The checkout of the runs: a run must leave it as the preparation left it, and one that did not is undone before the next.

use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, ensure};

use crate::dist::git::git;

/// One entry of the status of a checkout: a code and a path. The code is the two letters of `git status --porcelain` for a tracked file, `??` for an untracked file and `!!` for an ignored one; a file is one entry whatever else is in its directory.
type Entry = (String, String);

/// Where HEAD is: the commit it names and, when it is not detached, the branch it is on.
#[derive(Debug, PartialEq, Eq)]
struct Head {
    commit: String,
    /// The short name of the branch.
    branch: Option<String>,
}

impl Head {
    fn read(dir: &Path) -> anyhow::Result<Self> {
        // For a detached HEAD git answers `HEAD` itself, which names no branch.
        let name = git(dir, &["rev-parse", "--symbolic-full-name", "HEAD"])?;
        Ok(Self {
            commit: git(dir, &["rev-parse", "HEAD"])?,
            branch: name.strip_prefix("refs/heads/").map(str::to_owned),
        })
    }

    /// Puts HEAD back in the form it had, at the commit it had: a branch is reset to the commit, whether the run moved it, left it or deleted it. The tracked files are checked out with it.
    fn put_back(&self, dir: &Path) -> anyhow::Result<()> {
        let mut args = vec!["checkout", "--quiet", "--force"];
        match &self.branch {
            Some(branch) => args.extend(["-B", branch]),
            None => args.push("--detach"),
        }
        args.push(&self.commit);
        git(dir, &args).map(drop)
    }
}

/// The status of a checkout before a run.
#[derive(Debug)]
pub(super) struct Guard {
    dir: PathBuf,
    head: Head,
    before: BTreeSet<Entry>,
}

impl Guard {
    /// Takes the status of the checkout in `dir`.
    pub(super) fn new(dir: &Path) -> anyhow::Result<Self> {
        Ok(Self {
            dir: dir.to_path_buf(),
            head: Head::read(dir)?,
            before: status(dir)?,
        })
    }

    /// Whether the run changed the checkout. When it did, HEAD is put back where it was, the tracked files are reset, and what the run added, ignored files included, is removed, file by file; it fails when the checkout is still not as it was.
    pub(super) fn finish(self) -> anyhow::Result<bool> {
        let moved = Head::read(&self.dir)? != self.head;
        if !moved && status(&self.dir)? == self.before {
            return Ok(false);
        }
        if moved {
            self.head.put_back(&self.dir)?;
        }
        git(&self.dir, &["reset", "--hard", "--quiet"])?;
        // Read again after the reset: a file the run added to the index is untracked once more, and only now seen as new.
        let after = status(&self.dir)?;
        let added: Vec<&str> = after
            .difference(&self.before)
            .filter(|(code, _)| code == "??" || code == "!!")
            .map(|(_, path)| path.as_str())
            .collect();
        remove(&self.dir, &added)?;
        ensure!(
            Head::read(&self.dir)? == self.head && status(&self.dir)? == self.before,
            "the checkout {} is not as the preparation left it, and could not be restored",
            self.dir.display()
        );
        Ok(true)
    }
}

/// Removes `added`, the paths relative to `dir`, and the directories that are empty without them. A directory with anything else in it stays, so nothing that was there before the run goes with what the run added.
fn remove(dir: &Path, added: &[&str]) -> anyhow::Result<()> {
    for &path in added {
        let target = dir.join(path);
        // A repository the run made inside the checkout is listed as a directory, with a slash.
        let removed = if path.ends_with('/') {
            fs::remove_dir_all(&target)
        } else {
            fs::remove_file(&target)
        };
        match removed {
            Err(error) if error.kind() != ErrorKind::NotFound => {
                return Err(error).with_context(|| format!("cannot remove {}", target.display()));
            }
            _ => {}
        }
        for parent in Path::new(path).ancestors().skip(1) {
            if parent.as_os_str().is_empty() || fs::remove_dir(dir.join(parent)).is_err() {
                break;
            }
        }
    }
    Ok(())
}

/// The status of `dir`, one entry per file: the tracked files that differ from the index or from HEAD, the untracked files and the ignored ones, each listed by `git ls-files` and not by `git status`, which shows a directory with nothing but new or ignored files as one entry and the files in it one by one once a file of another kind comes into it. A file that was there before a run is then never taken for one the run added.
///
/// Only the names are compared: what a run changes in a file that is untracked or ignored, such as one in `node_modules`, is not seen.
fn status(dir: &Path) -> anyhow::Result<BTreeSet<Entry>> {
    let tracked = git(
        dir,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=no",
            "--no-renames",
        ],
    )?;
    let mut entries: BTreeSet<Entry> = tracked
        .split('\0')
        .filter_map(|record| {
            let (code, path) = record.split_at_checked(2)?;
            Some((
                code.to_owned(),
                path.strip_prefix(' ').unwrap_or(path).to_owned(),
            ))
        })
        .collect();
    for (code, flags) in [
        ("??", &["--others"][..]),
        ("!!", &["--others", "--ignored"]),
    ] {
        let mut args = vec!["ls-files", "-z", "--exclude-standard"];
        args.extend(flags);
        let files = git(dir, &args)?;
        entries.extend(
            files
                .split('\0')
                .filter(|path| !path.is_empty())
                .map(|path| (code.to_owned(), path.to_owned())),
        );
    }
    Ok(entries)
}

#[cfg(test)]
pub(super) fn commit(dir: &Path, message: &str) {
    let args = [
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--quiet",
        "--allow-empty",
        "-m",
        message,
    ];
    git(dir, &args).unwrap();
}

/// A git repository for the tests of the runs, with a tracked `a.ts`, a directory `ignored/` that was there when the run started, and every `*.log` ignored. The path is canonical.
#[cfg(test)]
pub(super) fn repo() -> (tempfile::TempDir, PathBuf) {
    use std::fs;

    let guard = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(guard.path()).unwrap();
    for args in [
        &["init", "--quiet"][..],
        &["config", "user.email", "t@example.test"],
        &["config", "user.name", "t"],
    ] {
        git(&dir, args).unwrap();
    }
    fs::write(dir.join(".gitignore"), "ignored/\n*.log\n").unwrap();
    fs::write(dir.join("a.ts"), "export {}\n").unwrap();
    git(&dir, &["add", "."]).unwrap();
    commit(&dir, "init");
    fs::create_dir(dir.join("ignored")).unwrap();
    fs::write(dir.join("ignored/installed"), "x").unwrap();
    (guard, dir)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A run that writes into the checkout is marked and undone"
    )]
    #[test]
    fn a_run_that_writes_into_the_checkout_is_marked_and_undone() {
        let (_guard, dir) = repo();
        let guard = Guard::new(&dir).unwrap();
        fs::write(dir.join("notes.md"), "a new file").unwrap();
        fs::write(dir.join("run.log"), "a new ignored file").unwrap();
        fs::create_dir(dir.join("scratch")).unwrap();
        fs::write(dir.join("scratch/x.ts"), "a new directory").unwrap();
        fs::write(dir.join("a.ts"), "changed").unwrap();

        assert!(guard.finish().unwrap(), "the run is marked");
        for gone in ["notes.md", "run.log", "scratch"] {
            assert!(!dir.join(gone).exists(), "{gone} is gone");
        }
        assert_eq!(fs::read_to_string(dir.join("a.ts")).unwrap(), "export {}\n");
        assert!(
            dir.join("ignored/installed").exists(),
            "what the preparation left stays"
        );
        assert_eq!(
            status(&dir).unwrap(),
            Guard::new(&dir).unwrap().before,
            "the next run starts from the status of the preparation"
        );
    }

    #[test]
    fn a_file_the_run_staged_is_removed_too() {
        let (_guard, dir) = repo();
        let guard = Guard::new(&dir).unwrap();
        fs::write(dir.join("staged.ts"), "export {}\n").unwrap();
        git(&dir, &["add", "staged.ts"]).unwrap();
        assert!(guard.finish().unwrap());
        assert!(!dir.join("staged.ts").exists());
    }

    #[test]
    fn a_run_that_leaves_the_checkout_alone_is_not_marked() {
        let (_guard, dir) = repo();
        let guard = Guard::new(&dir).unwrap();
        assert!(!guard.finish().unwrap());
    }

    /// Where HEAD is, as git names it: the commit, and the branch or `HEAD` itself when it is detached.
    fn head_of(dir: &Path) -> (String, String) {
        (
            git(dir, &["rev-parse", "HEAD"]).unwrap(),
            git(dir, &["rev-parse", "--symbolic-full-name", "HEAD"]).unwrap(),
        )
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A run that moves HEAD is marked and HEAD is put back"
    )]
    #[test]
    fn a_run_that_moves_head_is_marked_and_head_is_put_back() {
        let moves: [(&str, fn(&Path)); 5] = [
            ("commits", |dir| {
                fs::write(dir.join("committed.ts"), "export {}\n").unwrap();
                git(dir, &["add", "committed.ts"]).unwrap();
                commit(dir, "by the run");
            }),
            ("checks out another commit", |dir| {
                git(dir, &["checkout", "--quiet", "--detach", "HEAD~1"]).unwrap();
            }),
            ("checks out a branch of its own", |dir| {
                git(dir, &["checkout", "--quiet", "-b", "other", "HEAD~1"]).unwrap();
            }),
            ("resets to another commit", |dir| {
                git(dir, &["reset", "--hard", "--quiet", "HEAD~1"]).unwrap();
            }),
            ("commits on a branch of its own", |dir| {
                git(dir, &["checkout", "--quiet", "-b", "other"]).unwrap();
                commit(dir, "by the run");
            }),
        ];
        // The checkout of the runs is detached; one on a branch is put back on that branch.
        for detached in [true, false] {
            for (what, mv) in moves {
                let (_guard, dir) = repo();
                commit(&dir, "second");
                if detached {
                    git(&dir, &["checkout", "--quiet", "--detach"]).unwrap();
                }
                let before = head_of(&dir);
                let guard = Guard::new(&dir).unwrap();

                mv(&dir);

                let shown = format!("a run that {what}, detached: {detached}");
                assert!(guard.finish().unwrap(), "{shown} is marked");
                assert_eq!(head_of(&dir), before, "{shown}: HEAD is where it was");
                assert!(
                    !dir.join("committed.ts").exists(),
                    "{shown}: its file is gone"
                );
                assert_eq!(
                    git(&dir, &["status", "--porcelain"]).unwrap(),
                    "",
                    "{shown}: the tree is the commit"
                );
            }
        }
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A file that was there before the run is not removed with the files the run added"
    )]
    #[test]
    fn a_file_that_was_there_before_stays_when_the_run_adds_one_beside_it() {
        let (_guard, dir) = repo();
        // Every file of `logs/` is ignored, so a status shows the directory as one entry; `scratch/` is untracked and shown as one entry too.
        fs::create_dir(dir.join("logs")).unwrap();
        fs::write(dir.join("logs/old.log"), "kept").unwrap();
        fs::create_dir(dir.join("scratch")).unwrap();
        fs::write(dir.join("scratch/old.ts"), "kept").unwrap();
        let guard = Guard::new(&dir).unwrap();

        // The status then shows the files of `logs/` one by one, and `scratch/` as it was.
        fs::write(dir.join("logs/new.ts"), "added").unwrap();
        fs::write(dir.join("scratch/new.ts"), "added").unwrap();
        fs::create_dir_all(dir.join("ignored/deeper")).unwrap();
        fs::write(dir.join("ignored/deeper/new.ts"), "added").unwrap();

        assert!(guard.finish().unwrap(), "the run is marked");
        for kept in ["logs/old.log", "scratch/old.ts", "ignored/installed"] {
            assert!(dir.join(kept).exists(), "{kept} is still there");
        }
        for gone in ["logs/new.ts", "scratch/new.ts", "ignored/deeper"] {
            assert!(!dir.join(gone).exists(), "{gone} is gone");
        }
    }

    #[test]
    fn a_file_of_the_preparation_that_the_run_deleted_stops_the_task() {
        let (_guard, dir) = repo();
        let guard = Guard::new(&dir).unwrap();
        fs::remove_file(dir.join("ignored/installed")).unwrap();
        let error = guard.finish().unwrap_err();
        assert!(
            error.to_string().contains("could not be restored"),
            "{error}"
        );
    }
}
