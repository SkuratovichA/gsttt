//! The tests that replace a module: a test that swaps a module for a factory mock runs none of that module's code, so a change reaches the test only by the imports that never enter one of the modules it replaces.

use crate::fact::{Import, ImportFlags, ImportKind};
use crate::ids::{FileId, IdSet};
use crate::index::Index;

/// The modules `test` replaces, sorted and once each: the files its factory mocks stand in for, without those it also takes in a way that may run their code, whatever call made it. An import that asks for the module as it is, through an actual-module call, does; so does any other mock call that carries no `REPLACES`, which is a mock made without a factory, an unmock, `importMock`, `createMockFromModule` and the like, because the index does not tell what such a call loads. They are read from the imports of the test itself, because the edge of the graph keeps a flag only when every import behind it carries it. A test that replaces nothing allocates nothing.
fn replaced_by(index: &Index, test: FileId) -> Vec<FileId> {
    let Some(record) = index.record(test) else {
        return Vec::new();
    };
    let targets = |wanted: fn(&Import) -> bool| {
        record
            .imports
            .iter()
            .filter(move |resolved| wanted(&resolved.import))
            .flat_map(|resolved| resolved.to.files().iter().copied())
    };
    let mut replaced: Vec<FileId> =
        targets(|import| import.flags.contains(ImportFlags::REPLACES)).collect();
    if replaced.is_empty() {
        return replaced;
    }
    let unsure: Vec<FileId> = targets(|import| {
        import.flags.contains(ImportFlags::ACTUAL)
            || (import.kind == ImportKind::Mock && !import.flags.contains(ImportFlags::REPLACES))
    })
    .collect();
    replaced.retain(|file| !unsure.contains(file));
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

    /// Whether `test` still runs a changed file or a file met across a contract: it replaces no module, or it reaches an entry by imports that never enter a module it replaces. The walk never reaches a changed file, so a changed test is never asked about.
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
