//! The graph: files joined by imports, and contracts joined to the files that declare, serve and call them.
//!
//! Both directions of the import relation are stored as compressed sparse rows, so dependents and dependencies of a file are one slice each, and a walk over the whole repository touches two flat arrays.
//!
//! An edge also says what the importer takes from the file and what it passes on, by name. The names are numbered once, in byte order, so that a set of names is a sorted slice of ids, and the binding of an edge is a row in a table that both directions of the edge share.

mod symbols;

use std::collections::HashMap;

use foldhash::fast::RandomState;
use serde::{Deserialize, Serialize};

use crate::contract::{ContractKey, Role};
use crate::fact::{FileTags, Import, ImportFlags, ImportKind, Line, Symbols};
use crate::ids::{ContractId, FileId, Id, NameId};
use crate::path::RepoPath;

pub use symbols::{Declaration, FileSymbols};

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
    /// A row of the graph's bindings. Row 0 is the plain binding, which takes everything and passes nothing on.
    binding: u32,
}

impl Edge {
    /// The edge of one import, with the plain binding. [`GraphBuilder::import`] gives it the binding the import has.
    const fn new(kind: ImportKind, flags: ImportFlags) -> Self {
        Self {
            kinds: kind.bit(),
            flags,
            binding: 0,
        }
    }

    /// Two imports between the same pair of files: the kinds add up, and a flag stays only if both imports carry it, so one runtime import makes the whole edge a runtime edge. The binding stays this edge's, because merging bindings takes the names of the graph, which only [`GraphBuilder`] holds.
    #[must_use]
    const fn merge(self, other: Self) -> Self {
        Self {
            kinds: self.kinds | other.kinds,
            flags: self.flags.intersection(other.flags),
            binding: self.binding,
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

    /// Whether the importer takes everything the file exports and passes nothing on, as every import of a connector that records no names does. Those edges share one binding row, so a walk tells them apart without reading the binding.
    pub const fn is_plain(self) -> bool {
        self.binding == 0
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

    /// Appends one row after the last.
    fn push_row(&mut self, items: impl IntoIterator<Item = T>) {
        self.items.extend(items);
        self.offsets.push(len_u32(self.items.len()));
    }
}

fn len_u32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// Which names of the file an importer takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Uses<'g> {
    /// Everything the file exports: a namespace, a side effect, a dynamic import, a `require`, or an import whose connector records no names.
    All,
    /// Only these names, sorted. The slice is empty for an importer that takes nothing by name, such as a named re-export.
    Names(&'g [NameId]),
}

/// What the importer of an edge takes from the file it imports and passes on, by name. The names are ids of the graph, in byte order wherever there is more than one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding<'g> {
    /// The names the importer uses itself.
    pub uses: Uses<'g>,
    /// The names it passes on as a named re-export, as pairs of the name it exports and the name it takes, sorted. The name taken is `*` for `export * as ns from`.
    pub passes: &'g [(NameId, NameId)],
    /// Whether it re-exports everything the file exports, as `export * from` does.
    pub star: bool,
}

/// Set in the flags of a binding that takes everything.
const USES_ALL: u8 = 1;
/// Set in the flags of a binding that re-exports everything.
const STAR: u8 = 2;

/// The bindings of the edges, row by row. Row 0 is the plain binding that takes everything and passes nothing on, which most edges are, so they share it.
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
struct Bindings {
    /// The names a row uses; empty for a row that takes everything.
    uses: Rows<NameId>,
    passes: Rows<(NameId, NameId)>,
    /// [`USES_ALL`] and [`STAR`] per row.
    flags: Vec<u8>,
}

impl Default for Bindings {
    fn default() -> Self {
        Self {
            uses: Rows::default(),
            passes: Rows::default(),
            flags: Vec::new(),
        }
        .with_row(&[], &[], USES_ALL)
    }
}

impl Bindings {
    fn with_row(mut self, uses: &[NameId], passes: &[(NameId, NameId)], flags: u8) -> Self {
        self.uses.push_row(uses.iter().copied());
        self.passes.push_row(passes.iter().copied());
        self.flags.push(flags);
        self
    }
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
    /// Every name an import takes or passes on and a file exports, once, in byte order. A [`NameId`] is a position in it.
    names: Vec<String>,
    bindings: Bindings,
    /// The names each file exports itself, by file.
    own_exports: Rows<NameId>,
    /// Whether the exports of each file are known; an unknown file has an empty row.
    exports_known: Vec<bool>,
    /// The declarations of the files whose connector records them.
    symbols: symbols::Table,
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

    /// What the importer of `edge` takes from the file the edge leads to and passes on. An edge that does not belong to this graph takes everything.
    pub fn binding(&self, edge: Edge) -> Binding<'_> {
        let row = edge.binding as usize;
        let flags = self.bindings.flags.get(row).copied().unwrap_or(USES_ALL);
        Binding {
            uses: if flags & USES_ALL == 0 {
                Uses::Names(self.bindings.uses.row(row))
            } else {
                Uses::All
            },
            passes: self.bindings.passes.row(row),
            star: flags & STAR != 0,
        }
    }

    /// The names `file` exports itself, in byte order and without the names an `export *` brings in. `None` when the connector could not tell, which is not the same as an empty slice: a barrel made only of `export *` exports nothing itself.
    pub fn exports(&self, file: FileId) -> Option<&[NameId]> {
        self.exports_known
            .get(file.index())
            .copied()
            .unwrap_or(false)
            .then(|| self.own_exports.row(file.index()))
    }

    /// The declarations of `file`, what its code uses as it loads and what it exports from a local of another name. `None` when its connector records no declarations, which is not the same as a file that declares nothing.
    pub fn symbols(&self, file: FileId) -> Option<FileSymbols<'_>> {
        self.symbols.of(file)
    }

