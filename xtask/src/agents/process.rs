//! Running programs for the agent benchmarks: finding one on a PATH and ending a whole process group.

use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::Context as _;

/// The first executable file called `program` in the directories of `path`.
pub(super) fn which(program: &str, path: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(file: &Path) -> bool {
    fs::metadata(file).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Kills the process group of `child`, which was started with `process_group(0)`, and waits for the child. A test runner, a language server or an MCP server starts processes of its own, and std has no call for a group.
pub(super) fn kill_group(child: &mut Child) -> anyhow::Result<()> {
    // A group with no process left is the usual case after a run that ended on its own, and `kill` says so on its standard error.
    Command::new("kill")
        .args(["-s", "KILL", "--"])
        .arg(format!("-{}", child.id()))
        .stderr(Stdio::null())
        .status()
        .context("cannot run kill")?;
    child.wait()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_found_in_the_first_directory_that_holds_it() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        for dir in [&first, &second] {
            let program = dir.path().join("tool");
            fs::write(&program, "#!/bin/sh\n").unwrap();
            fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = std::env::join_paths([first.path(), second.path()]).unwrap();
        assert_eq!(which("tool", &path), Some(first.path().join("tool")));
        assert_eq!(which("other", &path), None);
    }

    #[test]
    fn a_file_that_is_not_executable_is_not_a_program() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("tool"), "text").unwrap();
        let path = std::env::join_paths([dir.path()]).unwrap();
        assert_eq!(which("tool", &path), None);
    }
}
