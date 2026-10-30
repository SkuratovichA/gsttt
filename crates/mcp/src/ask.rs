//! Answering a call: what each tool asks of the source and of the queries.
//!
//! The paths are read, and the changed files listed, before the index is refreshed, so that a call that cannot be answered does not first pay for an index. The answer is the text the command line prints, produced by the same functions in `gsttt-core`.

use gsttt_core::engine::changes::ChangeKind;
use gsttt_core::query::{self, Answer, Entries, ImpactRequest, LIST_CAP};
use gsttt_core::text::thousands;
use gsttt_core::{Freshness, Index, Loaded};

use crate::arguments::{
    Call, ContractArguments, CoverageArguments, ImpactCall, NeighbourArguments, RouteArguments,
    Subject,
};
use crate::error::ToolError;
use crate::source::IndexSource;

/// The most characters an answer holds: it stays under Claude Code's warning at 10,000 tokens, and at half the 50,000 characters above which Claude Code saves a result to a file.
pub const BUDGET: usize = 25_000;

/// What `index_status` says before the first index. It points at the questions that build one, because the tool does not.
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
            let answer = query::contract(&index, freshness, &wanted)?;
            Ok(within_budget(&answer, Entries::from_flag(all)))
        }
        Call::Coverage(CoverageArguments { all }) => {
            let Loaded { index, freshness } = source.refreshed()?;
            Ok(within_budget(
                &query::coverage(&index, freshness),
                Entries::from_flag(all),
            ))
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

/// The text of `answer` showing `entries`, or, when that is longer than [`BUDGET`] characters, the most entries of each list that fit, after a line that says the lists were shortened. Every answer text the server returns passes through here, so that no result is long enough for a client to save to a file.
fn within_budget<A: Answer>(answer: &A, entries: Entries) -> String {
    let text = answer.text(entries);
    let size = text.chars().count();
    if size <= BUDGET {
        return text;
    }
    let line = format!(
        "cut: lists shortened to fit this answer in {} characters; the \"more\" counts say what they leave out. \
         Ask a narrower question for the rest: fewer files, or a smaller depth.",
        thousands(BUDGET)
    );
    let cut_to = |shown: usize| format!("{line}\n{}", answer.text(Entries::First(shown)));
    let fits = |shown: usize| cut_to(shown).chars().count() <= BUDGET;

    // Every entry shown takes a character at least, so no list is longer than the uncut text.
    let upper = match entries {
        Entries::Capped => LIST_CAP,
        Entries::First(shown) => shown,
        Entries::All => size,
    };
    if fits(upper) {
        return cut_to(upper);
    }
    // Only the problems of a coverage answer are never cut, and they alone can fill the budget: no list is left to shorten, and the text goes out as it is.
    if !fits(0) {
        return cut_to(0);
    }
    // `low` fits and `high` does not, so the last `low` is a count that fits where one entry more does not. A larger count can give a shorter text, as a `… N more` line goes away, so this finds a boundary and not always the largest count; it finds the same one for the same answer.
    let (mut low, mut high) = (0, upper);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if fits(middle) {
            low = middle;
        } else {
            high = middle;
        }
    }
    cut_to(low)
}

/// The text of `answer` showing `entries`, or, when the index does not hold a file it was asked about, the error that carries the text the command line prints for it.
fn text_if_held<A: Answer>(answer: &A, entries: Entries) -> Result<String, ToolError> {
    if answer.held() {
        Ok(within_budget(answer, entries))
    } else {
        Err(ToolError::NotInIndex(answer.text(Entries::Capped)))
    }
}

/// The files `impact` is asked about, then the answer. A path with no file on disk is asked about as deleted, so that the answer can still list what used to import it; so is a file that git lists as deleted.
fn impact<S: IndexSource>(source: &S, call: ImpactCall) -> Result<String, ToolError> {
    let ImpactCall {
        subject,
        depth,
        all,
        tests,
    } = call;
    let mut request = ImpactRequest {
        depth,
        no_tests: !tests,
        ..ImpactRequest::default()
    };
    match subject {
        Subject::Paths { paths, names } => {
            for argument in &paths {
                let target = source.path(argument)?;
                let named = if target.on_disk {
                    &mut request.files
                } else {
                    &mut request.deleted
                };
                named.push(target.path.into());
            }
            request.names = names;
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
    Ok(within_budget(
        &query::impact(&index, freshness, &request),
        Entries::from_flag(all),
    ))
}

/// How the stored index was built and how it stands against the tree. This is the one tool that does not refresh.
fn status<S: IndexSource>(source: &S) -> Result<String, ToolError> {
    Ok(match source.stored()? {
        Some(loaded) => within_budget(
            &query::status(&loaded.index, loaded.freshness),
            Entries::Capped,
        ),
        None => NO_INDEX.to_owned(),
    })
}
