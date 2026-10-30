//! The fixpoint behind `impact`: every file a change reaches, how many hops away, and the first way it got there.

mod exports;
mod names;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use foldhash::fast::RandomState;

use crate::contract::Role;
use crate::graph::{Edge, Graph, Uses};
use crate::ids::{ContractId, FileId, IdSet, NameId};

use super::cross::{Crossed, Crossing, Direction, Leavers, keep_fewest, roles_of};

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

/// A file to expand, with the names of it that the walk meets for the first time in a layer.
struct Front {
    file: FileId,
    delta: Delta,
    /// The hops through code of the way it was reached by, which the depth bounds: a hop across a contract is not one. A file reached in some names has the fewest of the ways that brought in any of them, so its importers are looked at only while that is under the depth.
    code: u32,
}

/// The names of a file that the walk meets for the first time in a layer, which is what its importers are measured against.
enum Delta {
    /// Every name: the file is changed, or the walk has just found it affected in all its names.
    All,
    /// Only these names, sorted, each with the fewest hops through code of the ways that brought it in, and none at all for a barrel that a star reached while it has nothing to pass on: its importers are still checked against the names it does not export.
    Names(Vec<(NameId, u32)>),
}

/// The names of a barrel the walk has reached while the barrel is not yet affected in all its names.
#[derive(Default)]
struct Held {
    /// A(file): every name of the barrel that the change may reach.
    names: BTreeSet<NameId>,
    /// The names that joined `names` since the layer was cut, each with the fewest hops through code of the ways that brought it in that layer.
    growth: BTreeMap<NameId, u32>,
}

