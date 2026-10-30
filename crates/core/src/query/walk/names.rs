//! The walk behind `impact` asked about names: the declarations that refer to what a change reaches, hop by hop, in place of the files that import it.
//!
//! A hop goes from the declarations the walk has reached in a file to the declarations that refer to them, in the file itself and in the files that import them under the names the file exports them as. A barrel on the way, and a file that imports a name and exports it again, are listed and add no hop. Code that runs as a file loads is a caller of what it refers to, and a file reached at load reaches the files that load it. A file whose connector records no declarations is followed whole, as the walk over files follows every file.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::contract::Role;
use crate::fact::{Import, ImportKind, Line, MEMBER_SEPARATOR};
use crate::graph::{ContractSite, Declaration, Edge, FileSymbols, Graph};
use crate::ids::{ContractId, FileId, Id, NameId};
use crate::index::Index;
use crate::query::cross::{Crossed, Crossing, Direction, sites_of};

use super::{Cause, Reached, holds};

/// What the answer says about the walk beside the files it reached.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::query) enum Note {
    /// `file` declares no `name`, so nothing was reached from it by that name.
    Undeclared { file: FileId, name: String },
    /// The connector of `file` records no declarations, so the walk follows the file whole.
    NoDeclarations { file: FileId },
    /// A call through a value names no class, so the callers of these members are found through the declarations that refer to their classes. Each is a member with its class, in byte order of the member.
    ThroughClass { members: Vec<(String, String)> },
    /// The depth ended the walk while callers were left to follow.
    Stopped,
}

/// A file the walk by name reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::query) struct Hit {
    pub(in crate::query) file: FileId,
    /// Hops from the nearest declaration asked about.
    pub(in crate::query) distance: u32,
    /// The first way the walk found, chosen as in the walk over files: of the equally near ways the one from the file with the smallest path, and from that file an import before a contract.
    pub(in crate::query) cause: Cause,
    /// The declarations of the file the walk reached, each as its index among those of the symbols of the file and its hops from the nearest declaration asked about, in byte order of their names. The declarations asked about are not among them.
    pub(in crate::query) callers: Vec<(usize, u32)>,
    /// The hops from the nearest declaration asked about to the code that runs as the file loads, `None` when the walk did not reach it.
    pub(in crate::query) load: Option<u32>,
}

impl From<Reached> for Hit {
    /// A file the walk over files reached, which has no callers to name and no load.
    fn from(reached: Reached) -> Self {
        Self {
            file: reached.file,
            distance: reached.distance,
            cause: reached.cause,
            callers: Vec::new(),
            load: None,
        }
    }
}

/// What the walk by name found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::query) struct Walked {
    /// Nearest first, then by file.
    pub(in crate::query) hits: Vec<Hit>,
    pub(in crate::query) notes: Vec<Note>,
}

/// Every file that holds a declaration, or the load, that refers to the `names` of the declarations of `changed` within `depth` hops, with the callers in each. The files of `changed` are listed only when their own code refers to the names.
pub(in crate::query) fn reach_names(
    index: &Index,
    crossing: &Crossing<'_>,
    changed: &[FileId],
    names: &[String],
    depth: u32,
) -> Walked {
    let mut walk = Walk {
        index,
        graph: &index.graph,
        crossing,
        files: BTreeMap::new(),
        through_class: BTreeSet::new(),
    };
    let mut crossed = Crossed::new(Direction::ToCallers, &index.graph);
    let mut notes = BTreeSet::new();
    let mut asked = changed.to_vec();
    asked.sort_unstable();
    asked.dedup();
    let mut frontier = walk.start(&asked, names, &mut notes);
    let mut distance = 0;
    while !frontier.is_empty() && distance < depth {
        let layer = Step::new(&walk, &mut crossed).expand(&frontier);
        distance += 1;
        frontier = walk.apply(layer, distance);
    }
    // The depth ended the walk here. One more layer, which nothing records, tells whether it left callers behind.
    if !frontier.is_empty()
        && !Step::new(&walk, &mut crossed)
            .expand(&frontier)
            .units
            .is_empty()
    {
        notes.insert(Note::Stopped);
    }
    let hits = walk.hits();
    // One note for every member, so that the answer can cut the list of them as it cuts every list of names.
    if !walk.through_class.is_empty() {
        notes.insert(Note::ThroughClass {
            members: walk.through_class.into_iter().collect(),
        });
    }
    Walked {
        hits,
        notes: notes.into_iter().collect(),
    }
}

