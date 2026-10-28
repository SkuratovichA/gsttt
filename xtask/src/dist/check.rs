//! `cargo xtask check`: checks of the repository that no other tool makes.
//!
//! - Every action a workflow uses is pinned by a full commit id (`actions`).
//! - The plugin marketplace and the manifests of the plugins it lists parse, and every path they name exists (`plugin`).

mod actions;
mod plugin;

use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use anyhow::bail;

use crate::root::repository_root;

/// Checks the repository and prints every problem, one per line.
pub(super) fn run(args: &[String]) -> anyhow::Result<ExitCode> {
    if !args.is_empty() {
        bail!("usage: cargo xtask check");
    }
    let problems = problems(&repository_root()?)?;
    if problems.is_empty() {
        writeln!(std::io::stdout(), "cargo xtask check: no problems")?;
        return Ok(ExitCode::SUCCESS);
    }
    let mut stderr = std::io::stderr().lock();
    for problem in &problems {
        writeln!(stderr, "{problem}")?;
    }
    writeln!(stderr, "cargo xtask check: {} problem(s)", problems.len())?;
    Ok(ExitCode::FAILURE)
}

/// Every problem of the repository in `root`, in a fixed order.
fn problems(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut problems = actions::problems(root)?;
    problems.extend(plugin::problems(root)?);
    Ok(problems)
}
