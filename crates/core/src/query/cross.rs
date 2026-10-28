//! Contracts joining files: which contracts a given key meets, and the sites of one file in a contract.

use std::collections::BTreeMap;

use crate::contract::{ContractKey, Role};
use crate::graph::{ContractSite, Graph};
use crate::ids::{ContractId, FileId};
use crate::index::Index;

use super::http::HttpMatcher;

/// Finds the contracts whose sites join a key's, under the HTTP prefixes of one index.
#[derive(Debug)]
pub(super) struct Crossing<'a> {
    graph: &'a Graph,
    matcher: HttpMatcher,
    /// The HTTP contracts of the graph by the number of segments their path has, as written and with each prefix
    /// taken off. Two paths can meet only at a number of segments they both have, so a lookup compares the keys of
    /// the buckets it can meet in instead of every route of the graph.
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

    /// The contracts whose sites join `key`'s, in key order: the contract of `key` itself, and for an HTTP key every
    /// HTTP contract that meets it, whether or not `key` is a contract of the graph.
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