    /// The name `id` stands for.
    pub fn name(&self, id: NameId) -> Option<&str> {
        self.names.get(id.index()).map(String::as_str)
    }

    /// The id of `name`, `None` when no import and no export of the graph holds it.
    pub fn name_id(&self, name: &str) -> Option<NameId> {
        self.names
            .binary_search_by(|probe| probe.as_str().cmp(name))
            .ok()
            .map(NameId::from_index)
    }
}

/// What an import takes and passes on, with the names numbered in the order the builder met them.
#[derive(Clone, Debug, Default)]
struct Taken {
    /// Whether the importer takes everything the file exports.
    all: bool,
    /// Whether it re-exports everything.
    star: bool,
    uses: Vec<u32>,
    passes: Vec<(u32, u32)>,
}

impl Taken {
    /// Two imports between the same pair of files: `all` absorbs the names used, the passes add up and `star` holds if either has it.
    fn merge(&mut self, other: Self) {
        self.all |= other.all;
        self.star |= other.star;
        self.uses.extend(other.uses);
        self.passes.extend(other.passes);
    }
}

/// Collects edges and sites in any order and builds a [`Graph`].
#[derive(Debug, Clone)]
pub struct GraphBuilder {
    files: Vec<RepoPath>,
    tags: Vec<FileTags>,
    edges: Vec<(FileId, FileId, Edge, Taken)>,
    sites: Vec<(ContractKey, ContractSite)>,
    /// Every name met so far with the number it got, which is the order it was met in. The graph renumbers them in byte order when it is built, and nothing reads this map in its own order.
    numbers: HashMap<String, u32, RandomState>,
    /// The names each file exports itself, `None` for a file that is not known to.
    exports: Vec<Option<Vec<u32>>>,
    symbols: symbols::Table,
}

impl GraphBuilder {
    /// A builder over `files`, which must be sorted and unique: their positions become the [`FileId`]s.
    pub fn new(files: Vec<RepoPath>) -> Self {
        let tags = vec![FileTags::NONE; files.len()];
        let exports = vec![None; files.len()];
        let symbols = symbols::Table::new(files.len());
        Self {
            files,
            tags,
            edges: Vec::new(),
            sites: Vec::new(),
            numbers: HashMap::default(),
            exports,
            symbols,
        }
    }

