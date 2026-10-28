//! The purpose of a capability. The change that creates a capability gives it, and no spec keeps the placeholder that `openspec archive` writes when the delta has none, or a `## Purpose` with nothing under it.

use gsttt_spec::{Delta, OpenSpec, Requirement};

use super::proofs::Site;
use super::rules::Failure;

/// The start of the sentence that `openspec archive` writes as the purpose of a capability whose delta has no `## Purpose`. OpenSpec's own validator looks for the same two halves.
const PLACEHOLDER_START: &str = "TBD - created by archiving change ";
/// The end of that sentence; the name of the change sits between the two halves.
const PLACEHOLDER_END: &str = ". Update Purpose after archive.";

/// A spec whose purpose is empty or still the placeholder, and an open change, finished or not, whose delta would create a capability without giving its purpose, so that the spec it archives into would get the placeholder. The change is held back from `--ready`.
pub(super) fn failures(openspec: &OpenSpec) -> Vec<Failure> {
    let mut failures = Vec::new();
    for spec in &openspec.specs {
        let Some(purpose) = &spec.purpose else {
            continue;
        };
        let fault = if purpose.text.is_empty() {
            "an empty `## Purpose`"
        } else if is_placeholder(&purpose.text) {
            "the placeholder purpose that `openspec archive` writes when a delta gives none"
        } else {
            continue;
        };
        failures.push(Failure {
            site: Site {
                file: spec.file.clone(),
                line: purpose.line,
            },
            message: format!(
                "capability `{}` has {fault}; write what the capability is for",
                spec.capability
            ),
            changes: Vec::new(),
        });
    }
    for change in &openspec.changes {
        for delta in &change.deltas {
            let Some(first) = creation(openspec, delta) else {
                continue;
            };
            let (problem, remedy) = match &delta.purpose {
                None => (
                    "its delta has no `## Purpose`",
                    "add one above the first section",
                ),
                Some(purpose) if purpose.text.is_empty() => (
                    "its `## Purpose` is empty",
                    "write what the capability is for under it",
                ),
                Some(_) => continue,
            };
            failures.push(Failure {
                site: Site {
                    file: delta.file.clone(),
                    line: first.line,
                },
                message: format!(
                    "change `{}` creates capability `{}` and {problem}, so archiving it would write the placeholder purpose; {remedy}",
                    change.name, delta.capability
                ),
                changes: vec![change.name.clone()],
            });
        }
    }
    failures
}

/// The first requirement of `delta` when archiving it would create its capability: no spec has the capability, and the delta only adds to it. A delta that modifies, removes or renames needs the capability to exist already, so it is never the one that creates it.
fn creation<'a>(openspec: &OpenSpec, delta: &'a Delta) -> Option<&'a Requirement> {
    let only_adds =
        delta.modified.is_empty() && delta.removed.is_empty() && delta.renamed.is_empty();
    let missing = !openspec
        .specs
        .iter()
        .any(|spec| spec.capability == delta.capability);
    delta.added.first().filter(|_| only_adds && missing)
}

/// Whether `text` holds the sentence `openspec archive` writes: its two halves, in order.
fn is_placeholder(text: &str) -> bool {
    text.match_indices(PLACEHOLDER_END).any(|(end, _)| {
        text.get(..end)
            .is_some_and(|before| before.contains(PLACEHOLDER_START))
    })
}
