//! The graph: files joined by imports, and contracts joined to the files that declare, serve and call them.
//!
//! Both directions of the import relation are stored as compressed sparse rows, so dependents and dependencies of a file are one slice each, and a walk over the whole repository touches two flat arrays.

use serde::{Deserialize, Serialize};

use crate::contract::{ContractKey, Role};
use crate::fact::{FileTags, ImportFlags, ImportKind, Line};
use crate::ids::{ContractId, FileId, Id};
use crate::path::RepoPath;

/// How one file depends on another: the import kinds between them, and the flags every one of those imports carries.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Edge {
    kinds: u16,
    flags: ImportFlags,
}

impl Edge {
    pub const fn new(kind: ImportKind, flags: ImportFlags) -> Self {
        Self {
            kinds: kind.bit(),
            flags,
        }
    }

    /// Two imports between the same pair of files: the kinds add up, and a flag stays only if both imports carry it, so one runtime import makes the whole edge a runtime edge.
    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        Self {
            kinds: self.kinds | other.kinds,
            flags: self.flags.intersection(other.flags),
        }
    }

    pub const fn has(self, kind: ImportKind) -> bool {
        self.kinds & kind.bit() != 0
    }

    pub const fn flags(self) -> ImportFlags {
        self.flags
    }

    /// Nothing behind this edge exists at run time: TS `import type`, Python `if TYPE_CHECKING:`.
    pub const fn type_only(self) -> bool {
        self.flags.contains(ImportFlags::TYPE_ONLY)
            || self.flags.contains(ImportFlags::TYPE_CHECKING)
    }

    /// Only a Rust `mod` declaration: the parent owns the child file but uses nothing from it.
    pub const fn is_structural(self) -> bool {
        self.kinds == ImportKind::Module.bit()
    }

    pub fn kinds(self) -> impl Iterator<Item = ImportKind> {
        ImportKind::ALL
            .into_iter()
            .filter(move |kind| self.has(*kind))
    }
}

/// One place a contract appears.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ContractSite {
    pub file: FileId,
    pub line: Line,
    pub role: Role,
    /// The rule or module that found it.
    pub via: String,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
struct Rows<T> {
    /// `offsets[i]..offsets[i + 1]` is row `i`; one entry more than there are rows.
    offsets: Vec<u32>,
    items: Vec<T>,
}

impl<T> Default for Rows<T> {
    fn default() -> Self {
        Self {
            offsets: vec![0],
            items: Vec::new(),
        }
    }
}

impl<T> Rows<T> {
    fn from_sorted(rows: usize, sorted: impl IntoIterator<Item = (usize, T)>) -> Self {
        let mut offsets = Vec::with_capacity(rows + 1);
        let mut items = Vec::new();
        offsets.push(0);
        for (row, item) in sorted {
            while offsets.len() <= row {
                offsets.push(len_u32(items.len()));
            }
            items.push(item);
        }
        while offsets.len() <= rows {
            offsets.push(len_u32(items.len()));
        }
        Self { offsets, items }
    }

    fn row(&self, row: usize) -> &[T] {
        let start = self.offsets.get(row).copied().unwrap_or(0) as usize;
        let end = self.offsets.get(row + 1).copied().unwrap_or(0) as usize;
        self.items.get(start..end).unwrap_or(&[])
    }
}

fn len_u32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// The resolved repository: files, import edges both ways, and contract sites.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Graph {
    files: Vec<RepoPath>,
    tags: Vec<FileTags>,
    dependencies: Rows<(FileId, Edge)>,
    dependents: Rows<(FileId, Edge)>,
    contracts: Vec<ContractKey>,
    sites: Rows<ContractSite>,
    contracts_of: Rows<ContractId>,
}

impl Graph {
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Every file with its id, in path order.
    pub fn files(&self) -> impl ExactSizeIterator<Item = (FileId, &RepoPath)> {
        self.files
            .iter()
            .enumerate()
            .map(|(index, path)| (FileId::from_index(index), path))
    }

