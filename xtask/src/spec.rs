//! The spec gate: every scenario proven by exactly one test. Specified in `openspec/specs/spec-gate/`
//! and, until that change is archived, in `openspec/changes/add-spec-gate/`.
//!
//! - [`proofs`] finds the `#[scenario]` attributes in the sources of the workspace.
//! - [`rules`] decides which scenarios exist, which are proven and what fails.
//! - [`report`] prints the verdict.

mod proofs;
mod report;
mod rules;
#[cfg(test)]
mod tests;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context as _;
use xshell::{Shell, cmd};

/// What `cargo xtask spec` was asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Check everything and print the report.
    Report,
    /// Print the open changes that can be archived and nothing else.
    Ready,
}

/// The text the gate prints and whether the gate passed.
#[derive(Debug)]
struct Outcome {
    output: String,
    passed: bool,
}

/// `cargo xtask spec [--ready]`.
///
/// The exit code is non-zero exactly when the gate found something to fail. `--ready` only lists
/// changes, so it succeeds whatever else is wrong; the archive workflow runs the gate itself on the
/// archived specs.
pub(crate) fn run(args: &[String]) -> anyhow::Result<ExitCode> {
    let mode = match args {
        [] => Mode::Report,
        [flag] if flag == "--ready" => Mode::Ready,
        _ => anyhow::bail!("usage: cargo xtask spec [--ready]"),
    };
    let (root, members) = workspace()?;
    let outcome = execute(&root, &members, mode)?;
    std::io::stdout()
        .write_all(outcome.output.as_bytes())
        .context("writing the report")?;
    Ok(if outcome.passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Reads the specs under `root`, finds the proofs in the sources of `members` (directories,
/// absolute or relative to `root`) and judges them.
fn execute(root: &Path, members: &[PathBuf], mode: Mode) -> anyhow::Result<Outcome> {
    let openspec = gsttt_spec::read(root)?;
    let marks = proofs::find(root, members)?;
    let judgement = rules::judge(&openspec, &marks);
    Ok(match mode {
        Mode::Report => Outcome {
            output: report::render(&judgement),
            passed: judgement.passed(),
        },
        Mode::Ready => Outcome {
            output: report::ready(&judgement),
            passed: true,
        },
    })
}

/// The workspace root and the directories of its members, from `cargo metadata`.
fn workspace() -> anyhow::Result<(PathBuf, Vec<PathBuf>)> {
    let sh = Shell::new()?;
    let json = cmd!(sh, "cargo metadata --no-deps --format-version 1").read()?;
    let metadata: serde_json::Value =
        serde_json::from_str(&json).context("reading the output of cargo metadata")?;
    let root = metadata
        .get("workspace_root")
        .and_then(serde_json::Value::as_str)
        .context("cargo metadata names no workspace_root")?;
    let packages = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .context("cargo metadata lists no packages")?;
    let mut members = Vec::new();
    for package in packages {
        let manifest = package
            .get("manifest_path")
            .and_then(serde_json::Value::as_str)
            .context("a package has no manifest_path")?;
        let dir = Path::new(manifest)
            .parent()
            .context("a manifest path has no directory")?;
        members.push(dir.to_path_buf());
    }
    members.sort();
    Ok((PathBuf::from(root), members))
}
