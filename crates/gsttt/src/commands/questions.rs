//! The questions `dependents`, `dependencies`, `path`, `contract` and `coverage`, which answer from the index.

use gsttt_core::query;

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

/// `gsttt coverage`: the problems of the index, every one of them, then what the connectors read and what they could not.
pub(super) fn coverage(session: &Session<'_>, io: &mut Io<'_>) -> Result<Outcome, CliError> {
    let Loaded { index, freshness } = session.loaded()?;
    print::answer(&query::coverage(&index, freshness), session.flags, io)
}
