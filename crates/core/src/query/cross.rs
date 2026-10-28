//! Contracts joining files: which contracts a given key meets, how a walk crosses one, and the sites of one file in a contract.

use std::collections::BTreeMap;

use crate::contract::{ContractKey, Role};
use crate::graph::{ContractSite, Graph};
use crate::ids::{ContractId, FileId, IdSet};
use crate::index::Index;

use super::http::HttpMatcher;

/// Which way a walk crosses a contract. A contract joins files in three roles, `Declares < Serves < Calls`, and a walk leaves a file from its role at one end of that order towards the roles beyond it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direction {
    /// From a declarer to the servers and callers, and from a server to the callers: the way a change travels, because code follows the schema it is written against.
    ToCallers,
    /// From a caller to the servers and declarers, and from a server to the declarers: the way a chain runs from a caller to the schema behind it.
    ToDeclarers,
}

impl Direction {
    /// The role that decides where a file with `roles`, which are in role order, leaves a contract: its role at the walk's own end of the order, because that one reaches the most.
    fn leaving(self, roles: &[Role]) -> Option<Role> {
        match self {
            Self::ToCallers => roles.first().copied(),
            Self::ToDeclarers => roles.last().copied(),
        }
    }

    /// The role that reaches both of the others.
    const fn widest(self) -> Role {
        match self {
            Self::ToCallers => Role::Declares,
            Self::ToDeclarers => Role::Calls,
        }
    }

    /// Whether a walk that leaves a file in the role `from` reaches the sites in the role `to`.
    fn reaches(self, from: Role, to: Role) -> bool {
        match self {
            Self::ToCallers => from < to,
            Self::ToDeclarers => to < from,
        }
    }
}

/// The contracts a walk has crossed, so that none is crossed twice from the same kind of file.
#[derive(Debug)]
pub(super) struct Crossed {
    direction: Direction,
    /// Contracts crossed from a file in the widest role.
    widest: IdSet<ContractId>,
    /// Contracts crossed from a file in the middle role, or in the widest one: the second reaches the first's files.
    middle: IdSet<ContractId>,
}

impl Crossed {
    pub(super) fn new(direction: Direction, graph: &Graph) -> Self {
        Self {
            direction,
            widest: IdSet::with_universe(graph.contract_count()),
            middle: IdSet::with_universe(graph.contract_count()),
        }
    }
}

/// Finds the contracts whose sites join a key's, under the HTTP prefixes of one index.
#[derive(Debug)]
pub(super) struct Crossing<'a> {
    graph: &'a Graph,
    matcher: HttpMatcher,
    /// The HTTP contracts of the graph by the number of segments their path has, as written and with each prefix taken off. Two paths can meet only at a number of segments they both have, so a lookup compares the keys of the buckets it can meet in instead of every route of the graph.
    http: BTreeMap<usize, Vec<ContractId>>,
}

impl<'a> Crossing<'a> {
    pub(super) fn new(index: &'a Index) -> Self {
        let graph = &index.graph;
        let matcher = HttpMatcher::new(&index.http_prefixes);
        let mut http: BTreeMap<usize, Vec<ContractId>> = BTreeMap::new();
        for (contract, key) in graph.contracts() {
            let Some((_, path)) = key.http_parts() else {
                continue;
            };
            for length in matcher.lengths(path) {
                http.entry(length).or_default().push(contract);
            }
        }
        Self {
            graph,
            matcher,
            http,
        }
    }

    /// The contracts whose sites join `key`'s, in key order: the contract of `key` itself, and for an HTTP key every HTTP contract that meets it, whether or not `key` is a contract of the graph.
    pub(super) fn meeting(&self, key: &ContractKey) -> Vec<ContractId> {
        let Some((_, path)) = key.http_parts() else {
            return self.graph.contract_id(key).into_iter().collect();
        };
        // A contract sits in several buckets when its prefixes give it several lengths, and ids are in key order.
        let mut candidates: Vec<ContractId> = self
            .matcher
            .lengths(path)
            .filter_map(|length| self.http.get(&length))
            .flatten()
            .copied()
            .collect();
        candidates.sort_unstable();
        candidates.dedup();
        candidates.retain(|&contract| {
            self.graph
                .contract(contract)
                .is_some_and(|other| self.matcher.meets(key, other))
        });
        candidates
    }

