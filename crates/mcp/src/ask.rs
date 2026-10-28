//! Answering a call: what each tool asks of the source and of the queries.
//!
//! The paths are read, and the changed files listed, before the index is refreshed, so that a call that cannot be
//! answered does not first pay for an index. The answer is the text the command line prints, produced by the same
//! functions in `gsttt-core`.

use gsttt_core::engine::changes::ChangeKind;
use gsttt_core::query::{self, Answer, Entries, ImpactRequest};
use gsttt_core::{Freshness, Index, Loaded};

use crate::arguments::{
    Call, ContractArguments, CoverageArguments, ImpactCall, NeighbourArguments, RouteArguments,
    Subject,
};
use crate::error::ToolError;
use crate::source::IndexSource;

/// What `index_status` says before the first index. It points at the questions that build one, because the tool does
/// not.
const NO_INDEX: &str = "There is no index yet: the first question builds one. Ask `impact`, `dependents` or \
                        `dependencies` about a file to build it, which can take a while on a large repository.";

/// Answers `call` from `source` as the text of one result.
///
/// # Errors
///
/// Whatever stops the call: a path the source refuses, a failed refresh, a contract query with nothing in it, a file the index does not hold.
pub(crate) fn run<S: IndexSource>(source: &S, call: Call) -> Result<String, ToolError> {
    match call {
        Call::Impact(call) => impact(source, call),
        Call::Dependents(arguments) => about_a_file(source, &arguments, query::dependents),
        Call::Dependencies(arguments) => about_a_file(source, &arguments, query::dependencies),
        Call::Path(RouteArguments { from, to, all }) => {
            let from = source.path(&from)?;
            let to = source.path(&to)?;
            let Loaded { index, freshness } = source.refreshed()?;
            let route = query::path(&index, freshness, from.path.as_str(), to.path.as_str());
            text_if_held(&route, Entries::from_flag(all))
        }
        Call::Contract(ContractArguments { query: wanted, all }) => {
            let Loaded { index, freshness } = source.refreshed()?;
            Ok(query::contract(&index, freshness, &wanted)?.text(Entries::from_flag(all)))
        }
        Call::Coverage(CoverageArguments { all }) => {
            let Loaded { index, freshness } = source.refreshed()?;
            Ok(query::coverage(&index, freshness).text(Entries::from_flag(all)))
        }
        Call::IndexStatus => status(source),
    }
}

/// A question about one file, which `dependents` and `dependencies` ask in the same way.
fn about_a_file<S: IndexSource, A: Answer>(
    source: &S,
    arguments: &NeighbourArguments,
    ask: fn(&Index, Freshness, &str) -> A,
) -> Result<String, ToolError> {
    let target = source.path(&arguments.path)?;
    let Loaded { index, freshness } = source.refreshed()?;
    text_if_held(
        &ask(&index, freshness, target.path.as_str()),
        Entries::from_flag(arguments.all),
    )
}

/// The text of `answer` showing `entries`, or, when the index does not hold a file it was asked about, the error that carries the text the command line prints for it.
fn text_if_held<A: Answer>(answer: &A, entries: Entries) -> Result<String, ToolError> {
    if answer.held() {
        Ok(answer.text(entries))
    } else {
        Err(ToolError::NotInIndex(answer.text(Entries::Capped)))
    }
}

/// The files `impact` is asked about, then the answer. A path with no file on disk is asked about as deleted, so that
/// the answer can still list what used to import it; so is a file that git lists as deleted.
fn impact<S: IndexSource>(source: &S, call: ImpactCall) -> Result<String, ToolError> {
    let ImpactCall {
        subject,
        depth,
        all,
    } = call;
    let mut request = ImpactRequest {
        depth,
        ..ImpactRequest::default()
    };
    match subject {
        Subject::Paths(arguments) => {
            for argument in &arguments {
                let target = source.path(argument)?;
                let named = if target.on_disk {
                    &mut request.files
                } else {
                    &mut request.deleted
                };
                named.push(target.path.into());
            }
        }
        Subject::Changed { base } => {
            for change in source.changed(base.as_deref())? {
                let named = match change.kind {
                    ChangeKind::Deleted => &mut request.deleted,
                    ChangeKind::Modified | ChangeKind::Added => &mut request.files,
                };
                named.push(change.path.into());
            }
        }
    }
    let Loaded { index, freshness } = source.refreshed()?;
    Ok(query::impact(&index, freshness, &request).text(Entries::from_flag(all)))
}

/// How the stored index was built and how it stands against the tree. This is the one tool that does not refresh.
fn status<S: IndexSource>(source: &S) -> Result<String, ToolError> {
    Ok(match source.stored()? {
        Some(loaded) => query::status(&loaded.index, loaded.freshness).text(Entries::Capped),
        None => NO_INDEX.to_owned(),
    })
}
