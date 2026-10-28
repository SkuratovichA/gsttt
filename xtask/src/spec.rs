//! The spec gate: every scenario proven by exactly one test. Specified in `openspec/specs/spec-gate/`
//! and, until that change is archived, in `openspec/changes/add-spec-gate/`.
//!
//! - [`workspace`] asks cargo which targets the workspace has.
//! - [`proofs`] finds the `#[scenario]` attributes in the sources those targets compile.
//! - [`purpose`] holds back a spec with the placeholder purpose and the change that would leave one.
//! - [`rules`] decides which scenarios exist, which are proven and what fails.
//! - [`report`] prints the verdict.

mod proofs;
mod purpose;
mod report;
mod rules;
#[cfg(test)]
mod tests;
mod workspace;

use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use anyhow::Context as _;

use workspace::Workspace;

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

impl Outcome {
    /// Non-zero exactly when the gate failed.
    fn exit_code(&self) -> ExitCode {
        if self.passed {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }
}

/// `cargo xtask spec [--ready]`.
///
/// The exit code is non-zero exactly when the gate found something to fail. `--ready` only lists
/// changes, so it succeeds whatever else is wrong; the archive workflow runs the gate itself on the
/// archived specs.
pub(crate) fn run(args: &[String]) -> anyhow::Result<ExitCode> {
    let mode = mode(args)?;
    let workspace = Workspace::load(Path::new("."))?;
    let outcome = execute(&workspace, mode)?;
    std::io::stdout()
        .write_all(outcome.output.as_bytes())
        .context("writing the report")?;
    Ok(outcome.exit_code())
}

/// The mode that the arguments after `spec` ask for.
fn mode(args: &[String]) -> anyhow::Result<Mode> {
    match args {
        [] => Ok(Mode::Report),
        [flag] if flag == "--ready" => Ok(Mode::Ready),
        _ => anyhow::bail!("usage: cargo xtask spec [--ready]"),
    }
}

/// Reads the specs of the workspace, finds the proofs in its sources and judges them.
fn execute(workspace: &Workspace, mode: Mode) -> anyhow::Result<Outcome> {
    let openspec = gsttt_spec::read(&workspace.root)?;
    let marks = proofs::find(workspace)?;
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