/// Every file that reaches one of `changed` within `depth` hops through code, ordered by distance and then by file.
///
/// A hop goes from a file to the files that import it, except where the import is only a Rust `mod` declaration, which owns the child file and uses nothing from it; or across a contract: a file that declares one reaches the files that serve or call it, and a file that serves one reaches the files that call it. The changed files are never reached.
///
/// The depth bounds the hops through code, which are the imports. A hop across a contract is a hop in the distance and not in the depth: every file the walk reaches crosses its contracts, and the file across carries the hops through code of the file it came from. Each file, and each name that reaches a barrel, carries the hops through code of the way it was first reached by, the fewest of the equally near ways found in one layer, and takes a hop through code only while they are fewer than the depth.
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
        leavers: Leavers::new(Direction::ToCallers),
        seen: IdSet::with_universe(graph.file_count()),
        whole: IdSet::with_universe(graph.file_count()),
        held: HashMap::default(),
        exported: HashMap::default(),
        default: graph.name_id("default"),
        star: graph.name_id("*"),
        depth,
        distance: 0,
        reached: Vec::new(),
        fresh: BTreeMap::new(),
        next: Vec::new(),
    };
    let mut changed = changed.to_vec();
    changed.sort_unstable();
    changed.dedup();
    walk.seen.extend(changed.iter().copied());
    walk.whole.extend(changed.iter().copied());
    let mut frontier: Vec<Front> = changed
        .into_iter()
        .map(|file| Front {
            file,
            delta: Delta::All,
            code: 0,
        })
        .collect();
    while !frontier.is_empty() {
        walk.distance += 1;
        walk.note_leavers(&frontier);
        for front in &frontier {
            walk.expand(front);
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
    /// The fewest hops through code of the files of the layer being expanded that leave each contract.
    leavers: Leavers,
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
    /// The hops through code a file may be reached by before the walk takes no more, none when there is no limit.
    depth: Option<u32>,
    /// The distance of the files being reached now.
    distance: u32,
    reached: Vec<Reached>,
    /// The files that became whole in this layer, each with the fewest hops through code of the ways that made it whole.
    fresh: BTreeMap<FileId, u32>,
    /// The barrels that were reached in some names for the first time or whose names grew in this layer, each time with the hops through code of the way. A barrel that became whole in this layer is in `fresh`.
    next: Vec<(FileId, u32)>,
}

impl Walk<'_> {
    /// Notes how many hops through code the files of a layer that leave each contract took, when the layer holds files that took different numbers. With no depth the hops through code bound nothing.
    fn note_leavers(&mut self, frontier: &[Front]) {
        let whole = || {
            frontier
                .iter()
                .filter(|front| matches!(front.delta, Delta::All))
        };
        if !self.leavers.start(whole().map(|front| front.code)) || self.depth.is_none() {
            return;
        }
        for front in whole() {
            for &contract in self.graph.contracts_of(front.file) {
                for role in roles_of(self.graph.sites(contract), front.file) {
                    self.leavers.add(contract, role, front.code);
                }
            }
        }
    }

    /// Whether a way into `file` that took `code` hops through code can still change what the walk knows of it: the file is not whole yet, or it became whole in this layer by a way that took more.
    fn gains_from(&self, file: FileId, code: u32) -> bool {
        !self.whole.contains(file)
            || (self.depth.is_some() && self.fresh.get(&file).is_some_and(|&known| code < known))
    }

    fn expand(&mut self, front: &Front) {
        let (file, delta, code) = (front.file, &front.delta, front.code);
        let graph = self.graph;
        // Whether the exports of `file` are known, which every importer of it that is not plain asks when the walk reaches the file whole. The answer is the same for all of them, so it is worked out once, and not at all when none asks.
        let mut known = None;
        // The depth stops the hops through code and not the crossing of a contract.
        let importers = if self.depth.is_none_or(|limit| code < limit) {
            graph.dependents(file)
        } else {
            &[]
        };
        for &(importer, edge) in importers {
            if edge.is_structural() || !self.gains_from(importer, code + 1) {
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
                    code + 1,
                );
            } else {
                self.follow(importer, file, edge, front);
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
                    let arrival = self.leavers.arrival(contract, site.role, code);
                    self.take_all(site.file, cause, arrival);
                }
            }
        }
    }

    /// What the importer gets from the new names of the file of `front` over `edge`. A use comes before a pass, so the first way a file is found over one edge is the import. A name is followed only while the hops through code of its way are fewer than the depth, and the importer lists every name it takes among those, as it would had they all come by the same way, and has the fewest of their hops, plus one.
    fn follow(&mut self, importer: FileId, file: FileId, edge: Edge, front: &Front) {
        let graph = self.graph;
        let binding = graph.binding(edge);
        let (delta, code) = (&front.delta, front.code);
        let depth = self.depth;
        let within = |code: u32| depth.is_none_or(|limit| code < limit);

        // The names the importer uses itself, each with the hops through code it was reached by. One of them is enough to affect it in all of its names.
        let mut used: Vec<(NameId, u32)> = Vec::new();
        let uses_all = match (binding.uses, delta) {
            (Uses::All, _) => true,
            (Uses::Names(wanted), Delta::All) => {
                used.extend(wanted.iter().map(|&name| (name, code)));
                false
            }
            (Uses::Names(wanted), Delta::Names(fresh)) => {
                for &name in wanted {
                    if let Some(reached) =
                        self.reaches(file, fresh, code, name).filter(|&c| within(c))
                    {
                        used.push((name, reached));
                    }
                }
                false
            }
        };
        if uses_all || !used.is_empty() {
            let fewest = used
                .iter()
                .map(|&(_, reached)| reached)
                .min()
                .unwrap_or(code);
            let names = used.into_iter().map(|(name, _)| name).collect();
            self.take_all(importer, Cause::Imports { file, names }, fewest + 1);
            return;
        }
        // Only a way that makes the importer whole can change a file that is whole already.
        if self.whole.contains(importer) {
            return;
        }

        // The names it passes on, which affect it only in those names, each with the name it has in `file` and the hops through code it was reached by.
        let mut passed: Vec<(NameId, NameId, u32)> = Vec::new();
        for &(exported_as, taken) in binding.passes {
            let reached = match delta {
                Delta::All => Some(code),
                Delta::Names(_) if Some(taken) == self.star => Some(code),
                Delta::Names(fresh) => self
                    .reaches(file, fresh, code, taken)
                    .filter(|&c| within(c)),
            };
            if let Some(reached) = reached {
                passed.push((taken, exported_as, reached));
            }
        }
        if binding.star {
            // The names `file` gained: all of them when the walk reaches it whole, and then its exports are known, because `expand` followed the importer at file level otherwise.
            let default = self.default;
            match delta {
                Delta::Names(fresh) => passed.extend(
                    fresh
                        .iter()
                        .filter(|&&(name, reached)| Some(name) != default && within(reached))
                        .map(|&(name, reached)| (name, name, reached)),
                ),
                Delta::All => passed.extend(
                    self.exported_by(file)
                        .unwrap_or_default()
                        .iter()
                        .copied()
                        .filter(|&name| Some(name) != default)
                        .map(|name| (name, name, code)),
                ),
            }
        }
        // The first of the same pair has the fewest hops.
        passed.sort_unstable();
        passed.dedup_by(|later, earlier| (later.0, later.1) == (earlier.0, earlier.1));
        // A star edge always reaches its importer, even when the file has nothing left to pass on, because the importers of the barrel answer for the names that no one exports any more.
        if passed.is_empty() && !binding.star {
            return;
        }
        let fewest = passed
            .iter()
            .map(|&(_, _, reached)| reached)
            .min()
            .unwrap_or(code)
            + 1;
        let names: Vec<(NameId, NameId)> = passed
            .iter()
            .map(|&(taken, exported_as, _)| (taken, exported_as))
            .collect();
        // An importer that does not list what it exports could pass on any of its names.
        if graph.exports(importer).is_none() {
            self.take_all(importer, Cause::ReExports { file, names }, fewest);
            return;
        }
        let first = self.seen.insert(importer);
        let held = self.held.entry(importer).or_default();
        let before = held.growth.len();
        for &(_, exported_as, reached) in &passed {
            // A name that joined in this layer by another way keeps the fewer hops through code.
            if held.names.insert(exported_as) || held.growth.contains_key(&exported_as) {
                keep_fewest(&mut held.growth, exported_as, reached + 1);
            }
        }
        let grew = held.growth.len() > before;
        if first {
            self.reached.push(Reached {
                file: importer,
                distance: self.distance,
                cause: Cause::ReExports { file, names },
            });
        }
        // A barrel met for the first time is expanded once, with whatever it holds, even if that is nothing.
        if first || grew {
            self.next.push((importer, fewest));
        }
    }

    /// Makes `file` affected in all its names, reached by `code` hops through code. A file already reached in some names keeps the distance and cause it first had, and a file made whole in this layer by another way keeps the fewer hops through code.
    fn take_all(&mut self, file: FileId, cause: Cause, code: u32) {
        if !self.whole.insert(file) {
            if let Some(known) = self.fresh.get_mut(&file) {
                *known = (*known).min(code);
            }
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
        self.fresh.insert(file, code);
    }

    /// The hops through code by which an importer of `file` that takes `name`, to use it or to pass it on, meets the names `fresh` that `file` gained, `code` being those of the file: the hops of `name` when it is one of them, and `code` when `file` does not export it, because the import may be the very thing the change breaks or mends. A file whose exports are unknown could lose any name, so none is ruled out. None when it meets none. X(file) is only worked out when `name` is not among `fresh`.
    fn reaches(
        &mut self,
        file: FileId,
        fresh: &[(NameId, u32)],
        code: u32,
        name: NameId,
    ) -> Option<u32> {
        if let Ok(at) = fresh.binary_search_by_key(&name, |&(gained, _)| gained) {
            return fresh.get(at).map(|&(_, reached)| reached);
        }
        self.exported_by(file)
            .is_none_or(|all| !holds(all, &name))
            .then_some(code)
    }

    /// X(file), worked out once.
    fn exported_by(&mut self, file: FileId) -> Option<&[NameId]> {
        let (graph, default) = (self.graph, self.default);
        self.exported
            .entry(file)
            .or_insert_with(|| exports::exported(graph, file, default))
            .as_deref()
    }

    /// Ends the layer: the files that grew, in file order, with what each gained. The gain is fixed here and not when the file is expanded, so a file that becomes whole while its layer is expanded is still met with the names it had when the layer began and its importers sit one hop further out. A barrel is met once, with each name it gained and the hops through code of that name's own way, so that each name is followed as far as its way allows.
    fn cut(&mut self) -> Vec<Front> {
        let mut frontier: Vec<Front> = std::mem::take(&mut self.fresh)
            .into_iter()
            .map(|(file, code)| Front {
                file,
                delta: Delta::All,
                code,
            })
            .collect();
        let mut grown = std::mem::take(&mut self.next);
        // The first entry of a file has the fewest hops through code.
        grown.sort_unstable();
        grown.dedup_by_key(|&mut (file, _)| file);
        for (file, first) in grown {
            let Some(held) = self
                .held
                .get_mut(&file)
                .filter(|_| !self.whole.contains(file))
            else {
                continue;
            };
            let gained: Vec<(NameId, u32)> = std::mem::take(&mut held.growth).into_iter().collect();
            // A barrel met for the first time is expanded once, with whatever it holds, even if that is nothing.
            let code = gained.iter().map(|&(_, code)| code).fold(first, u32::min);
            frontier.push(Front {
                file,
                delta: Delta::Names(gained),
                code,
            });
        }
        frontier.sort_unstable_by_key(|front| front.file);
        frontier
    }
}

/// Whether `item` is among `sorted`.
fn holds<T: Ord>(sorted: &[T], item: &T) -> bool {
    sorted.binary_search(item).is_ok()
}