    /// The sites a walk reaches by following `contract` out of `file`, each with the contract it is a site of: the contract itself, or for an HTTP key one that meets it. They come contract by contract in key order, and within a contract by file and line. A contract is followed once from each of the two roles that can leave it, because a walk meets files by growing distance, so a later file can only be as far away or farther.
    pub(super) fn cross(
        &self,
        crossed: &mut Crossed,
        file: FileId,
        contract: ContractId,
    ) -> Vec<(ContractId, &'a ContractSite)> {
        let graph = self.graph;
        let direction = crossed.direction;
        let Some(leaving) = direction.leaving(&roles_of(graph.sites(contract), file)) else {
            return Vec::new();
        };
        let first = if leaving == direction.widest() {
            crossed.middle.insert(contract);
            crossed.widest.insert(contract)
        } else if leaving == Role::Serves {
            crossed.middle.insert(contract)
        } else {
            // The role at the far end reaches nothing.
            false
        };
        let Some(key) = graph.contract(contract).filter(|_| first) else {
            return Vec::new();
        };
        self.meeting(key)
            .into_iter()
            .flat_map(|meeting| {
                graph
                    .sites(meeting)
                    .iter()
                    .filter(move |site| direction.reaches(leaving, site.role))
                    .map(move |site| (meeting, site))
            })
            .collect()
    }
}

/// The sites of `file` among the sites of one contract, which the graph keeps sorted by file.
pub(super) fn sites_of(
    sites: &[ContractSite],
    file: FileId,
) -> impl Iterator<Item = &ContractSite> {
    let start = sites.partition_point(|site| site.file < file);
    sites
        .get(start..)
        .unwrap_or_default()
        .iter()
        .take_while(move |site| site.file == file)
}

/// The roles a file takes in one contract, in role order.
pub(super) fn roles_of(sites: &[ContractSite], file: FileId) -> Vec<Role> {
    let mut roles: Vec<Role> = sites_of(sites, file).map(|site| site.role).collect();
    roles.sort_unstable();
    roles.dedup();
    roles
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROLES: [Role; 3] = [Role::Declares, Role::Serves, Role::Calls];
    const DIRECTIONS: [Direction; 2] = [Direction::ToCallers, Direction::ToDeclarers];

    #[test]
    fn a_walk_to_callers_is_the_mirror_of_a_walk_to_declarers() {
        for from in ROLES {
            for to in ROLES {
                assert_eq!(
                    Direction::ToCallers.reaches(from, to),
                    Direction::ToDeclarers.reaches(to, from),
                    "{from:?} to {to:?}"
                );
            }
        }
    }

    #[test]
    fn the_widest_role_reaches_both_others_and_the_far_end_none() {
        for direction in DIRECTIONS {
            let reached = |from: Role| {
                ROLES
                    .into_iter()
                    .filter(|&to| direction.reaches(from, to))
                    .count()
            };
            let far_end = if direction.widest() == Role::Declares {
                Role::Calls
            } else {
                Role::Declares
            };
            assert_eq!(reached(direction.widest()), 2, "{direction:?}");
            assert_eq!(reached(Role::Serves), 1, "{direction:?}");
            assert_eq!(reached(far_end), 0, "{direction:?}");
        }
    }

    #[test]
    fn a_file_leaves_a_contract_in_the_role_at_its_own_end() {
        let roles = [Role::Serves, Role::Calls];
        assert_eq!(Direction::ToCallers.leaving(&roles), Some(Role::Serves));
        assert_eq!(Direction::ToDeclarers.leaving(&roles), Some(Role::Calls));
        assert_eq!(Direction::ToCallers.leaving(&[]), None);
    }
}