    pub fn tag(&mut self, file: FileId, tags: FileTags) {
        if let Some(slot) = self.tags.get_mut(file.index()) {
            *slot = *slot | tags;
        }
    }

    /// The number of `name`, a new one the first time it is met.
    fn number(&mut self, name: &str) -> u32 {
        if let Some(&number) = self.numbers.get(name) {
            return number;
        }
        let number = len_u32(self.numbers.len());
        self.numbers.insert(name.to_owned(), number);
        number
    }

    /// What `import` takes and passes on. A named re-export passes its names under the names it exports them as and uses nothing; `export *` re-exports everything and uses nothing; a static import that names what it takes, and not a namespace, uses those names; every other import takes everything. Dynamic imports and `require` take everything even when a connector names what is destructured from them, because what they hand back is the whole module.
    fn taken(&mut self, import: &Import) -> Taken {
        let names = &import.names;
        match import.kind {
            ImportKind::ReExportAll => Taken {
                star: true,
                ..Taken::default()
            },
            ImportKind::ReExport if !names.is_empty() && names.len() == import.exported.len() => {
                Taken {
                    passes: import
                        .exported
                        .iter()
                        .zip(names)
                        .map(|(exported, name)| (self.number(exported), self.number(name)))
                        .collect(),
                    ..Taken::default()
                }
            }
            ImportKind::Static if !names.is_empty() && !names.iter().any(|name| name == "*") => {
                Taken {
                    uses: names.iter().map(|name| self.number(name)).collect(),
                    ..Taken::default()
                }
            }
            _ => Taken {
                all: true,
                ..Taken::default()
            },
        }
    }

    /// `from` has `import`, which resolved to `to`. A file importing itself adds nothing.
    pub fn import(&mut self, from: FileId, to: FileId, import: &Import) {
        if from != to {
            let taken = self.taken(import);
            let edge = Edge::new(import.kind, import.flags);
            self.edges.push((from, to, edge, taken));
        }
    }

    /// The names `file` exports itself, `None` when they are not known.
    pub fn exports(&mut self, file: FileId, names: Option<&[String]>) {
        let numbers = names.map(|names| names.iter().map(|name| self.number(name)).collect());
        if let Some(slot) = self.exports.get_mut(file.index()) {
            *slot = numbers;
        }
    }

    /// The declarations of `file`, `None` when its connector records none. They are copied into one buffer in the order of the calls, so that the same tree gives the same bytes when the calls come in file order, as the engine's do.
    pub fn symbols(&mut self, file: FileId, symbols: Option<&Symbols>) {
        self.symbols.insert(file, symbols);
    }

    pub fn site(&mut self, key: ContractKey, site: ContractSite) {
        self.sites.push((key, site));
    }

