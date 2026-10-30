//! The fixpoint behind `impact`: every file a change reaches, how many hops away, and the first way it got there.

mod exports;
mod names;

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};

use foldhash::fast::RandomState;

use crate::contract::Role;
use crate::graph::{Edge, Graph, Uses};
use crate::ids::{ContractId, FileId, IdSet, NameId};

use super::cross::{Crossed, Crossing, Direction};

pub(super) use names::{Hit, Note, reach_names};

/// How the walk got to a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Cause {
    /// The file imports the file the walk came from. `names` are the names it imports that the change can reach, sorted, and none when it takes every name.
    Imports { file: FileId, names: Vec<NameId> },
    /// The file re-exports `names` of the file the walk came from, each as the name it has there and the name this file exports it as, sorted by the first. There are no names when it passes none on, because the file it came from has nothing left to export through a star.
    ReExports {
        file: FileId,
        names: Vec<(NameId, NameId)>,
    },
    /// The file has a site with `role` in `contract`, which joins a contract of the file the walk came from.
    Contract { role: Role, contract: ContractId },
    /// A walk by name only: the file declares the names the walk came from, and its own declarations refer to them. `names` are the declarations asked about, in byte order.
    Declares { names: Vec<String> },
}

/// A file the walk reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Reached {
    pub(super) file: FileId,
    /// Hops from the nearest changed file.
    pub(super) distance: u32,
    /// The first way the walk found. Of the equally near ways it is the one from the file with the smallest path among those one hop nearer; from that file an import comes before a contract, contracts go in key order and the sites of a contract by line.
    pub(super) cause: Cause,
}

/// The names of a file that the walk meets for the first time in a layer, which is what its importers are measured against.
enum Delta {
    /// Every name: the file is changed, or the walk has just found it affected in all its names.
    All,
    /// Only these names, sorted, and none at all for a barrel that a star reached while it has nothing to pass on: its importers are still checked against the names it does not export.
    Names(Vec<NameId>),
}

/// The names of a barrel the walk has reached while the barrel is not yet affected in all its names.
#[derive(Default)]
struct Held {
    /// A(file): every name of the barrel that the change may reach.
    names: BTreeSet<NameId>,
    /// The names that joined `names` since the layer was cut, in the order they came.
    growth: Vec<NameId>,
}

/// Every file that reaches one of `changed` within `depth` hops, ordered by distance and then by file.
///
/// A hop goes from a file to the files that import it, except where the import is only a Rust `mod` declaration, which owns the child file and uses nothing from it; or across a contract: a file that declares one reaches the files that serve or call it, and a file that serves one reaches the files that call it. The changed files are never reached.
///
/// An import that records names is followed by name. A file that uses a name the change can reach is reached in all its names. A file that only re-exports is reached in the names it passes on, and passes on those and no others, so an `export *` barrel stops fanning out to importers of names the change cannot touch. A star re-export always reaches its importer, and holds it with no names when there is nothing to pass on. A file whose names do not grow is not visited again.
pub(super) fn reach(
    graph: &Graph,
    crossing: &Crossing<'_>,
    changed: &[FileId],
    depth: Option<u32>,
) -> Vec<Reached> {
    let mut walk = Walk {
        graph,
        crossing,
        crossed: Crossed::new(Direction::ToCallers, graph),
        seen: IdSet::with_universe(graph.file_count()),
        whole: IdSet::with_universe(graph.file_count()),
        held: HashMap::default(),
        exported: HashMap::default(),
        default: graph.name_id("default"),
        star: graph.name_id("*"),
        distance: 0,
        reached: Vec::new(),
        next: Vec::new(),
    };
    let mut changed = changed.to_vec();
    changed.sort_unstable();
    changed.dedup();
    walk.seen.extend(changed.iter().copied());
    walk.whole.extend(changed.iter().copied());
    let mut frontier: Vec<(FileId, Delta)> =
        changed.into_iter().map(|file| (file, Delta::All)).collect();
    while !frontier.is_empty() && depth.is_none_or(|limit| walk.distance < limit) {
        walk.distance += 1;
        for (file, delta) in &frontier {
            walk.expand(*file, delta);
        }
        frontier = walk.cut();
    }
    walk.reached
        .sort_unstable_by_key(|reached| (reached.distance, reached.file));
    walk.reached
}

