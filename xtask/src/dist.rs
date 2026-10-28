//! Releases and repository checks. Specified in `openspec/changes/add-distribution/`.
//!
//! Every task works on the repository in the current directory, which is the repository root when a task is run as `cargo xtask <task>`.

mod check;
mod git;
mod notes;
mod package;
mod version;

use std::process::ExitCode;

/// The tasks this module runs.
pub(crate) const TASKS: [&str; 4] = ["version", "package", "notes", "check"];

/// Runs `task` with its arguments.
pub(crate) fn run(task: &str, args: &[String]) -> anyhow::Result<ExitCode> {
    match task {
        "version" => version::run(args),
        "package" => package::run(args),
        "notes" => notes::run(args),
        "check" => check::run(args),
        other => anyhow::bail!("`cargo xtask {other}` is not a release task"),
    }
}
