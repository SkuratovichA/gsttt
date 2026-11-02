//! The tests that replace a module: a test that swaps a module for a factory mock runs none of that module's code, so a change reaches the test only by the imports that never enter one of the modules it replaces.

use crate::fact::ImportFlags;
use crate::ids::{FileId, IdSet};
use crate::index::Index;

/// The modules `test` replaces, sorted and once each: the files its factory mocks stand in for, without those it also loads as they are through an actual-module call anywhere in the test, which run their own code whatever the mock says. They are read from the imports of the test itself, because the edge of the graph keeps a flag only when every import behind it carries it. A test that replaces nothing allocates nothing.
fn replaced_by(index: &Index, test: FileId) -> Vec<FileId> {
    let Some(record) = index.record(test) else {
        return Vec::new();
    };
    let with = |flag: ImportFlags| {
        record
            .imports
            .iter()
            .filter(move |resolved| resolved.import.flags.contains(flag))
            .flat_map(|resolved| resolved.to.files().iter().copied())
    };
    let mut replaced: Vec<FileId> = with(ImportFlags::REPLACES).collect();
    if replaced.is_empty() {
        return replaced;
    }
    let actual: Vec<FileId> = with(ImportFlags::ACTUAL).collect();
    replaced.retain(|file| !actual.contains(file));
    replaced.sort_unstable();
    replaced.dedup();
    replaced
}

/// Which tests of an answer a replaced module cuts off from the change.
pub(super) struct Cut<'a> {
    index: &'a Index,
    changed: &'a [FileId],
    across: &'a [FileId],
    /// Where a change enters a test: the changed files, and every file the walk met across a contract, which the test runs whatever it replaces. Built for the first test that replaces a module and not before.
    entries: Option<IdSet<FileId>>,
}

impl<'a> Cut<'a> {
    /// A cut for the tests of a walk that started from `changed` and met `across` while it crossed contracts.
    pub(super) fn new(index: &'a Index, changed: &'a [FileId], across: &'a [FileId]) -> Self {
        Self {
            index,
            changed,
            across,
            entries: None,
        }
    }

    /// Whether `test` still runs a changed file or a file met across a contract: it replaces no module, or it reaches an entry by imports that never enter a module it replaces. A changed test is an entry itself.
    pub(super) fn keeps(&mut self, test: FileId) -> bool {
        let replaced = replaced_by(self.index, test);
        if replaced.is_empty() {
            return true;
        }
        let graph = &self.index.graph;
        let (changed, across) = (self.changed, self.across);
        let entries = self.entries.get_or_insert_with(|| {
            let mut entries = IdSet::with_universe(graph.file_count());
            entries.extend(changed.iter().chain(across).copied());
            entries
        });
        graph.reaches_avoiding(test, entries, &replaced)
    }
}
