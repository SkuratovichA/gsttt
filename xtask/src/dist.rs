//! Releases and repository checks. Specified in `openspec/changes/add-distribution/`.

use std::process::ExitCode;

/// The tasks this module runs.
pub(crate) const TASKS: [&str; 4] = ["version", "package", "notes", "check"];

/// Runs `task` with its arguments.
pub(crate) fn run(task: &str, _args: &[String]) -> anyhow::Result<ExitCode> {
    anyhow::bail!("`cargo xtask {task}` is not built yet")
}
