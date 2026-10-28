//! Contracts joining files: which contracts a given key meets, and the sites of one file in a contract.

use crate::contract::{ContractKey, ContractKind, Role};
use crate::graph::{ContractSite, Graph};
use crate::ids::{ContractId, FileId};
use crate::index::Index;

use super::http::HttpMatcher;

/// Finds the contracts whose sites join a key's, under the HTTP prefixes of one index.
#[derive(Debug)]
pub(super) struct Crossing<'a> {
    graph: &'a Graph,
    matcher: HttpMatcher,
    /// Every HTTP contract of the graph, in key order.
    http: Vec<ContractId>,
}

impl<'a> Crossing<'a> {
    pub(super) fn new(index: &'a Index) -> Self {
        let graph = &index.graph;
        Self {
            graph,
            matcher: HttpMatcher::new(&index.http_prefixes),
            http: graph
                .contracts()
                .filter(|(_, key)| *key.kind() == ContractKind::Http)
                .map(|(contract, _)| contract)
                .collect(),
        }
    }

    /// The contracts whose sites join `key`'s, in key order: the contract of `key` itself, and for an HTTP key every
    /// HTTP contract that meets it, whether or not `key` is a contract of the graph.
    pub(super) fn meeting(&self, key: &ContractKey) -> Vec<ContractId> {
        if *key.kind() != ContractKind::Http {
            return self.graph.contract_id(key).into_iter().collect();
        }
        self.http
            .iter()
            .copied()
            .filter(|&contract| {
                self.graph
                    .contract(contract)
                    .is_some_and(|other| self.matcher.meets(key, other))
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
