//! Which GraphQL endpoint a site belongs to, so that a walk crosses a GraphQL contract only within an endpoint.

use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, HashMap};

use foldhash::fast::RandomState;
use globset::GlobSet;

use crate::contract::{ContractKey, ContractKind, Role};
use crate::glob::glob_set;
use crate::graph::Graph;
use crate::ids::FileId;
use crate::path::RepoPath;
use crate::setup::GraphqlEndpoint;

/// A set of endpoints, as the numbers they have in [`Index::graphql_endpoints`](crate::index::Index::graphql_endpoints). Equal sets are equal scopes, so a scope is a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct Scope(u32);

impl Scope {
    /// The scope of a site no endpoint claims, and of every site of a contract that is not scoped.
    pub(super) const EMPTY: Self = Self(0);

    /// A scope that stands for no endpoints in particular, for the tests of what keeps scopes apart.
    #[cfg(test)]
    pub(super) const fn numbered(number: u32) -> Self {
        Self(number)
    }
}

/// The globs of one endpoint, compiled.
#[derive(Debug)]
struct Endpoint {
    clients: GlobSet,
    servers: GlobSet,
}

/// The scopes of the sites of one query, worked out for a file and a role the first time a walk asks, because most queries meet no GraphQL contract and none of them needs every file. The globs are compiled by the first of those asks, so a query that never scopes compiles nothing.
#[derive(Debug)]
pub(super) struct Scopes<'a> {
    graph: &'a Graph,
    endpoints: &'a [GraphqlEndpoint],
    compiled: OnceCell<Vec<Endpoint>>,
    memo: RefCell<Memo>,
}

#[derive(Debug)]
struct Memo {
    /// The endpoints of each scope, sorted. The scope numbered 0 is the empty one.
    sets: Vec<Vec<u32>>,
    scopes: BTreeMap<Vec<u32>, Scope>,
    // Only probed, never iterated: AGENTS.md forbids iterating a hash map into output.
    sites: HashMap<(FileId, Role), Scope, RandomState>,
}

impl Memo {
    fn intern(&mut self, endpoints: Vec<u32>) -> Scope {
        if endpoints.is_empty() {
            return Scope::EMPTY;
        }
        if let Some(&scope) = self.scopes.get(&endpoints) {
            return scope;
        }
        let scope = Scope(u32::try_from(self.sets.len()).unwrap_or(u32::MAX));
        self.sets.push(endpoints.clone());
        self.scopes.insert(endpoints, scope);
        scope
    }

    fn shares(&self, left: Scope, right: Scope) -> bool {
        let (Some(left), Some(right)) = (
            usize::try_from(left.0)
                .ok()
                .and_then(|at| self.sets.get(at)),
            usize::try_from(right.0)
                .ok()
                .and_then(|at| self.sets.get(at)),
        ) else {
            return false;
        };
        left.iter()
            .any(|endpoint| right.binary_search(endpoint).is_ok())
    }
}

impl<'a> Scopes<'a> {
    /// The scopes of the sites of `graph` under `endpoints`. With none, no contract is scoped and nothing is matched.
    pub(super) fn new(graph: &'a Graph, endpoints: &'a [GraphqlEndpoint]) -> Self {
        Self {
            graph,
            endpoints,
            compiled: OnceCell::new(),
            memo: RefCell::new(Memo {
                sets: vec![Vec::new()],
                scopes: BTreeMap::new(),
                sites: HashMap::default(),
            }),
        }
    }

    /// The endpoints with their globs compiled, on the first call. The config checked every glob when it read them, so a set that does not build comes from an index made some other way and claims nothing.
    fn compiled(&self) -> &[Endpoint] {
        self.compiled.get_or_init(|| {
            self.endpoints
                .iter()
                .map(|endpoint| Endpoint {
                    clients: glob_set(&endpoint.clients).unwrap_or_default(),
                    servers: glob_set(&endpoint.servers).unwrap_or_default(),
                })
                .collect()
        })
    }

    /// Whether the sites of the contract `key` have scopes: it is a GraphQL contract and the index has endpoints.
    pub(super) fn applies(&self, key: &ContractKey) -> bool {
        !self.endpoints.is_empty() && matches!(key.kind(), ContractKind::Graphql)
    }

    /// The scope of the site of `file` in `role`. A server belongs to the endpoints that list it among their servers, a caller to those that list it among their clients or, when none does, among their servers, because a gateway that is no client of its own endpoint calls the API it serves, and a declarer to the endpoints that list it in either.
    pub(super) fn of(&self, file: FileId, role: Role) -> Scope {
        if let Some(&scope) = self.memo.borrow().sites.get(&(file, role)) {
            return scope;
        }
        let endpoints = self.endpoints_of(file, role);
        let mut memo = self.memo.borrow_mut();
        let scope = memo.intern(endpoints);
        memo.sites.insert((file, role), scope);
        scope
    }

    fn endpoints_of(&self, file: FileId, role: Role) -> Vec<u32> {
        let Some(path) = self.graph.path(file).map(RepoPath::as_str) else {
            return Vec::new();
        };
        let claimed = |claims: &dyn Fn(&Endpoint) -> bool| -> Vec<u32> {
            (0..)
                .zip(self.compiled())
                .filter(|&(_, endpoint)| claims(endpoint))
                .map(|(number, _)| number)
                .collect()
        };
        let client = |endpoint: &Endpoint| endpoint.clients.is_match(path);
        let server = |endpoint: &Endpoint| endpoint.servers.is_match(path);
        match role {
            Role::Serves => claimed(&server),
            Role::Declares => claimed(&|endpoint| client(endpoint) || server(endpoint)),
            Role::Calls => {
                let clients = claimed(&client);
                if clients.is_empty() {
                    claimed(&server)
                } else {
                    clients
                }
            }
        }
    }

    /// Whether two sites meet: the scope of the one in the higher role is empty, or shares an endpoint with the other's. It is a property of the pair, so a walk towards callers and a walk towards declarers agree on it.
    pub(super) fn meet(&self, left: (Role, Scope), right: (Role, Scope)) -> bool {
        let (high, low) = if left.0 >= right.0 {
            (left.1, right.1)
        } else {
            (right.1, left.1)
        };
        high == Scope::EMPTY || self.memo.borrow().shares(high, low)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Tree;

    #[test]
    fn the_globs_are_compiled_by_the_first_ask_for_a_scope_and_not_before() {
        let index = Tree::new()
            .endpoint(&["web/**"], &["api/**"])
            .serves("api/users.ts", "http GET /users")
            .build();
        let file = index.graph.file_id("api/users.ts").unwrap();
        let http = ContractKey::parse("http GET /users").unwrap();

        let scopes = Scopes::new(&index.graph, &index.graphql_endpoints);
        assert!(scopes.compiled.get().is_none());
        assert!(!scopes.applies(&http));
        assert!(scopes.compiled.get().is_none());
        let _ = scopes.of(file, Role::Serves);

        assert!(scopes.compiled.get().is_some());
    }
}
