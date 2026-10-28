//! The status of the checkout of the runs: a run must leave it as the preparation left it, and one that did not is undone before the next.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::ensure;

use crate::dist::git::git;

/// One entry of `git status --porcelain --ignored`: the two letters of the status and the path.
type Entry = (String, String);

/// The status of a checkout before a run.
#[derive(Debug)]
pub(super) struct Guard {
    dir: PathBuf,
    before: BTreeSet<Entry>,
}

impl Guard {
    /// Takes the status of the checkout in `dir`.
    pub(super) fn new(dir: &Path) -> anyhow::Result<Self> {
        Ok(Self {
            dir: dir.to_path_buf(),
            before: status(dir)?,
        })
    }

    /// Whether the run changed the checkout. When it did, the tracked files are reset and what the run added, ignored files included, is removed; it fails when the checkout is still not as it was.
    pub(super) fn finish(self) -> anyhow::Result<bool> {
        if status(&self.dir)? == self.before {
            return Ok(false);
        }
        git(&self.dir, &["reset", "--hard", "--quiet"])?;
        // Read again after the reset: a file the run added to the index is untracked once more, and only now seen as new.
        let added: Vec<String> = status(&self.dir)?
            .difference(&self.before)
            .filter(|(code, _)| code == "??" || code == "!!")
            .map(|(_, path)| path.clone())
            .collect();
        if !added.is_empty() {
            let mut args = vec!["--literal-pathspecs", "clean", "-ffdxq", "--"];
            args.extend(added.iter().map(String::as_str));
            git(&self.dir, &args)?;
        }
        ensure!(
            status(&self.dir)? == self.before,
            "the checkout {} is not as the preparation left it, and could not be restored",
            self.dir.display()
        );
        Ok(true)
    }
}

/// The entries of `git status --porcelain --ignored`. A directory that is wholly new or wholly ignored is one entry, so what a run changes inside an ignored directory that was there before is not seen.
fn status(dir: &Path) -> anyhow::Result<BTreeSet<Entry>> {
    let text = git(dir, &["status", "--porcelain=v1", "-z", "--ignored"])?;
    Ok(text
        .split('\0')
        .filter_map(|record| {
            let (code, path) = record.split_at_checked(2)?;
            Some((
                code.to_owned(),
                path.strip_prefix(' ').unwrap_or(path).to_owned(),
            ))
        })
        .collect())
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
    git(
        &dir,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "init",
        ],
    )
    .unwrap();
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
}
