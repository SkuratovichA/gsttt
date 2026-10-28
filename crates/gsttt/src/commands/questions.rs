//! The questions `dependents`, `dependencies`, `path`, `contract` and `coverage`, which answer from the index.

use std::fmt;

use gsttt_core::query::{self, Answer, CoverageReport, Entries};
use gsttt_core::text::thousands;
use serde::Serialize;

use super::{Session, print};
use crate::cli::{ContractArgs, FileArgs, PathArgs};
use crate::error::CliError;
use crate::repository::Loaded;
use crate::streams::{Io, Outcome};

/// `gsttt dependents <path>`: the files that import one file.
pub(super) fn dependents(
    session: &Session<'_>,
    args: &FileArgs,
    io: &mut Io<'_>,
) -> Result<Outcome, CliError> {
    let file = session.argument(&args.path)?;
    let Loaded { index, freshness } = session.loaded()?;
    let answer = query::dependents(&index, freshness, file.path.as_str());
    print::answer_if(answer.in_index, &answer, session.flags, io)
}

/// `gsttt dependencies <path>`: what one file imports.
pub(super) fn dependencies(
    session: &Session<'_>,
    args: &FileArgs,
    io: &mut Io<'_>,
) -> Result<Outcome, CliError> {
    let file = session.argument(&args.path)?;
    let Loaded { index, freshness } = session.loaded()?;
    let answer = query::dependencies(&index, freshness, file.path.as_str());
    print::answer_if(answer.in_index, &answer, session.flags, io)
}

/// `gsttt path <from> <to>`: a shortest chain from one file to another.
pub(super) fn path(
    session: &Session<'_>,
    args: &PathArgs,
    io: &mut Io<'_>,
) -> Result<Outcome, CliError> {
    let from = session.argument(&args.from)?;
    let to = session.argument(&args.to)?;
    let Loaded { index, freshness } = session.loaded()?;
    let route = query::path(&index, freshness, from.path.as_str(), to.path.as_str());
    print::answer_if(route.unknown.is_empty(), &route, session.flags, io)
}

/// `gsttt contract <query>...`: the sites of a contract, or the contracts whose keys contain the query.
pub(super) fn contract(
    session: &Session<'_>,
    args: &ContractArgs,
    io: &mut Io<'_>,
) -> Result<Outcome, CliError> {
    let Loaded { index, freshness } = session.loaded()?;
    let lookup =
        query::contract(&index, freshness, &args.text()).map_err(|_| CliError::EmptyQuery)?;
    print::answer(&lookup, session.flags, io)
}

/// `gsttt coverage`: what the connectors read and what they could not.
pub(super) fn coverage(session: &Session<'_>, io: &mut Io<'_>) -> Result<Outcome, CliError> {
    let Loaded { index, freshness } = session.loaded()?;
    let answer = Coverage {
        report: query::coverage(&index, freshness),
        problems: index.coverage.problems.clone(),
    };
    print::answer(&answer, session.flags, io)
}

/// The answer of `gsttt coverage`: the report of the engine and, before it, the problems of the index, which the
/// report does not carry. One of them is an external connector that waits for trust, which is the only place the
/// command line tells a person to run `gsttt config trust`.
// When `CoverageReport` gains a `problems` field of its own, this type has to go: the document would hold the key
// twice. `the_coverage_document_holds_problems_once` in `tests/cli/trust.rs` fails then.
#[derive(Debug, Serialize)]
struct Coverage {
    #[serde(flatten)]
    report: CoverageReport,
    /// Connector calls that failed outside the parse of one file, and externals that wait for trust, one line each.
    problems: Vec<String>,
}

impl Answer for Coverage {
    fn write_text(&self, f: &mut fmt::Formatter<'_>, entries: Entries) -> fmt::Result {
        if !self.problems.is_empty() {
            writeln!(f, "problems ({}):", thousands(self.problems.len()))?;
            for problem in &self.problems {
                writeln!(f, "  {problem}")?;
            }
        }
        self.report.write_text(f, entries)
    }
}

impl fmt::Display for Coverage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_text(f, Entries::Capped)
    }
}