/// A part of a file the walk reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    /// The declaration at this index among those of the file.
    Decl(usize),
    /// The code that runs as the file loads.
    Load,
    /// The whole file: its connector records no declarations, or the walk cannot tell which of them a name reaches.
    Whole,
}

/// A part of one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Unit {
    file: FileId,
    part: Part,
}

/// A name a file exports that the walk has reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Name<'a> {
    Is(&'a str),
    /// Every name of the file: the file is followed whole, or exports its declarations under names that cannot be listed.
    Any,
}

impl Name<'_> {
    /// Whether an import that takes `taken` from the file gets this name. A namespace takes every name.
    fn takes(self, taken: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Is(name) => taken == "*" || taken == name,
        }
    }
}

/// What the walk knows of a file it has reached.
#[derive(Debug, Default)]
struct Touch {
    /// The hops to the first way found into the file, and that way. A file the walk started from has none until its own code refers to the names.
    listed: Option<(u32, Cause)>,
    /// The declarations reached, as indices among those of the file, each with its distance. Those asked about are at 0.
    decls: BTreeMap<usize, u32>,
    /// The distance of the code that runs as the file loads, once the walk reaches it.
    load: Option<u32>,
    whole: bool,
}

/// A way into a file that one layer found, in the order the layer found it.
struct Candidate {
    file: FileId,
    /// The file the way comes from, which settles which of the equally near ways is the first.
    from: FileId,
    cause: Cause,
}

/// What one layer found: the parts of files it reached, the ways into the files, and the members of classes it looked for the callers of.
#[derive(Default)]
struct Layer {
    units: BTreeSet<Unit>,
    candidates: Vec<Candidate>,
    /// Each member with its class.
    through_class: BTreeSet<(String, String)>,
}

struct Walk<'a> {
    index: &'a Index,
    graph: &'a Graph,
    crossing: &'a Crossing<'a>,
    files: BTreeMap<FileId, Touch>,
    /// The members of classes the walk looked for the callers of through their class, each with its class.
    through_class: BTreeSet<(String, String)>,
}

