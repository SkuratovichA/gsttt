//! X(file): what a file exports once its `export *` lines are followed, which the walk asks for to tell a name a barrel never had from one it passes on.

use std::borrow::Cow;
use std::collections::BTreeSet;

use crate::graph::Graph;
use crate::ids::{FileId, NameId};

/// Whether the exports of `file`, and of every file it re-exports everything from, are known, which is whether X(file) exists. No name is collected to tell, so a file with no `export *` line of its own costs a look at its imports, and a barrel a walk over its star lines.
pub(super) fn known(graph: &Graph, file: FileId) -> bool {
    if !is_barrel(graph, file) {
        return graph.exports(file).is_some();
    }
    visit_stars(graph, file, |member| graph.exports(member).is_some())
}

/// The names `file` exports itself and those it takes in through `export *`, in byte order and without repeats. `export *` never forwards `default`, so the default export of a file reached through a star is left out. `None` when the exports of `file`, or of a file it re-exports everything from, are unknown, because nothing can then be said about a name it does not list.
///
/// A file with no `export *` line of its own exports what it lists, so the list is borrowed from the graph, and only a barrel gets a set built.
///
/// The star lines are followed to a fixpoint with a visited set, so a cycle of them ends, and every member of the cycle gets the same names up to `default`.
///
/// The TypeScript linker follows the same stars to find where a name is defined (`export_of` in `crates/lang-ts/src/link.rs`). The two cannot share code, because the core knows no language, so tests in `crates/lang-ts/tests/typescript/exports.rs` pin them against each other.
pub(super) fn exported(
    graph: &Graph,
    file: FileId,
    default: Option<NameId>,
) -> Option<Cow<'_, [NameId]>> {
    if !is_barrel(graph, file) {
        return graph.exports(file).map(Cow::Borrowed);
    }
    let mut names = Vec::new();
    let complete = visit_stars(graph, file, |member| {
        let Some(listed) = graph.exports(member) else {
            return false;
        };
        names.extend(
            listed
                .iter()
                .copied()
                .filter(|&name| member == file || Some(name) != default),
        );
        true
    });
    if !complete {
        return None;
    }
    names.sort_unstable();
    names.dedup();
    Some(Cow::Owned(names))
}

/// The files `file` re-exports everything from.
fn star_targets(graph: &Graph, file: FileId) -> impl Iterator<Item = FileId> + '_ {
    graph
        .dependencies(file)
        .iter()
        .filter(|&&(_, edge)| graph.binding(edge).star)
        .map(|&(target, _)| target)
}

/// Whether `file` has an `export *` line of its own.
fn is_barrel(graph: &Graph, file: FileId) -> bool {
    star_targets(graph, file).next().is_some()
}

/// Calls `visit` on `file` and on every file an `export *` line leads to from it, once each, so a cycle of them ends. Stops at the first call that returns `false`, and says whether there was none.
fn visit_stars(graph: &Graph, file: FileId, mut visit: impl FnMut(FileId) -> bool) -> bool {
    let mut visited = BTreeSet::from([file]);
    let mut pending = vec![file];
    while let Some(member) = pending.pop() {
        if !visit(member) {
            return false;
        }
        for target in star_targets(graph, member) {
            if visited.insert(target) {
                pending.push(target);
            }
        }
    }
    true
}
