//! The fixpoint behind `impact`: every file a change reaches, how many hops away, and the first way it got there.

use crate::contract::Role;
use crate::graph::Graph;
use crate::ids::{ContractId, FileId, IdSet};

use super::cross::{Crossing, roles_of};

/// How the walk got to a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Cause {
    /// The file imports the file the walk came from.
    Imports(FileId),
    /// The file has a site with `role` in `contract`, which joins a contract of the file the walk came from.
    Contract { role: Role, contract: ContractId },
}

/// A file the walk reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Reached {
    pub(super) file: FileId,
    /// Hops from the nearest changed file.
    pub(super) distance: u32,
    /// The first way the walk found: of the nearest ways, the one from the file with the smallest path.
    pub(super) cause: Cause,
}

/// Every file that reaches one of `changed` within `depth` hops, ordered by distance and then by file.
///
/// A hop goes from a file to the files that import it, except where the import is only a Rust `mod` declaration,
/// which owns the child file and uses nothing from it; or across a contract: a file that declares one reaches the
/// files that serve or call it, and a file that serves one reaches the files that call it. The changed files are
/// never reached.
pub(super) fn reach(
    graph: &Graph,
    crossing: &Crossing<'_>,
    changed: &[FileId],
    depth: Option<u32>,
) -> Vec<Reached> {
    let mut walk = Walk {
        graph,
        crossing,
        seen: IdSet::with_universe(graph.file_count()),
        declared: IdSet::with_universe(graph.contract_count()),
        served: IdSet::with_universe(graph.contract_count()),
        distance: 0,
        reached: Vec::new(),
        next: Vec::new(),
    };
    let mut frontier = changed.to_vec();
    frontier.sort_unstable();
    frontier.dedup();
    walk.seen.extend(frontier.iter().copied());
    while !frontier.is_empty() && depth.is_none_or(|limit| walk.distance < limit) {
        walk.distance += 1;
        for &file in &frontier {
            walk.expand(file);
        }
        frontier = std::mem::take(&mut walk.next);
        // Ascending files make the first cause found the same on every run.
        frontier.sort_unstable();
    }
    walk.reached
        .sort_unstable_by_key(|reached| (reached.distance, reached.file));
    walk.reached
}

struct Walk<'a> {
    graph: &'a Graph,
    crossing: &'a Crossing<'a>,
    seen: IdSet<FileId>,
    /// Contracts already crossed from a file that declares them.
    declared: IdSet<ContractId>,
    /// Contracts already crossed from a file that serves them, or declares them: the second reaches the first's files.
    served: IdSet<ContractId>,
    /// The distance of the files being reached now.
    distance: u32,
    reached: Vec<Reached>,
    next: Vec<FileId>,
}

impl Walk<'_> {
    fn expand(&mut self, file: FileId) {
        let graph = self.graph;
        for &(importer, edge) in graph.dependents(file) {
            if !edge.is_structural() {
                self.visit(importer, Cause::Imports(file));
            }
        }
        for &contract in graph.contracts_of(file) {
            self.cross(file, contract);
        }
    }

    /// Follows `contract` out of `file`. A contract is crossed once for each of the two sources, because the walk
    /// meets files by growing distance, so a later file can only be as far away or farther.
    fn cross(&mut self, file: FileId, contract: ContractId) {
        let graph = self.graph;
        let roles = roles_of(graph.sites(contract), file);
        let declares = roles.contains(&Role::Declares);
        if declares {
            if !self.declared.insert(contract) {
                return;
            }
            self.served.insert(contract);
        } else if roles.contains(&Role::Serves) {
            if !self.served.insert(contract) {
                return;
            }
        } else {
            return;
        }
        let Some(key) = graph.contract(contract) else {
            return;
        };
        for meeting in self.crossing.meeting(key) {
            for site in graph.sites(meeting) {
                if site.role == Role::Calls || (declares && site.role == Role::Serves) {
                    self.visit(
                        site.file,
                        Cause::Contract {
                            role: site.role,
                            contract: meeting,
                        },
                    );
                }
            }
        }
    }

    fn visit(&mut self, file: FileId, cause: Cause) {
        if self.seen.insert(file) {
            self.reached.push(Reached {
                file,
                distance: self.distance,
                cause,
            });
            self.next.push(file);
        }
    }
}