    pub fn path(&self, file: FileId) -> Option<&RepoPath> {
        self.files.get(file.index())
    }

    pub fn file_id(&self, path: &str) -> Option<FileId> {
        self.files
            .binary_search_by(|probe| probe.as_str().cmp(path))
            .ok()
            .map(FileId::from_index)
    }

    pub fn tags(&self, file: FileId) -> FileTags {
        self.tags.get(file.index()).copied().unwrap_or_default()
    }

    /// The files `file` imports.
    pub fn dependencies(&self, file: FileId) -> &[(FileId, Edge)] {
        self.dependencies.row(file.index())
    }

    /// The files that import `file`.
    pub fn dependents(&self, file: FileId) -> &[(FileId, Edge)] {
        self.dependents.row(file.index())
    }

    pub fn contract_count(&self) -> usize {
        self.contracts.len()
    }

    /// Every contract with its id, in key order.
    pub fn contracts(&self) -> impl ExactSizeIterator<Item = (ContractId, &ContractKey)> {
        self.contracts
            .iter()
            .enumerate()
            .map(|(index, key)| (ContractId::from_index(index), key))
    }

    pub fn contract(&self, contract: ContractId) -> Option<&ContractKey> {
        self.contracts.get(contract.index())
    }

    pub fn contract_id(&self, key: &ContractKey) -> Option<ContractId> {
        self.contracts
            .binary_search(key)
            .ok()
            .map(ContractId::from_index)
    }

    /// The sites of `contract`, by file and line.
    pub fn sites(&self, contract: ContractId) -> &[ContractSite] {
        self.sites.row(contract.index())
    }

    /// The contracts that appear in `file`.
    pub fn contracts_of(&self, file: FileId) -> &[ContractId] {
        self.contracts_of.row(file.index())
    }
}

/// Collects edges and sites in any order and builds a [`Graph`].
#[derive(Debug, Clone)]
pub struct GraphBuilder {
    files: Vec<RepoPath>,
    tags: Vec<FileTags>,
    edges: Vec<(FileId, FileId, Edge)>,
    sites: Vec<(ContractKey, ContractSite)>,
}

impl GraphBuilder {
    /// A builder over `files`, which must be sorted and unique: their positions become the [`FileId`]s.
    pub fn new(files: Vec<RepoPath>) -> Self {
        let tags = vec![FileTags::NONE; files.len()];
        Self {
            files,
            tags,
            edges: Vec::new(),
            sites: Vec::new(),
        }
    }

    pub fn tag(&mut self, file: FileId, tags: FileTags) {
        if let Some(slot) = self.tags.get_mut(file.index()) {
            *slot = *slot | tags;
        }
    }

    /// `from` imports `to`. A file importing itself adds nothing.
    pub fn edge(&mut self, from: FileId, to: FileId, edge: Edge) {
        if from != to {
            self.edges.push((from, to, edge));
        }
    }

    pub fn site(&mut self, key: ContractKey, site: ContractSite) {
        self.sites.push((key, site));
    }

