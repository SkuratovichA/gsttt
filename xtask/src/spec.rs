//! The spec gate: every scenario proven by exactly one test. Specified in `openspec/changes/add-spec-gate/`.

use std::process::ExitCode;

/// `cargo xtask spec [--ready]`.
pub(crate) fn run(_args: &[String]) -> anyhow::Result<ExitCode> {
    anyhow::bail!("the spec gate is not built yet")
}