    pub fn build(mut self) -> Graph {
        let file_count = self.files.len();

        // The order of a hash map never reaches the output: the names are sorted, and unique, before anything is numbered from them.
        let mut sorted: Vec<(String, u32)> = self.numbers.into_iter().collect();
        sorted.sort_unstable();
        let mut place = vec![NameId::from_raw(0); sorted.len()];
        for (position, (_, number)) in sorted.iter().enumerate() {
            if let Some(slot) = place.get_mut(*number as usize) {
                *slot = NameId::from_index(position);
            }
        }
        let names: Vec<String> = sorted.into_iter().map(|(name, _)| name).collect();
        // Every number was handed out by `number`, so the fallback is never read; it names no name if it ever is.
        let id_of = |number: u32| {
            place
                .get(number as usize)
                .copied()
                .unwrap_or(NameId::from_raw(u32::MAX))
        };
        let sorted_ids = |numbers: &[u32]| {
            let mut ids: Vec<NameId> = numbers.iter().map(|&number| id_of(number)).collect();
            ids.sort_unstable();
            ids.dedup();
            ids
        };

        self.edges
            .sort_unstable_by_key(|&(from, to, _, _)| (from, to));
        let mut merged: Vec<(FileId, FileId, Edge, Taken)> = Vec::with_capacity(self.edges.len());
        for (from, to, edge, taken) in self.edges {
            match merged.last_mut() {
                Some(last) if last.0 == from && last.1 == to => {
                    last.2 = last.2.merge(edge);
                    last.3.merge(taken);
                }
                _ => merged.push((from, to, edge, taken)),
            }
        }
        let mut bindings = Bindings::default();
        for (_, _, edge, taken) in &mut merged {
            if taken.all && !taken.star && taken.passes.is_empty() {
                continue;
            }
            let uses = if taken.all {
                Vec::new()
            } else {
                sorted_ids(&taken.uses)
            };
            let mut passes: Vec<(NameId, NameId)> = taken
                .passes
                .iter()
                .map(|&(exported, name)| (id_of(exported), id_of(name)))
                .collect();
            passes.sort_unstable();
            passes.dedup();
            let flags =
                (if taken.all { USES_ALL } else { 0 }) | (if taken.star { STAR } else { 0 });
            edge.binding = len_u32(bindings.flags.len());
            bindings = bindings.with_row(&uses, &passes, flags);
        }
        let dependencies = Rows::from_sorted(
            file_count,
            merged
                .iter()
                .map(|&(from, to, edge, _)| (from.index(), (to, edge))),
        );
        merged.sort_unstable_by_key(|&(from, to, _, _)| (to, from));
        let dependents = Rows::from_sorted(
            file_count,
            merged
                .iter()
                .map(|&(from, to, edge, _)| (to.index(), (from, edge))),
        );

        let exports_known: Vec<bool> = self.exports.iter().map(Option::is_some).collect();
        let own_exports = Rows::from_sorted(
            file_count,
            self.exports.iter().enumerate().flat_map(|(file, numbers)| {
                let ids = sorted_ids(numbers.as_deref().unwrap_or_default());
                ids.into_iter().map(move |id| (file, id))
            }),
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
            names,
            bindings,
            own_exports,
            exports_known,
            symbols: self.symbols,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::owned;

    fn paths(names: &[&str]) -> Vec<RepoPath> {
        names
            .iter()
            .map(|name| RepoPath::normalize(name).unwrap())
            .collect()
    }

    fn plain(kind: ImportKind, flags: ImportFlags) -> Import {
        Import {
            flags,
            ..Import::new("./b", kind, 1)
        }
    }

    /// An import as a connector records it, with the names it takes and the names it exports them as.
    fn named(kind: ImportKind, names: &[&str], exported: &[&str]) -> Import {
        Import {
            names: owned(names),
            exported: owned(exported),
            ..Import::new("./b", kind, 1)
        }
    }

    /// What `a.ts` takes from `b.ts`, with the names spelled out: the names it uses, `None` for everything; the pairs it passes on; whether it re-exports everything.
    type Taking = (Option<Vec<String>>, Vec<(String, String)>, bool);

    /// The graph of two files in which `a.ts` has made `imports` of `b.ts`.
    fn after(imports: &[Import]) -> Graph {
        let mut builder = GraphBuilder::new(paths(&["a.ts", "b.ts"]));
        for import in imports {
            builder.import(FileId::from_raw(0), FileId::from_raw(1), import);
        }
        builder.build()
    }

    fn taking(imports: &[Import]) -> Taking {
        let graph = after(imports);
        let [(_, edge)] = graph.dependencies(FileId::from_raw(0)) else {
            panic!("one dependency expected")
        };
        let binding = graph.binding(*edge);
        let name = |id| graph.name(id).unwrap().to_owned();
        let uses = match binding.uses {
            Uses::All => None,
            Uses::Names(ids) => Some(ids.iter().map(|&id| name(id)).collect()),
        };
        let passes = binding
            .passes
            .iter()
            .map(|&(exported, source)| (name(exported), name(source)))
            .collect();
        (uses, passes, binding.star)
    }

    fn pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|&(exported, source)| (exported.to_owned(), source.to_owned()))
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
        let import = |kind, flags| plain(kind, flags);
        builder.import(a, b, &import(ImportKind::Static, ImportFlags::TYPE_ONLY));
        builder.import(a, b, &import(ImportKind::ReExportAll, ImportFlags::NONE));
        builder.import(c, b, &import(ImportKind::Static, ImportFlags::TYPE_ONLY));
        builder.import(c, c, &import(ImportKind::Static, ImportFlags::NONE));
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
    fn every_import_form_binds_what_it_takes() {
        let everything: Taking = (None, Vec::new(), false);
        let only = |names: &[&str]| -> Taking { (Some(owned(names)), Vec::new(), false) };
        let none = ImportFlags::NONE;
        let cases: Vec<(&str, Import, Taking)> = vec![
            (
                "an import that names nothing",
                plain(ImportKind::Static, none),
                everything.clone(),
            ),
            (
                "named imports, in byte order",
                named(ImportKind::Static, &["useFoo", "useBar"], &[]),
                only(&["useBar", "useFoo"]),
            ),
            (
                "a default import beside a named one",
                named(ImportKind::Static, &["default", "b"], &[]),
                only(&["b", "default"]),
            ),
            (
                "a namespace import",
                named(ImportKind::Static, &["*"], &[]),
                everything.clone(),
            ),
            (
                "a namespace beside a name",
                named(ImportKind::Static, &["a", "*"], &[]),
                everything.clone(),
            ),
            (
                "a side-effect import",
                plain(ImportKind::SideEffect, none),
                everything.clone(),
            ),
            (
                "a destructured dynamic import, which a rest element may widen",
                named(ImportKind::Dynamic, &["a"], &[]),
                everything.clone(),
            ),
            (
                "a destructured require",
                named(ImportKind::Require, &["a"], &[]),
                everything.clone(),
            ),
            (
                "a mod declaration, whose names are a path attribute",
                named(ImportKind::Module, &["renamed.rs"], &[]),
                everything.clone(),
            ),
            (
                "named re-exports under aliases, which use nothing",
                named(ImportKind::ReExport, &["useFoo", "bar"], &["useBaz", "bar"]),
                (
                    Some(Vec::new()),
                    pairs(&[("bar", "bar"), ("useBaz", "useFoo")]),
                    false,
                ),
            ),
            (
                "export * as ns",
                named(ImportKind::ReExport, &["*"], &["ns"]),
                (Some(Vec::new()), pairs(&[("ns", "*")]), false),
            ),
            (
                "export *",
                plain(ImportKind::ReExportAll, none),
                (Some(Vec::new()), Vec::new(), true),
            ),
            (
                "a re-export that records no export names",
                named(ImportKind::ReExport, &["b"], &[]),
                everything.clone(),
            ),
            (
                "a re-export whose export names do not match its names",
                named(ImportKind::ReExport, &["a", "b"], &["x"]),
                everything,
            ),
        ];
        for (form, import, expected) in cases {
            assert_eq!(taking(&[import]), expected, "{form}");
        }
    }

    #[test]
    fn two_imports_of_one_file_merge_into_one_binding() {
        let uses = |names: &[&str]| named(ImportKind::Static, names, &[]);
        let passes = |pairs: &[(&str, &str)]| {
            let (exported, names): (Vec<&str>, Vec<&str>) = pairs.iter().copied().unzip();
            named(ImportKind::ReExport, &names, &exported)
        };
        let all = plain(ImportKind::SideEffect, ImportFlags::NONE);
        let star = plain(ImportKind::ReExportAll, ImportFlags::NONE);

        assert_eq!(
            taking(&[uses(&["b", "a"]), uses(&["c", "a"])]),
            (Some(owned(&["a", "b", "c"])), Vec::new(), false),
            "names add up, once each"
        );
        assert_eq!(
            taking(&[uses(&["a"]), all.clone()]),
            (None, Vec::new(), false),
            "an import of everything absorbs the names of the others, in either order"
        );
        assert_eq!(
            taking(&[all, uses(&["a"])]),
            (None, Vec::new(), false),
            "an import of everything absorbs the names of the others, in either order"
        );
        assert_eq!(
            taking(&[passes(&[("y", "x")]), passes(&[("x", "x"), ("y", "x")])]),
            (Some(Vec::new()), pairs(&[("x", "x"), ("y", "x")]), false),
            "passes add up, once each"
        );
        assert_eq!(
            taking(&[star.clone(), passes(&[("x", "y")])]),
            (Some(Vec::new()), pairs(&[("x", "y")]), true),
            "export * beside a named re-export of the same file"
        );
        assert_eq!(
            taking(&[uses(&["a"]), passes(&[("x", "b")]), star]),
            (Some(owned(&["a"])), pairs(&[("x", "b")]), true),
            "an import and a re-export of the same file"
        );
    }

    #[test]
    fn merged_edges_keep_their_kinds_flags_and_own_binding_row() {
        let graph = after(&[
            Import {
                flags: ImportFlags::TYPE_ONLY,
                ..named(ImportKind::Static, &["a"], &[])
            },
            plain(ImportKind::ReExportAll, ImportFlags::NONE),
        ]);
        let [(_, edge)] = graph.dependencies(FileId::from_raw(0)) else {
            panic!("one dependency expected")
        };
        assert!(edge.has(ImportKind::Static) && edge.has(ImportKind::ReExportAll));
        assert!(!edge.type_only(), "one runtime import makes a runtime edge");
        let [(_, seen)] = graph.dependents(FileId::from_raw(1)) else {
            panic!("one dependent expected")
        };
        assert_eq!(
            graph.binding(*seen),
            graph.binding(*edge),
            "both directions of an edge share its binding"
        );
    }

    #[test]
    fn names_are_numbered_in_byte_order_whatever_the_order_they_came_in() {
        let mut builder = GraphBuilder::new(paths(&["a.ts", "b.ts"]));
        let (a, b) = (FileId::from_raw(0), FileId::from_raw(1));
        builder.exports(b, Some(&owned(&["zed", "alpha", "alpha"])));
        builder.import(a, b, &named(ImportKind::Static, &["Mid", "alpha"], &[]));
        let graph = builder.build();

        let id = |name: &str| graph.name_id(name).unwrap();
        assert!(id("Mid") < id("alpha") && id("alpha") < id("zed"));
        assert_eq!(graph.name(id("zed")), Some("zed"));
        assert_eq!(graph.name_id("missing"), None);
        assert_eq!(graph.name(NameId::from_raw(3)), None);
        let [(_, edge)] = graph.dependencies(a) else {
            panic!("one dependency expected")
        };
        assert_eq!(
            graph.binding(*edge).uses,
            Uses::Names(&[id("Mid"), id("alpha")])
        );
    }

    #[test]
    fn a_file_exports_its_own_names_or_they_are_unknown() {
        let mut builder = GraphBuilder::new(paths(&["a.ts", "b.ts", "c.ts"]));
        let (a, b, c) = (
            FileId::from_raw(0),
            FileId::from_raw(1),
            FileId::from_raw(2),
        );
        builder.exports(a, Some(&owned(&["b", "a", "b"])));
        builder.exports(b, Some(&[]));
        let graph = builder.build();

        let named = |file| -> Option<Vec<&str>> {
            graph
                .exports(file)?
                .iter()
                .map(|&id| graph.name(id))
                .collect()
        };
        assert_eq!(named(a), Some(vec!["a", "b"]), "sorted, once each");
        assert_eq!(named(b), Some(Vec::new()), "a file that exports nothing");
        assert_eq!(named(c), None, "a file that says nothing is unknown");
        assert_eq!(graph.exports(FileId::from_raw(9)), None);
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
