//! The directory Claude Code makes for a session in the temporary root that every session of the user shares.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, ensure};

/// The temporary root of the Claude Code sessions of this user: `/tmp/claude-<uid>`, which is `/private/tmp/claude-<uid>` on macOS. It holds a directory per project, named after the working directory of its sessions, and a directory per session below it.
pub(super) fn root() -> PathBuf {
    PathBuf::from(format!(
        "/tmp/claude-{}",
        rustix::process::getuid().as_raw()
    ))
}

/// Removes the directory of the session `id`, which is `<root>/<project>/<id>` for the project it ran in, and nothing else in `root`: the sessions of the user's other work are there too. The id is read from the stream of a run, so one that is not a plain id removes nothing and is an error, and a link is never followed.
pub(super) fn remove(root: &Path, id: &str) -> anyhow::Result<()> {
    ensure!(
        !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "the session id `{id}` is not a plain id, so no directory is removed for it"
    );
    let projects = match fs::read_dir(root) {
        Ok(projects) => projects,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).with_context(|| format!("cannot read {}", root.display())),
    };
    for project in projects {
        let project = project.with_context(|| format!("cannot read {}", root.display()))?;
        let session = project.path().join(id);
        let is_project = project.file_type().is_ok_and(|kind| kind.is_dir());
        if is_project && fs::symlink_metadata(&session).is_ok_and(|meta| meta.is_dir()) {
            fs::remove_dir_all(&session)
                .with_context(|| format!("cannot remove {}", session.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    /// A session directory with a file in it.
    fn session(root: &Path, project: &str, id: &str) -> PathBuf {
        let dir = root.join(project).join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("note"), "").unwrap();
        dir
    }

    #[test]
    fn the_directories_of_the_session_are_removed_and_nothing_else() {
        let root = tempfile::tempdir().unwrap();
        let mine = [
            session(root.path(), "-a", "abc-123"),
            session(root.path(), "-b", "abc-123"),
        ];
        let others = [
            session(root.path(), "-a", "abc-124"),
            session(root.path(), "-c", "xabc-123"),
        ];
        fs::write(root.path().join("stray"), "").unwrap();

        remove(root.path(), "abc-123").unwrap();

        for gone in &mine {
            assert!(!gone.exists(), "{}", gone.display());
        }
        for kept in &others {
            assert!(kept.join("note").exists(), "{}", kept.display());
        }
        assert!(root.path().join("stray").exists());
    }

    #[test]
    fn a_session_without_a_directory_and_a_root_that_is_not_there_are_fine() {
        let root = tempfile::tempdir().unwrap();
        let kept = session(root.path(), "-a", "other");
        remove(root.path(), "abc-123").unwrap();
        assert!(kept.join("note").exists());
        remove(&root.path().join("not-there"), "abc-123").unwrap();
    }

    #[test]
    fn an_id_that_is_not_a_plain_id_removes_nothing() {
        let root = tempfile::tempdir().unwrap();
        let kept = session(root.path(), "-a", "x");
        for id in ["", ".", "..", "../x", "-a/x", "a b", "a\n"] {
            let error = remove(root.path(), id).unwrap_err();
            assert!(error.to_string().contains("not a plain id"), "{id:?}");
            assert!(kept.join("note").exists(), "{id:?}");
        }
    }

    #[test]
    fn a_link_is_not_followed() {
        let root = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        // A project that is a link to a directory with a session of that id in it, and a session that is a link.
        let behind_a_link = session(elsewhere.path(), "project", "abc-123");
        symlink(elsewhere.path().join("project"), root.path().join("-link")).unwrap();
        let target = elsewhere.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("note"), "").unwrap();
        fs::create_dir(root.path().join("-b")).unwrap();
        symlink(&target, root.path().join("-b/abc-123")).unwrap();

        remove(root.path(), "abc-123").unwrap();

        assert!(behind_a_link.join("note").exists());
        assert!(target.join("note").exists());
    }
}