impl Walk<'_> {
    /// Marks the declarations asked about as reached at distance 0 and returns them as the first frontier. A file without declarations is reached whole instead.
    fn start(
        &mut self,
        changed: &[FileId],
        names: &[String],
        notes: &mut BTreeSet<Note>,
    ) -> Vec<Unit> {
        let mut started = BTreeSet::new();
        for &file in changed {
            let touch = self.files.entry(file).or_default();
            let Some(symbols) = self.graph.symbols(file) else {
                notes.insert(Note::NoDeclarations { file });
                touch.whole = true;
                started.insert(Unit {
                    file,
                    part: Part::Whole,
                });
                continue;
            };
            for name in names {
                let mut declared = false;
                for (index, decl) in symbols.declarations().enumerate() {
                    if decl.name() == name {
                        declared = true;
                        touch.decls.insert(index, 0);
                        started.insert(Unit {
                            file,
                            part: Part::Decl(index),
                        });
                    }
                }
                if !declared {
                    notes.insert(Note::Undeclared {
                        file,
                        name: name.clone(),
                    });
                }
            }
        }
        started.into_iter().collect()
    }

    fn reached(&self, unit: Unit) -> bool {
        self.files
            .get(&unit.file)
            .is_some_and(|touch| match unit.part {
                Part::Decl(index) => touch.decls.contains_key(&index),
                Part::Load => touch.load.is_some(),
                Part::Whole => touch.whole,
            })
    }

    /// The names of the declarations of `file` the walk started from, in byte order.
    fn asked(&self, file: FileId) -> Vec<String> {
        let (Some(touch), Some(symbols)) = (self.files.get(&file), self.graph.symbols(file)) else {
            return Vec::new();
        };
        let mut names: Vec<String> = touch
            .decls
            .iter()
            .filter(|&(_, &distance)| distance == 0)
            .filter_map(|(&index, _)| symbols.declaration(index))
            .map(|decl| decl.name().to_owned())
            .collect();
        names.sort_unstable();
        names
    }

    /// Records what the layer found at `distance`, and returns the parts of files it reached, which are the next layer's frontier.
    fn apply(&mut self, layer: Layer, distance: u32) -> Vec<Unit> {
        self.through_class.extend(layer.through_class);
        // A stable sort keeps the order the layer found the ways in, which is contract order within one file.
        let mut candidates = layer.candidates;
        candidates
            .sort_by_key(|candidate| (candidate.file, candidate.from, rank(&candidate.cause)));
        let mut ways: Vec<Candidate> = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if let Some(last) = ways.last_mut()
                && last.file == candidate.file
                && last.from == candidate.from
                && merge(&mut last.cause, &candidate.cause)
            {
                continue;
            }
            ways.push(candidate);
        }
        for way in ways {
            let touch = self.files.entry(way.file).or_default();
            if touch.listed.is_none() {
                touch.listed = Some((distance, way.cause));
            }
        }
        for unit in &layer.units {
            let touch = self.files.entry(unit.file).or_default();
            match unit.part {
                Part::Decl(index) => {
                    touch.decls.entry(index).or_insert(distance);
                }
                Part::Load => {
                    touch.load.get_or_insert(distance);
                }
                Part::Whole => touch.whole = true,
            }
        }
        layer.units.into_iter().collect()
    }

    /// The files the walk listed, nearest first and then by file.
    fn hits(&self) -> Vec<Hit> {
        let mut hits: Vec<Hit> = self
            .files
            .iter()
            .filter_map(|(&file, touch)| {
                let (distance, cause) = touch.listed.clone()?;
                let symbols = self.graph.symbols(file);
                let name = |index: usize| {
                    symbols
                        .and_then(|symbols| symbols.declaration(index))
                        .map(Declaration::name)
                };
                let mut callers: Vec<(usize, u32)> = touch
                    .decls
                    .iter()
                    .filter(|&(_, &distance)| distance > 0)
                    .map(|(&index, &distance)| (index, distance))
                    .collect();
                callers.sort_by(|&(left, _), &(right, _)| name(left).cmp(&name(right)));
                Some(Hit {
                    file,
                    distance,
                    cause,
                    callers,
                    load: touch.load,
                })
            })
            .collect();
        hits.sort_by_key(|hit| (hit.distance, hit.file));
        hits
    }
}

/// How early a way into a file wins when two come from the same file: a declaration of the file itself, then an import, then a re-export, then a contract.
fn rank(cause: &Cause) -> u8 {
    match cause {
        Cause::Declares { .. } => 0,
        Cause::Imports { .. } => 1,
        Cause::ReExports { .. } => 2,
        Cause::Contract { .. } => 3,
    }
}

/// Folds `other` into `cause` when both are the same kind of way from the same file, so the names of both are listed. Says whether it did.
fn merge(cause: &mut Cause, other: &Cause) -> bool {
    match (cause, other) {
        (
            Cause::Imports { file, names },
            Cause::Imports {
                file: same,
                names: more,
            },
        ) if file == same => union(names, more),
        (
            Cause::ReExports { file, names },
            Cause::ReExports {
                file: same,
                names: more,
            },
        ) if file == same => union(names, more),
        (Cause::Declares { names }, Cause::Declares { names: more }) => union(names, more),
        _ => return false,
    }
    true
}

/// Adds `more` to the sorted `names`, each once.
fn union<T: Ord + Clone>(names: &mut Vec<T>, more: &[T]) {
    names.extend_from_slice(more);
    names.sort_unstable();
    names.dedup();
}

/// What one importer takes from a file, collected over its imports of it.
struct Via<'a> {
    /// The names of the file it takes that the walk reached, for the reason it is listed with.
    taken: Taken,
    /// The names it binds them under, which is what its declarations refer to.
    locals: Vec<&'a str>,
    /// What it passes on as a named re-export, as the name it takes and the name it exports.
    passes: Vec<(NameId, NameId)>,
    /// Whether it uses a name beyond its locals: it exports one again, takes every name by a dynamic import or a `require`, or takes names by an import type.
    used: bool,
    /// Whether it re-exports a name.
    barrel: bool,
    /// Whether it must be followed whole.
    whole: bool,
}