    pub fn build(mut self) -> Graph {
        let file_count = self.files.len();

        self.edges.sort_unstable_by_key(|&(from, to, _)| (from, to));
        let mut merged: Vec<(FileId, FileId, Edge)> = Vec::with_capacity(self.edges.len());
        for (from, to, edge) in self.edges {
            match merged.last_mut() {
                Some(last) if last.0 == from && last.1 == to => last.2 = last.2.merge(edge),
                _ => merged.push((from, to, edge)),
            }
        }
        let dependencies = Rows::from_sorted(
            file_count,
            merged
                .iter()
                .map(|&(from, to, edge)| (from.index(), (to, edge))),
        );
        merged.sort_unstable_by_key(|&(from, to, _)| (to, from));
        let dependents = Rows::from_sorted(
            file_count,
            merged
                .iter()
                .map(|&(from, to, edge)| (to.index(), (from, edge))),
        );

        self.sites.sort_unstable();
        self.sites.dedup();
        let mut contracts: Vec<ContractKey> = Vec::new();
        let mut keyed_sites: Vec<(usize, ContractSite)> = Vec::with_capacity(self.sites.len());
        for (key, site) in self.sites {
            if contracts.last() != Some(&key) {
                contracts.push(key);
            }
            keyed_sites.push((contracts.len() - 1, site));
        }
        let mut by_file: Vec<(usize, ContractId)> = keyed_sites
            .iter()
            .map(|(contract, site)| (site.file.index(), ContractId::from_index(*contract)))
            .collect();
        by_file.sort_unstable();
        by_file.dedup();
        let sites = Rows::from_sorted(contracts.len(), keyed_sites);
        let contracts_of = Rows::from_sorted(file_count, by_file);

        Graph {
            files: self.files,
            tags: self.tags,
            dependencies,
            dependents,
            contracts,
            sites,
            contracts_of,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<RepoPath> {
        names
            .iter()
            .map(|name| RepoPath::normalize(name).unwrap())
            .collect()
    }

    #[test]
    fn edges_are_stored_both_ways_and_merged() {
        let mut builder = GraphBuilder::new(paths(&["a.ts", "b.ts", "c.ts"]));
        let (a, b, c) = (
            FileId::from_raw(0),
            FileId::from_raw(1),
            FileId::from_raw(2),
        );
        builder.edge(a, b, Edge::new(ImportKind::Static, ImportFlags::TYPE_ONLY));
        builder.edge(a, b, Edge::new(ImportKind::ReExportAll, ImportFlags::NONE));
        builder.edge(c, b, Edge::new(ImportKind::Static, ImportFlags::TYPE_ONLY));
        builder.edge(c, c, Edge::new(ImportKind::Static, ImportFlags::NONE));
        let graph = builder.build();

        let [(to, edge)] = graph.dependencies(a) else {
            panic!("one dependency expected")
        };
        assert_eq!(*to, b);
        assert!(
            edge.has(ImportKind::Static) && edge.has(ImportKind::ReExportAll) && !edge.type_only()
        );
        assert!(
            graph
                .dependencies(c)
                .iter()
                .all(|(_, edge)| edge.type_only())
        );
        let dependents: Vec<FileId> = graph.dependents(b).iter().map(|(from, _)| *from).collect();
        assert_eq!(dependents, vec![a, c]);
        assert!(graph.dependents(a).is_empty() && graph.dependencies(c).len() == 1);
        assert_eq!(graph.file_id("c.ts"), Some(c));
    }

    #[test]
    fn sites_group_by_contract_in_key_order() {
        let mut builder = GraphBuilder::new(paths(&["api.ts", "web.ts"]));
        let site = |file: u32, role| ContractSite {
            file: FileId::from_raw(file),
            line: 3,
            role,
            via: "test".into(),
        };
        let get = ContractKey::parse("http GET /x").unwrap();
        let env = ContractKey::parse("env API_URL").unwrap();
        builder.site(get.clone(), site(1, Role::Calls));
        builder.site(get.clone(), site(0, Role::Serves));
        builder.site(get.clone(), site(0, Role::Serves));
        builder.site(env.clone(), site(1, Role::Calls));
        let graph = builder.build();

        assert_eq!(
            graph
                .contracts()
                .map(|(_, key)| key.to_string())
                .collect::<Vec<_>>(),
            ["http GET /x", "env API_URL"],
            "kinds sort in the order ContractKind declares them, not by name"
        );
        let get_id = graph.contract_id(&get).unwrap();
        assert_eq!(graph.sites(get_id).len(), 2);
        assert_eq!(graph.contracts_of(FileId::from_raw(1)).len(), 2);
    }
}
