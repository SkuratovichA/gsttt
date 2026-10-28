//! X(file): what a file exports once its `export *` lines are followed, which the walk asks for to tell a name a barrel never had from one it passes on.

use std::collections::BTreeSet;

use crate::graph::Graph;
use crate::ids::{FileId, NameId};

/// The names `file` exports itself and those it takes in through `export *`, in byte order. `export *` never forwards `default`, so the default export of a file reached through a star is left out. `None` when the exports of `file`, or of a file it re-exports everything from, are unknown, because nothing can then be said about a name it does not list.
///
/// The star lines are followed to a fixpoint with a visited set, so a cycle of them ends, and every member of the cycle gets the same names up to `default`.
pub(super) fn exported(
    graph: &Graph,
    file: FileId,
    default: Option<NameId>,
) -> Option<Vec<NameId>> {
    let mut names = graph.exports(file)?.to_vec();
    let mut visited = BTreeSet::from([file]);
    let mut pending = vec![file];
    while let Some(barrel) = pending.pop() {
        for &(target, edge) in graph.dependencies(barrel) {
            if !graph.binding(edge).star || !visited.insert(target) {
                continue;
            }
            names.extend(
                graph
                    .exports(target)?
                    .iter()
                    .copied()
                    .filter(|&name| Some(name) != default),
            );
            pending.push(target);
        }
    }
    names.sort_unstable();
    names.dedup();
    Some(names)
}
