//! Contracts joining files: which contracts a given key meets, how a walk crosses one, and the sites of one file in a contract.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::{ContractKey, Role};
use crate::graph::{ContractSite, Graph};
use crate::ids::{ContractId, FileId, IdSet};
use crate::index::Index;

use super::http::HttpMatcher;
use super::scope::{Scope, Scopes};

/// Which way a walk crosses a contract. A contract joins files in three roles, `Declares < Serves < Calls`, and a walk leaves a file from its role at one end of that order towards the roles beyond it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direction {
    /// From a declarer to the servers and callers, and from a server to the callers: the way a change travels, because code follows the schema it is written against.
    ToCallers,
    /// From a caller to the servers and declarers, and from a server to the declarers: the way a chain runs from a caller to the schema behind it.
    ToDeclarers,
}

impl Direction {
    /// The roles a walk leaves a file by, the widest first. The role at the far end reaches nothing, so it is not one of them.
    const fn leaving(self) -> [Role; 2] {
        match self {
            Self::ToCallers => [Role::Declares, Role::Serves],
            Self::ToDeclarers => [Role::Calls, Role::Serves],
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

/// The contracts a walk has crossed, so that none is crossed twice from the same kind of file with the same scope.
#[derive(Debug)]
pub(super) struct Crossed {
    direction: Direction,
    /// Contracts crossed from a file in the widest role.
    widest: Marks,
    /// Contracts crossed from a file in the middle role, or in the widest one: the second reaches the first's files.
    middle: Marks,
}

impl Crossed {
    pub(super) fn new(direction: Direction, graph: &Graph) -> Self {
        Self {
            direction,
            widest: Marks::new(graph),
            middle: Marks::new(graph),
        }
    }

    /// Notes a crossing of `contract` from a file in `role` with `scope`, and says whether it is the first of its kind. Files with another scope reach other sites, so each scope is crossed on its own.
    fn first(&mut self, contract: ContractId, role: Role, scope: Scope) -> bool {
        let middle = self.middle.insert(contract, scope);
        if role == self.direction.widest() {
            self.widest.insert(contract, scope)
        } else {
            middle
        }
    }
}

/// The contracts crossed, each with the scopes it was crossed with.
#[derive(Debug)]
struct Marks {
    /// The contracts crossed with the empty scope, which is every contract that is not scoped, so the walk over a repository with no endpoint pays a bit per contract.
    empty: IdSet<ContractId>,
    others: BTreeSet<(ContractId, Scope)>,
}

impl Marks {
    fn new(graph: &Graph) -> Self {
        Self {
            empty: IdSet::with_universe(graph.contract_count()),
            others: BTreeSet::new(),
        }
    }

    fn insert(&mut self, contract: ContractId, scope: Scope) -> bool {
        if scope == Scope::EMPTY {
            self.empty.insert(contract)
        } else {
            self.others.insert((contract, scope))
        }
    }
}

/// Notes `code` hops through code under `key`, keeping the fewest of those noted.
pub(super) fn keep_fewest<K: Ord>(fewest: &mut BTreeMap<K, u32>, key: K, code: u32) {
    fewest
        .entry(key)
        .and_modify(|known| *known = (*known).min(code))
        .or_insert(code);
}

/// The fewest hops through code of the files of one layer of a walk that leave each contract, by the role and the scope they leave it in. A contract is crossed once, from the first file of the layer that leaves it, and the files across are as few hops through code from the walk's start as the fewest of those that reach them.
#[derive(Debug)]
pub(super) struct Leavers<'a> {
    direction: Direction,
    crossing: &'a Crossing<'a>,
    fewest: BTreeMap<(ContractId, Role, Scope), u32>,
}

impl<'a> Leavers<'a> {
    pub(super) fn new(direction: Direction, crossing: &'a Crossing<'a>) -> Self {
        Self {
            direction,
            crossing,
            fewest: BTreeMap::new(),
        }
    }

    /// Starts the notes of a layer whose files took `codes` hops through code, forgetting those of the layer before, and says whether the files took different numbers. With equal ones the first file to leave a contract has the fewest, so nothing needs noting and [`Leavers::arrival`] answers with the hops of the file that crosses.
    #[must_use]
    pub(super) fn start(&mut self, codes: impl IntoIterator<Item = u32>) -> bool {
        self.fewest.clear();
        let mut codes = codes.into_iter();
        let first = codes.next();
        codes.any(|code| Some(code) != first)
    }

    /// Notes that `file`, a file of the layer, leaves `contract` by a site in `role`, with `code` hops through code.
    pub(super) fn add(&mut self, file: FileId, contract: ContractId, role: Role, code: u32) {
        let scope = self.crossing.scope_of(contract, file, role);
        keep_fewest(&mut self.fewest, (contract, role, scope), code);
    }

    /// The hops through code of `site` across `contract`: the fewest of the files that leave it in a role that reaches the site's and meet it, or `code`, the hops of the file that crosses, when none was noted.
    pub(super) fn arrival(&self, contract: ContractId, site: &ContractSite, code: u32) -> u32 {
        let to = (
            site.role,
            self.crossing.scope_of(contract, site.file, site.role),
        );
        self.fewest
            .range((contract, Role::Declares, Scope::EMPTY)..)
            .take_while(|&(&(noted, ..), _)| noted == contract)
            .filter(|&(&(_, from, scope), _)| {
                self.direction.reaches(from, site.role)
                    && self.crossing.scopes.meet((from, scope), to)
            })
            .map(|(_, &fewest)| fewest)
            .min()
            .unwrap_or(code)
    }
}

/// Finds the contracts whose sites join a key's, under the HTTP prefixes of one index.
#[derive(Debug)]
pub(super) struct Crossing<'a> {
    graph: &'a Graph,
    scopes: Scopes<'a>,
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
            scopes: Scopes::new(graph, &index.graphql_endpoints),
            matcher,
            http,
        }
    }

    /// The scope of the site of `file` in `role` in `contract`: empty unless the contract is a GraphQL one and the index has endpoints.
    fn scope_of(&self, contract: ContractId, file: FileId, role: Role) -> Scope {
        match self.graph.contract(contract) {
            Some(key) if self.scopes.applies(key) => self.scopes.of(file, role),
            _ => Scope::EMPTY,
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

    /// The sites a walk reaches by following `contract` out of `file`, each with the contract it is a site of: the contract itself, or for an HTTP key one that meets it. They come contract by contract in key order, and within a contract by file and line. A contract is followed once from each of the two roles that can leave it, because a walk meets files by growing distance, so a later file can only be as far away or farther. A GraphQL contract of an index with endpoints is followed once from each scope too, and only to the sites that meet the file's.
    pub(super) fn cross(
        &self,
        crossed: &mut Crossed,
        file: FileId,
        contract: ContractId,
    ) -> Vec<(ContractId, &'a ContractSite)> {
        let roles = sites_of(self.graph.sites(contract), file).map(|site| site.role);
        self.cross_as(crossed, file, roles, contract)
    }

    /// [`Crossing::cross`] for a walk that leaves a file by some of its sites in `contract` and not all: `roles` are those of the sites it leaves by. A walk by name leaves a file by the sites of the declarations it reached.
    ///
    /// Without scopes the first role of the file in the order of the walk decides where it leaves, because it reaches the most and its marks keep the next role from crossing again. A scoped contract is left by every role the file holds, because the file may serve one endpoint and call another.
    pub(super) fn cross_as(
        &self,
        crossed: &mut Crossed,
        file: FileId,
        roles: impl IntoIterator<Item = Role>,
        contract: ContractId,
    ) -> Vec<(ContractId, &'a ContractSite)> {
        let graph = self.graph;
        let direction = crossed.direction;
        let Some(key) = graph.contract(contract) else {
            return Vec::new();
        };
        let scoped = self.scopes.applies(key);
        let held = roles
            .into_iter()
            .fold(0_u8, |held, role| held | (1 << role as u8));
        let leaving = direction.leaving().map(|role| {
            if held & (1 << role as u8) == 0 {
                return None;
            }
            let scope = if scoped {
                self.scopes.of(file, role)
            } else {
                Scope::EMPTY
            };
            crossed
                .first(contract, role, scope)
                .then_some((role, scope))
        });
        if leaving.iter().all(Option::is_none) {
            return Vec::new();
        }
        self.meeting(key)
            .into_iter()
            .flat_map(|meeting| {
                graph
                    .sites(meeting)
                    .iter()
                    .filter(move |site| {
                        leaving.iter().flatten().any(|&(from, scope)| {
                            direction.reaches(from, site.role)
                                && (!scoped
                                    || self.scopes.meet(
                                        (from, scope),
                                        (site.role, self.scopes.of(site.file, site.role)),
                                    ))
                        })
                    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::Id as _;
    use crate::testing::Tree;

    const DIRECTIONS: [Direction; 2] = [Direction::ToCallers, Direction::ToDeclarers];
    /// The roles a file takes in a contract, in role order.
    const ROLES: [Role; 3] = [Role::Declares, Role::Serves, Role::Calls];

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
    fn a_walk_leaves_by_the_widest_role_first_and_never_by_the_far_end() {
        for direction in DIRECTIONS {
            let [first, second] = direction.leaving();
            assert_eq!(first, direction.widest());
            assert_eq!(second, Role::Serves);
            assert!(ROLES.into_iter().any(|to| direction.reaches(first, to)));
            assert!(ROLES.into_iter().any(|to| direction.reaches(second, to)));
        }
    }

    /// `schema.graphql` declares, `a.ts` and `b.ts` serve and `c.ts` calls one GraphQL contract.
    fn four_files() -> Index {
        Tree::new()
            .declares("schema.graphql", KEY)
            .serves("a.ts", KEY)
            .serves("b.ts", KEY)
            .calls("c.ts", KEY)
            .build()
    }

    const KEY: &str = "graphql Query.tenant";

    fn contract_of(index: &Index) -> ContractId {
        index
            .graph
            .contract_id(&ContractKey::parse(KEY).unwrap())
            .unwrap()
    }

    fn site_in<'a>(index: &'a Index, contract: ContractId, path: &str) -> &'a ContractSite {
        let file = index.graph.file_id(path).unwrap();
        sites_of(index.graph.sites(contract), file).next().unwrap()
    }

    #[test]
    fn a_site_gets_the_fewest_hops_of_the_files_that_leave_in_a_role_that_reaches_it() {
        let index = four_files();
        let crossing = Crossing::new(&index);
        let contract = contract_of(&index);
        let file = |path: &str| index.graph.file_id(path).unwrap();
        let mut leavers = Leavers::new(Direction::ToCallers, &crossing);
        leavers.add(file("a.ts"), contract, Role::Serves, 3);
        leavers.add(file("b.ts"), contract, Role::Serves, 1);
        leavers.add(file("schema.graphql"), contract, Role::Declares, 5);
        leavers.add(file("c.ts"), contract, Role::Calls, 0);

        // A caller is reached from a declarer and from both servers, and a server only from the declarer. A caller leaves nothing behind it, so its hops count for no one, though it is noted.
        let arrival = |leavers: &Leavers<'_>, path: &str| {
            leavers.arrival(contract, site_in(&index, contract, path), 9)
        };
        assert_eq!(arrival(&leavers, "c.ts"), 1);
        assert_eq!(arrival(&leavers, "a.ts"), 5);
        assert_eq!(arrival(&leavers, "schema.graphql"), 9);
        assert!(!leavers.start([]));
        assert_eq!(arrival(&leavers, "c.ts"), 9);
    }

    #[test]
    fn the_notes_of_a_layer_are_needed_only_when_its_files_took_different_hops() {
        let index = Index::default();
        let crossing = Crossing::new(&index);
        let mut leavers = Leavers::new(Direction::ToCallers, &crossing);
        assert!(!leavers.start([]));
        assert!(!leavers.start([2]));
        assert!(!leavers.start([2, 2, 2]));
        assert!(leavers.start([2, 2, 1]));
    }

    #[test]
    fn a_contract_is_crossed_again_from_a_file_of_another_scope_and_not_from_one_of_the_same() {
        let graph = Graph::default();
        let contract = ContractId::from_index(0);
        let (a, b) = (Scope::EMPTY, Scope::numbered(1));
        for direction in DIRECTIONS {
            let mut crossed = Crossed::new(direction, &graph);
            let widest = direction.widest();
            assert!(crossed.first(contract, widest, a));
            assert!(!crossed.first(contract, widest, a));
            assert!(crossed.first(contract, widest, b));
            // The widest role covers the middle one of the same scope only.
            assert!(!crossed.first(contract, Role::Serves, a));
            assert!(!crossed.first(contract, Role::Serves, b));
            assert!(crossed.first(contract, Role::Serves, Scope::numbered(2)));
        }
    }
}