struct Walk<'a> {
    graph: &'a Graph,
    crossing: &'a Crossing<'a>,
    /// The contracts crossed so far, from declarers and servers towards callers.
    crossed: Crossed,
    /// The files reached in at least one name, and the changed files.
    seen: IdSet<FileId>,
    /// The files reached in every name: the changed files, and those that use an affected name or sit across a contract. Only these cross contracts.
    whole: IdSet<FileId>,
    /// The seen files that are not whole. Probed and never iterated, so the order of a hash map cannot reach the output.
    held: HashMap<FileId, Held, RandomState>,
    /// X(file) per file that was asked for, kept because a barrel is asked about once per layer that grows it. A file with no `export *` line of its own holds the list of the graph and builds nothing.
    exported: HashMap<FileId, Option<Cow<'a, [NameId]>>, RandomState>,
    /// The ids of the names `export *` never forwards and `export * as ns` takes, when the graph holds them.
    default: Option<NameId>,
    star: Option<NameId>,
    /// The distance of the files being reached now.
    distance: u32,
    reached: Vec<Reached>,
    /// The files to expand in the next layer: those that were reached in some names for the first time or whose names grew in this layer, and those that became whole.
    next: Vec<FileId>,
}

impl Walk<'_> {
    fn expand(&mut self, file: FileId, delta: &Delta) {
        let graph = self.graph;
        // Whether the exports of `file` are known, which every importer of it that is not plain asks when the walk reaches the file whole. The answer is the same for all of them, so it is worked out once, and not at all when none asks.
        let mut known = None;
        for &(importer, edge) in graph.dependents(file) {
            if edge.is_structural() || self.whole.contains(importer) {
                continue;
            }
            // Every import of a connector that records no names is plain and lands here, at the cost of a bit per file. So does an import of a whole file whose exports are unknown: it could lose or pass on any name, so over such an edge the importer is followed at file level, whatever names the edge carries.
            let at_file_level = edge.is_plain()
                || (matches!(delta, Delta::All)
                    && !*known.get_or_insert_with(|| exports::known(graph, file)));
            if at_file_level {
                self.take_all(
                    importer,
                    Cause::Imports {
                        file,
                        names: Vec::new(),
                    },
                );
            } else {
                self.follow(importer, file, edge, delta);
            }
        }
        // A file reached in some names only has not been shown to touch its contracts.
        if matches!(delta, Delta::All) {
            for &contract in graph.contracts_of(file) {
                for (meeting, site) in self.crossing.cross(&mut self.crossed, file, contract) {
                    let cause = Cause::Contract {
                        role: site.role,
                        contract: meeting,
                    };
                    self.take_all(site.file, cause);
                }
            }
        }
    }

    /// What the importer gets from the new names of `file` over `edge`. A use comes before a pass, so the first way a file is found over one edge is the import.
    fn follow(&mut self, importer: FileId, file: FileId, edge: Edge, delta: &Delta) {
        let graph = self.graph;
        let binding = graph.binding(edge);

        // The names the importer uses itself. One of them is enough to affect it in all of its names.
        let mut used = Vec::new();
        let uses_all = match (binding.uses, delta) {
            (Uses::All, _) => true,
            (Uses::Names(wanted), Delta::All) => {
                used.extend_from_slice(wanted);
                false
            }
            (Uses::Names(wanted), Delta::Names(fresh)) => {
                for &name in wanted {
                    if self.reaches(file, fresh, name) {
                        used.push(name);
                    }
                }
                false
            }
        };
        if uses_all || !used.is_empty() {
            self.take_all(importer, Cause::Imports { file, names: used });
            return;
        }

        // The names it passes on, which affect it only in those names, each with the name it has in `file`.
        let mut passed: Vec<(NameId, NameId)> = Vec::new();
        for &(exported_as, taken) in binding.passes {
            let reached = match delta {
                Delta::All => true,
                Delta::Names(fresh) => Some(taken) == self.star || self.reaches(file, fresh, taken),
            };
            if reached {
                passed.push((taken, exported_as));
            }
        }
        if binding.star {
            // The names `file` gained: all of them when the walk reaches it whole, and then its exports are known, because `expand` followed the importer at file level otherwise.
            let default = self.default;
            let gained: &[NameId] = match delta {
                Delta::Names(fresh) => fresh,
                Delta::All => self.exported_by(file).unwrap_or_default(),
            };
            passed.extend(
                gained
                    .iter()
                    .copied()
                    .filter(|&name| Some(name) != default)
                    .map(|name| (name, name)),
            );
        }
        passed.sort_unstable();
        passed.dedup();
        // A star edge always reaches its importer, even when the file has nothing left to pass on, because the importers of the barrel answer for the names that no one exports any more.
        if passed.is_empty() && !binding.star {
            return;
        }
        // An importer that does not list what it exports could pass on any of its names.
        if graph.exports(importer).is_none() {
            self.take_all(
                importer,
                Cause::ReExports {
                    file,
                    names: passed,
                },
            );
            return;
        }
        let first = self.seen.insert(importer);
        let held = self.held.entry(importer).or_default();
        let before = held.growth.len();
        for &(_, exported_as) in &passed {
            if held.names.insert(exported_as) {
                held.growth.push(exported_as);
            }
        }
        let grew = held.growth.len() > before;
        if first {
            self.reached.push(Reached {
                file: importer,
                distance: self.distance,
                cause: Cause::ReExports {
                    file,
                    names: passed,
                },
            });
        }
        // A barrel met for the first time is expanded once, with whatever it holds, even if that is nothing.
        if first || grew {
            self.next.push(importer);
        }
    }

    /// Makes `file` affected in all its names. A file already reached in some names keeps the distance and cause it first had.
    fn take_all(&mut self, file: FileId, cause: Cause) {
        if !self.whole.insert(file) {
            return;
        }
        if self.seen.insert(file) {
            self.reached.push(Reached {
                file,
                distance: self.distance,
                cause,
            });
        } else {
            self.held.remove(&file);
        }
        self.next.push(file);
    }

    /// Whether an importer of `file` that takes `name`, to use it or to pass it on, meets the names `fresh` that `file` gained: it is one of them, or `file` does not export it, because the import may be the very thing the change breaks or mends. A file whose exports are unknown could lose any name, so none is ruled out. X(file) is only worked out when `name` is not among `fresh`.
    fn reaches(&mut self, file: FileId, fresh: &[NameId], name: NameId) -> bool {
        holds(fresh, &name) || self.exported_by(file).is_none_or(|all| !holds(all, &name))
    }

    /// X(file), worked out once.
    fn exported_by(&mut self, file: FileId) -> Option<&[NameId]> {
        let (graph, default) = (self.graph, self.default);
        self.exported
            .entry(file)
            .or_insert_with(|| exports::exported(graph, file, default))
            .as_deref()
    }

    /// Ends the layer: the files that grew, in file order, with what each gained. The gain is fixed here and not when the file is expanded, so a file that becomes whole while its layer is expanded is still met with the names it had when the layer began and its importers sit one hop further out.
    fn cut(&mut self) -> Vec<(FileId, Delta)> {
        let mut grown = std::mem::take(&mut self.next);
        grown.sort_unstable();
        grown.dedup();
        let mut frontier = Vec::with_capacity(grown.len());
        for file in grown {
            if self.whole.contains(file) {
                frontier.push((file, Delta::All));
            } else if let Some(held) = self.held.get_mut(&file) {
                let mut gained = std::mem::take(&mut held.growth);
                gained.sort_unstable();
                frontier.push((file, Delta::Names(gained)));
            }
        }
        frontier
    }
}

/// Whether `item` is among `sorted`.
fn holds<T: Ord>(sorted: &[T], item: &T) -> bool {
    sorted.binary_search(item).is_ok()
}