/// The names an importer takes from a file.
enum Taken {
    Named(Vec<NameId>),
    /// Every name, which the reason leaves unsaid.
    Everything,
}

impl Taken {
    fn name(&mut self, graph: &Graph, taken: &str) {
        match (&mut *self, graph.name_id(taken)) {
            (Self::Named(names), Some(id)) if taken != "*" => names.push(id),
            (Self::Named(_), _) => *self = Self::Everything,
            (Self::Everything, _) => {}
        }
    }

    fn into_names(self) -> Vec<NameId> {
        match self {
            Self::Named(mut names) => {
                names.sort_unstable();
                names.dedup();
                names
            }
            Self::Everything => Vec::new(),
        }
    }
}

/// One layer of the walk: the parts of files it was given, and what they reach.
struct Step<'w, 'a> {
    walk: &'w Walk<'a>,
    crossed: &'w mut Crossed,
    layer: Layer,
}

impl<'w, 'a> Step<'w, 'a> {
    fn new(walk: &'w Walk<'a>, crossed: &'w mut Crossed) -> Self {
        Self {
            walk,
            crossed,
            layer: Layer::default(),
        }
    }

    /// What the `frontier`, which is sorted, reaches one hop further.
    fn expand(mut self, frontier: &[Unit]) -> Layer {
        let mut rest = frontier;
        while let Some(first) = rest.first() {
            let file = first.file;
            let end = rest.partition_point(|unit| unit.file == file);
            let Some((group, tail)) = rest.split_at_checked(end) else {
                break;
            };
            self.file(file, group);
            rest = tail;
        }
        self.layer
    }

    /// The parts of one file that sit at the frontier. Imports come before contracts, as in the walk over files.
    fn file(&mut self, file: FileId, group: &[Unit]) {
        let mut declared = Vec::new();
        let (mut load, mut whole) = (false, false);
        for unit in group {
            match unit.part {
                Part::Decl(index) => declared.push(index),
                Part::Load => load = true,
                Part::Whole => whole = true,
            }
        }
        let symbols = self
            .walk
            .graph
            .symbols(file)
            .filter(|_| !declared.is_empty());
        if let Some(symbols) = symbols {
            self.callers_in(file, symbols, &declared);
            self.exports(file, symbols, &declared);
        }
        if load {
            self.loaders(file);
        }
        if whole {
            self.flow(file, BTreeSet::from([Name::Any]));
        }
        if let Some(symbols) = symbols {
            self.declared_contracts(file, symbols, &declared);
        }
        if load || whole {
            self.all_contracts(file);
        }
    }

    /// Reaches a part of a file and says whether it is new.
    fn unit(&mut self, file: FileId, part: Part) -> bool {
        let unit = Unit { file, part };
        if self.walk.reached(unit) {
            return false;
        }
        self.layer.units.insert(unit);
        true
    }

    /// Records a way into `file` from `from`.
    fn list(&mut self, file: FileId, from: FileId, cause: Cause) {
        self.layer.candidates.push(Candidate { file, from, cause });
    }

    /// The declarations that hold `line` most narrowly, or the load of the file when none does. Says whether any part is new.
    fn place(&mut self, file: FileId, symbols: FileSymbols<'a>, line: Line) -> bool {
        let owners = narrowest(symbols, line);
        if owners.is_empty() {
            return self.unit(file, Part::Load);
        }
        owners.into_iter().fold(false, |found, index| {
            self.unit(file, Part::Decl(index)) | found
        })
    }

