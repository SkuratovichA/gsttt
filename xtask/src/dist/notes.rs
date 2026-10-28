//! `cargo xtask notes`: the notes of a release.

use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use anyhow::bail;

use super::git::git;
use crate::root::repository_root;

/// Prints the subjects of the commits since the previous release.
pub(super) fn run(args: &[String]) -> anyhow::Result<ExitCode> {
    if !args.is_empty() {
        bail!("usage: cargo xtask notes");
    }
    let notes = release_notes(&repository_root()?)?;
    std::io::stdout().write_all(notes.as_bytes())?;
    Ok(ExitCode::SUCCESS)
}

/// One `- <subject>` line per commit since the latest release tag, newest first. Without a release tag it is every commit.
fn release_notes(root: &Path) -> anyhow::Result<String> {
    // The newest version among the release tags in the history, so a tag that is not on this branch is not "previous".
    let tags = git(
        root,
        &[
            "tag",
            "--list",
            "v[0-9]*",
            "--merged",
            "HEAD",
            "--sort=-version:refname",
        ],
    )?;
    let range = match tags.lines().next() {
        Some(tag) => format!("{tag}..HEAD"),
        None => "HEAD".to_owned(),
    };
    let subjects = git(root, &["log", "--no-show-signature", "--format=%s", &range])?;
    let mut notes = String::new();
    for subject in subjects.lines() {
        notes.push_str("- ");
        notes.push_str(subject);
        notes.push('\n');
    }
    Ok(notes)
}