    /// The declarations of `file` that refer to the `declared` ones reached, and its load when it does. A member `Class.member` is also referred to by the declarations outside `Class` that refer to the class, because a call through a value names no class.
    fn callers_in(&mut self, file: FileId, symbols: FileSymbols<'a>, declared: &[usize]) {
        let mut names: Vec<&'a str> = declared
            .iter()
            .filter_map(|&index| symbols.declaration(index))
            .map(Declaration::name)
            .collect();
        names.sort_unstable();
        names.dedup();
        let mut classes: Vec<&'a str> = names.iter().filter_map(|name| class_of(name)).collect();
        classes.sort_unstable();
        classes.dedup();
        for member in &names {
            if let Some(class) = class_of(member) {
                self.layer
                    .through_class
                    .insert(((*member).to_owned(), class.to_owned()));
            }
        }
        let mut found = false;
        for (index, decl) in symbols.declarations().enumerate() {
            let refers = decl.uses().any(|used| holds(&names, &used))
                || classes.iter().any(|class| {
                    decl.uses().any(|used| used == *class) && !belongs_to(decl.name(), class)
                });
            if refers {
                found |= self.unit(file, Part::Decl(index));
            }
        }
        if symbols
            .on_load()
            .any(|used| holds(&names, &used) || holds(&classes, &used))
        {
            found |= self.unit(file, Part::Load);
        }
        if !found {
            return;
        }
        let asked = self.walk.asked(file);
        if !asked.is_empty() {
            self.list(file, file, Cause::Declares { names: asked });
        }
    }

    /// The names `file` exports the `declared` ones as, which its importers take. The class of a member is the name it is exported as, because the member is not. A file whose exports are unknown could export them as any name.
    fn exports(&mut self, file: FileId, symbols: FileSymbols<'a>, declared: &[usize]) {
        let mut exported = BTreeSet::new();
        for decl in declared
            .iter()
            .filter_map(|&index| symbols.declaration(index))
        {
            let name = decl.name();
            for local in [Some(name), class_of(name)].into_iter().flatten() {
                exported.extend(self.exported_as(file, symbols, local));
            }
        }
        if exported.contains(&Name::Any) {
            exported = BTreeSet::from([Name::Any]);
        }
        self.flow(file, exported);
    }

    /// The names under which `file` exports `local`: itself when the file lists it among its exports, and each name it exports from it. A file whose exports are unknown could export it as any name.
    fn exported_as(&self, file: FileId, symbols: FileSymbols<'a>, local: &'a str) -> Vec<Name<'a>> {
        let graph = self.walk.graph;
        let Some(listed) = graph.exports(file) else {
            return vec![Name::Any];
        };
        let itself = graph
            .name_id(local)
            .is_some_and(|id| listed.binary_search(&id).is_ok())
            .then_some(local);
        itself
            .into_iter()
            .chain(
                symbols
                    .exports()
                    .filter(|&(_, from)| from == local)
                    .map(|(name, _)| name),
            )
            .map(Name::Is)
            .collect()
    }

    /// Follows the `names` of `file` to the files that import them. A barrel that passes a name on, and a file that imports a name and exports it again, are listed and pass the name to their own importers within the same hop.
    ///
    /// The walk over files decides the same things in `follow`, `take_all`, `reaches` and `exported_by` of `crates/core/src/query/walk.rs`, and in `known`, `exported` and `visit_stars` of `crates/core/src/query/walk/exports.rs`. This is a second copy because it follows one name at a time, through the local an import binds it to so as to find the declarations that use it, which the edges of the graph do not record, and because a star hands a name on in the same hop, so it never works out X(file). The properties `a_walk_by_name_lists_no_file_the_walk_per_file_does_not` and `a_walk_by_every_name_lists_what_the_walk_per_file_does_when_every_import_is_used` in `crates/core/tests/queries/names.rs` hold the two against each other, on the trees where the design lets them agree.
    fn flow(&mut self, file: FileId, names: BTreeSet<Name<'a>>) {
        let graph = self.walk.graph;
        let mut seen = BTreeSet::new();
        let mut work: VecDeque<(FileId, Name<'a>)> =
            names.into_iter().map(|name| (file, name)).collect();
        while let Some((from, name)) = work.pop_front() {
            if !seen.insert((from, name)) {
                continue;
            }
            for &(importer, edge) in graph.dependents(from) {
                if !edge.is_structural() {
                    self.importer(from, name, importer, &mut work);
                }
            }
        }
    }

    /// What `importer` makes of `name`, which `from` exports.
    fn importer(
        &mut self,
        from: FileId,
        name: Name<'a>,
        importer: FileId,
        work: &mut VecDeque<(FileId, Name<'a>)>,
    ) {
        let walk = self.walk;
        let symbols = walk.graph.symbols(importer);
        let imports = walk
            .index
            .records
            .get(importer.index())
            .map_or(&[][..], |record| record.imports.as_slice());
        let mut via = Via {
            taken: Taken::Named(Vec::new()),
            locals: Vec::new(),
            passes: Vec::new(),
            used: false,
            barrel: false,
            whole: false,
        };
        for resolved in imports
            .iter()
            .filter(|resolved| resolved.to.files().contains(&from))
        {
            match symbols {
                Some(symbols) => {
                    self.by_name(name, importer, symbols, &resolved.import, &mut via, work);
                }
                None => whole_if_taken(walk.graph, name, &resolved.import, &mut via),
            }
        }
        if let Some(symbols) = symbols {
            via.used |= self.users(importer, symbols, &via.locals);
        }
        let passes = std::mem::take(&mut via.passes);
        let (used, barrel, whole) = (via.used, via.barrel, via.whole);
        let names = via.taken.into_names();
        if whole && self.unit(importer, Part::Whole) {
            let cause = Cause::Imports {
                file: from,
                names: names.clone(),
            };
            self.list(importer, from, cause);
        }
        if used {
            self.list(importer, from, Cause::Imports { file: from, names });
        }
        if barrel {
            let mut passes = passes;
            passes.sort_unstable();
            passes.dedup();
            self.list(
                importer,
                from,
                Cause::ReExports {
                    file: from,
                    names: passes,
                },
            );
        }
    }

    /// One import of `from` by a file that records its declarations.
    fn by_name(
        &mut self,
        name: Name<'a>,
        importer: FileId,
        symbols: FileSymbols<'a>,
        import: &'a Import,
        via: &mut Via<'a>,
        work: &mut VecDeque<(FileId, Name<'a>)>,
    ) {
        let graph = self.walk.graph;
        match import.kind {
            ImportKind::Static | ImportKind::ReExport => {
                // An import type binds no local: `import('./a').foo` uses `foo` where it is written, in the declaration whose lines hold it or in the load, as a dynamic import does.
                if import.kind == ImportKind::Static
                    && !import.names.is_empty()
                    && import.exported.is_empty()
                {
                    let mut takes = false;
                    for taken in import.names.iter().filter(|taken| name.takes(taken)) {
                        via.taken.name(graph, taken);
                        takes = true;
                    }
                    via.used |= takes && self.place(importer, symbols, import.line);
                    return;
                }
                // An import that names what it takes binds each name; one that does not could use anything.
                if import.names.is_empty() || import.exported.len() != import.names.len() {
                    via.whole = true;
                    via.taken = Taken::Everything;
                    return;
                }
                let re_export = import.kind == ImportKind::ReExport;
                for (taken, bound) in import.names.iter().zip(&import.exported) {
                    if !name.takes(taken) {
                        continue;
                    }
                    via.taken.name(graph, taken);
                    if re_export {
                        via.barrel = true;
                        if let (Some(source), Some(target)) =
                            (graph.name_id(taken), graph.name_id(bound))
                        {
                            via.passes.push((source, target));
                        }
                        work.push_back((importer, Name::Is(bound)));
                        continue;
                    }
                    via.locals.push(bound);
                    for exported in self.exported_as(importer, symbols, bound) {
                        via.used = true;
                        work.push_back((importer, exported));
                    }
                }
            }
            ImportKind::ReExportAll => {
                // `export *` never forwards `default`.
                if name != Name::Is("default") {
                    via.barrel = true;
                    if let Name::Is(exported) = name
                        && let Some(id) = graph.name_id(exported)
                    {
                        via.passes.push((id, id));
                    }
                    work.push_back((importer, name));
                }
            }
            // It uses nothing by name, but a file whose every name is reached loads the importer.
            ImportKind::SideEffect => {
                if name == Name::Any {
                    via.used |= self.unit(importer, Part::Load);
                }
            }
            // A use of every name, by the declaration whose lines hold the import, or by the load.
            ImportKind::Dynamic | ImportKind::Require => {
                via.taken = Taken::Everything;
                via.used |= self.place(importer, symbols, import.line);
            }
            ImportKind::Reference | ImportKind::Module | ImportKind::Include => {
                via.taken = Taken::Everything;
                via.whole = true;
            }
        }
    }

    /// The declarations of `importer` that refer to one of `locals`, and its load when it does. Says whether any is new.
    fn users(&mut self, importer: FileId, symbols: FileSymbols<'a>, locals: &[&str]) -> bool {
        if locals.is_empty() {
            return false;
        }
        let mut found = false;
        for (index, decl) in symbols.declarations().enumerate() {
            if decl.uses().any(|used| locals.contains(&used)) {
                found |= self.unit(importer, Part::Decl(index));
            }
        }
        if symbols.on_load().any(|used| locals.contains(&used)) {
            found |= self.unit(importer, Part::Load);
        }
        found
    }

    /// Every file that loads `file`, reached at load: a static import that is not type-only, a re-export, a side-effect import, a dynamic import and `require`.
    fn loaders(&mut self, file: FileId) {
        let graph = self.walk.graph;
        for &(importer, edge) in graph.dependents(file) {
            if !loads(edge) {
                continue;
            }
            let part = if graph.symbols(importer).is_some() {
                Part::Load
            } else {
                Part::Whole
            };
            if self.unit(importer, part) {
                let cause = Cause::Imports {
                    file,
                    names: Vec::new(),
                };
                self.list(importer, file, cause);
            }
        }
    }

    /// The contracts of the `declared` ones, crossed from the sites that belong to them and no others.
    fn declared_contracts(&mut self, file: FileId, symbols: FileSymbols<'a>, declared: &[usize]) {
        let graph = self.walk.graph;
        for &contract in graph.contracts_of(file) {
            let mut roles: Vec<Role> = sites_of(graph.sites(contract), file)
                .filter(|site| {
                    narrowest(symbols, site.line)
                        .iter()
                        .any(|owner| declared.contains(owner))
                })
                .map(|site| site.role)
                .collect();
            roles.sort_unstable();
            roles.dedup();
            if roles.is_empty() {
                continue;
            }
            let crossed = self.walk.crossing.cross_as(self.crossed, &roles, contract);
            self.arrive(file, &crossed);
        }
    }

    /// Every contract of `file`, as a file affected in all its names crosses them.
    fn all_contracts(&mut self, file: FileId) {
        for &contract in self.walk.graph.contracts_of(file) {
            let crossed = self.walk.crossing.cross(self.crossed, file, contract);
            self.arrive(file, &crossed);
        }
    }

    /// The part of each file across a contract that its site belongs to, or the whole file when it records no declarations.
    fn arrive(&mut self, from: FileId, sites: &[(ContractId, &'a ContractSite)]) {
        for &(contract, site) in sites {
            let file = site.file;
            let found = match self.walk.graph.symbols(file) {
                Some(symbols) => self.place(file, symbols, site.line),
                None => self.unit(file, Part::Whole),
            };
            if found {
                let cause = Cause::Contract {
                    role: site.role,
                    contract,
                };
                self.list(file, from, cause);
            }
        }
    }
}

/// An import of a file that records no declarations takes the file whole when it takes the name, and by its edge alone when it names nothing.
fn whole_if_taken(graph: &Graph, name: Name<'_>, import: &Import, via: &mut Via<'_>) {
    match import.kind {
        ImportKind::ReExportAll => {
            if name != Name::Is("default") {
                via.taken = Taken::Everything;
                via.whole = true;
            }
        }
        ImportKind::Static | ImportKind::ReExport if !import.names.is_empty() => {
            for taken in import.names.iter().filter(|taken| name.takes(taken)) {
                via.taken.name(graph, taken);
                via.whole = true;
            }
        }
        _ => {
            via.taken = Taken::Everything;
            via.whole = true;
        }
    }
}

/// Whether the importer of `edge` runs the code of the file it imports as it loads.
fn loads(edge: Edge) -> bool {
    !edge.type_only()
        && [
            ImportKind::Static,
            ImportKind::ReExport,
            ImportKind::ReExportAll,
            ImportKind::SideEffect,
            ImportKind::Dynamic,
            ImportKind::Require,
        ]
        .into_iter()
        .any(|kind| edge.has(kind))
}

/// The declarations that hold `line` most narrowly, as indices. A member holds a line more narrowly than its class, also when both span the same lines. None when no declaration holds it.
fn narrowest(symbols: FileSymbols<'_>, line: Line) -> Vec<usize> {
    let mut best: Vec<(usize, &str)> = Vec::new();
    let mut width = Line::MAX;
    for (index, decl) in symbols.declarations().enumerate() {
        if line < decl.line() || decl.end() < line {
            continue;
        }
        let span = decl.end().saturating_sub(decl.line());
        match span.cmp(&width) {
            Ordering::Less => {
                width = span;
                best.clear();
                best.push((index, decl.name()));
            }
            Ordering::Equal => best.push((index, decl.name())),
            Ordering::Greater => {}
        }
    }
    best.iter()
        .filter(|&&(_, name)| !best.iter().any(|&(_, other)| member_of(other, name)))
        .map(|&(index, _)| index)
        .collect()
}

/// `Class` for the member `Class.member`.
fn class_of(name: &str) -> Option<&str> {
    name.split_once(MEMBER_SEPARATOR).map(|(class, _)| class)
}

/// Whether `name` is a member of `class`.
fn member_of(name: &str, class: &str) -> bool {
    name.strip_prefix(class)
        .is_some_and(|rest| rest.starts_with(MEMBER_SEPARATOR))
}

/// Whether `name` is `class` or one of its members.
fn belongs_to(name: &str, class: &str) -> bool {
    name == class || member_of(name, class)
}

#[cfg(test)]
mod tests {
    use crate::index::Index;
    use crate::testing::{Code, Tree};

    use super::*;

    /// `lib.ts` with a class over lines 1 to 20 and two methods, one of which shares its line with a function.
    fn index() -> Index {
        Tree::new()
            .symbols(
                "lib.ts",
                Code::new()
                    .declaration("Runner", (1, 20), &[])
                    .declaration("Runner.start", (3, 6), &[])
                    .declaration("Runner.stop", (8, 12), &[])
                    .declaration("one", (25, 25), &[])
                    .declaration("two", (25, 25), &[]),
            )
            .build()
    }

    /// The names of the declarations that hold `line` most narrowly.
    fn holding(index: &Index, line: Line) -> Vec<String> {
        let file = index.graph.file_id("lib.ts").unwrap();
        let symbols = index.graph.symbols(file).unwrap();
        narrowest(symbols, line)
            .into_iter()
            .map(|at| symbols.declaration(at).unwrap().name().to_owned())
            .collect()
    }

    #[test]
    fn a_member_holds_a_line_more_narrowly_than_its_class() {
        let index = index();
        assert_eq!(holding(&index, 4), ["Runner.start"]);
        assert_eq!(holding(&index, 9), ["Runner.stop"]);
    }

    #[test]
    fn a_line_of_the_class_outside_its_members_belongs_to_the_class() {
        assert_eq!(holding(&index(), 7), ["Runner"]);
    }

    #[test]
    fn declarations_over_the_same_line_hold_it_together() {
        assert_eq!(holding(&index(), 25), ["one", "two"]);
    }

    #[test]
    fn a_line_outside_every_declaration_belongs_to_none() {
        assert_eq!(holding(&index(), 22), Vec::<String>::new());
    }

    #[test]
    fn a_member_over_the_lines_of_its_class_still_comes_first() {
        let index = Tree::new()
            .symbols(
                "lib.ts",
                Code::new()
                    .declaration("Box", (1, 1), &[])
                    .declaration("Box.open", (1, 1), &[]),
            )
            .build();
        assert_eq!(holding(&index, 1), ["Box.open"]);
    }

    #[test]
    fn the_class_of_a_member_is_the_name_before_the_first_dot() {
        assert_eq!(class_of("Runner.start"), Some("Runner"));
        assert_eq!(class_of("Runner.#secret"), Some("Runner"));
        assert_eq!(class_of("run"), None);
    }

    #[test]
    fn a_member_is_named_after_its_class_and_a_dot() {
        assert!(member_of("Runner.start", "Runner"));
        assert!(!member_of("Runner", "Runner"));
        assert!(!member_of("RunnerPool.start", "Runner"));
        assert!(belongs_to("Runner", "Runner"));
        assert!(belongs_to("Runner.start", "Runner"));
        assert!(!belongs_to("Runners", "Runner"));
    }

    #[test]
    fn a_name_is_found_in_a_sorted_list() {
        let sorted = ["a", "b", "c"];
        assert!(holds(&sorted, &"b"));
        assert!(!holds(&sorted, &"d"));
        assert!(!holds(&[], &"a"));
    }
}
