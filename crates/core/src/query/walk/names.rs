//! The walk behind `impact` asked about names: the declarations that refer to what a change reaches, hop by hop, in place of the files that import it.
//!
//! A hop goes from the declarations the walk has reached in a file to the declarations that refer to them, in the file itself and in the files that import them under the names the file exports them as. A barrel on the way, and a file that imports a name and exports it again, are listed and add no hop. Code that runs as a file loads is a caller of what it refers to, and a file reached at load reaches the files that load it. A file whose connector records no declarations is followed whole, as the walk over files follows every file.
//!
//! The depth bounds the hops through code: the declarations that refer to a name, the loads and the mocks. A hop across a contract counts in the distance and not in the depth, so every part the walk reaches crosses its contracts, and the part across carries the hops through code of the part it came from.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::contract::Role;
use crate::fact::{Import, ImportFlags, ImportKind, Line, MEMBER_SEPARATOR};
use crate::graph::{ContractSite, Declaration, Edge, FileSymbols, Graph};
use crate::ids::{ContractId, FileId, Id, NameId};
use crate::index::Index;
use crate::query::cross::{Crossed, Crossing, Direction, Leavers, keep_fewest, roles_of, sites_of};

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

/// Every file that holds a declaration, or the load, that refers to the `names` of the declarations of `changed` within `depth` hops through code, with the callers in each. The files of `changed` are listed only when their own code refers to the names.
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
        depth,
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
    while !frontier.is_empty() {
        let layer = Step::new(&walk, &mut crossed).expand(&frontier);
        distance += 1;
        frontier = walk.apply(layer, distance);
    }
    // The parts at the depth took no hop through code. One more hop from them, which nothing records, tells whether the depth left callers behind.
    if !Step::new(&walk, &mut crossed)
        .beyond(&walk.at_depth())
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

/// How far a part of a file is from the declarations asked about, by the way the walk first found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Hops {
    /// Every hop of the way, a contract crossing included, which is the distance the answer gives.
    distance: u32,
    /// The hops of the way through code, which the depth bounds.
    code: u32,
}

/// The hops of a declaration asked about, which the walk starts from.
const START: Hops = Hops {
    distance: 0,
    code: 0,
};

/// A part of a file the walk has just reached.
#[derive(Clone, Copy, Debug)]
struct Front {
    unit: Unit,
    /// The hops through code of the way it was reached by.
    code: u32,
}

/// A name a file exports that the walk has reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Name<'a> {
    Is(&'a str),
    /// A member of the class exported as the first name, which is what the walk reached of it. The class is the name that imports take; the member is what an importer's declarations use as `Local.member`, where a use of `Local` alone is a use of every member.
    Member(&'a str, &'a str),
    /// Every name of the file: the file is followed whole, or exports its declarations under names that cannot be listed.
    Any,
}

impl<'a> Name<'a> {
    /// The name the file exports, `None` for every name.
    fn exported(self) -> Option<&'a str> {
        match self {
            Self::Is(name) | Self::Member(name, _) => Some(name),
            Self::Any => None,
        }
    }

    /// Whether an import that takes `taken` from the file gets this name. A namespace takes every name, and a member gets the import of its class.
    fn takes(self, taken: &str) -> bool {
        self.exported()
            .is_none_or(|name| taken == "*" || taken == name)
    }

    /// The member an import that takes `taken` is about. A namespace holds the class under a name of its own, so a use of the namespace is not named after the member.
    fn member_taken(self, taken: &str) -> Option<&'a str> {
        match self {
            Self::Member(_, member) if taken != "*" => Some(member),
            _ => None,
        }
    }

    /// This name for `member` of the class it names. `Any` stays whatever the member.
    fn with(self, member: Option<&'a str>) -> Self {
        match (self, member) {
            (Self::Is(class), Some(member)) => Self::Member(class, member),
            _ => self,
        }
    }
}

/// What the walk knows of a file it has reached.
#[derive(Debug, Default)]
struct Touch {
    /// The hops to the first way found into the file, and that way. A file the walk started from has none until its own code refers to the names.
    listed: Option<(u32, Cause)>,
    /// The declarations reached, as indices among those of the file, each with its hops. Those asked about are at distance 0.
    decls: BTreeMap<usize, Hops>,
    /// The hops to the code that runs as the file loads, once the walk reaches it.
    load: Option<Hops>,
    /// The hops to the file as a whole, once the walk reaches it.
    whole: Option<Hops>,
}

impl Touch {
    /// The parts reached with their hops, in the order of [`Part`].
    fn parts(&self) -> impl Iterator<Item = (Part, Hops)> + '_ {
        self.decls
            .iter()
            .map(|(&index, &hops)| (Part::Decl(index), hops))
            .chain(self.load.map(|hops| (Part::Load, hops)))
            .chain(self.whole.map(|hops| (Part::Whole, hops)))
    }
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
    /// Each part with the fewest hops through code of the ways the layer found it by.
    units: BTreeMap<Unit, u32>,
    candidates: Vec<Candidate>,
    /// Each member with its class.
    through_class: BTreeSet<(String, String)>,
}

struct Walk<'a> {
    index: &'a Index,
    graph: &'a Graph,
    crossing: &'a Crossing<'a>,
    /// The hops through code a part may be reached by before the walk takes no more.
    depth: u32,
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
    ) -> Vec<Front> {
        let mut started = BTreeSet::new();
        for &file in changed {
            let touch = self.files.entry(file).or_default();
            let Some(symbols) = self.graph.symbols(file) else {
                notes.insert(Note::NoDeclarations { file });
                touch.whole = Some(START);
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
                        touch.decls.insert(index, START);
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
        started
            .into_iter()
            .map(|unit| Front { unit, code: 0 })
            .collect()
    }

    fn reached(&self, unit: Unit) -> bool {
        self.files
            .get(&unit.file)
            .is_some_and(|touch| match unit.part {
                Part::Decl(index) => touch.decls.contains_key(&index),
                Part::Load => touch.load.is_some(),
                Part::Whole => touch.whole.is_some(),
            })
    }

    /// The parts reached by `depth` hops through code, which take no more of them.
    fn at_depth(&self) -> Vec<Front> {
        self.files
            .iter()
            .flat_map(|(&file, touch)| {
                touch
                    .parts()
                    .filter(|&(_, hops)| hops.code == self.depth)
                    .map(move |(part, hops)| Front {
                        unit: Unit { file, part },
                        code: hops.code,
                    })
            })
            .collect()
    }

    /// The names of the declarations of `file` the walk started from, in byte order.
    fn asked(&self, file: FileId) -> Vec<String> {
        let (Some(touch), Some(symbols)) = (self.files.get(&file), self.graph.symbols(file)) else {
            return Vec::new();
        };
        let mut names: Vec<String> = touch
            .decls
            .iter()
            .filter(|&(_, hops)| hops.distance == 0)
            .filter_map(|(&index, _)| symbols.declaration(index))
            .map(|decl| decl.name().to_owned())
            .collect();
        names.sort_unstable();
        names
    }

    /// Records what the layer found at `distance`, and returns the parts of files it reached, which are the next layer's frontier.
    fn apply(&mut self, layer: Layer, distance: u32) -> Vec<Front> {
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
        for (unit, &code) in &layer.units {
            let touch = self.files.entry(unit.file).or_default();
            let hops = Hops { distance, code };
            match unit.part {
                Part::Decl(index) => {
                    touch.decls.entry(index).or_insert(hops);
                }
                Part::Load => {
                    touch.load.get_or_insert(hops);
                }
                Part::Whole => {
                    touch.whole.get_or_insert(hops);
                }
            }
        }
        let mut frontier: Vec<Front> = layer
            .units
            .into_iter()
            .map(|(unit, code)| Front { unit, code })
            .collect();
        frontier.sort_unstable_by_key(|front| (front.unit.file, front.code, front.unit.part));
        frontier
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
                    .filter(|&(_, hops)| hops.distance > 0)
                    .map(|(&index, hops)| (index, hops.distance))
                    .collect();
                callers.sort_by(|&(left, _), &(right, _)| name(left).cmp(&name(right)));
                Some(Hit {
                    file,
                    distance,
                    cause,
                    callers,
                    load: touch.load.map(|hops| hops.distance),
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
    locals: Vec<Local<'a>>,
    /// What it passes on as a named re-export, as the name it takes and the name it exports.
    passes: Vec<(NameId, NameId)>,
    /// Whether it uses a name beyond its locals: it exports one again, takes names by a dynamic import, a `require` or an import type, or calls a file followed whole where it mocks it.
    used: bool,
    /// Whether it re-exports a name.
    barrel: bool,
    /// Whether it must be followed whole.
    whole: bool,
}

/// A name an importer binds, with the member of it that the walk reached when it reached one.
#[derive(Clone, Copy)]
struct Local<'a> {
    name: &'a str,
    member: Option<&'a str>,
}

impl Local<'_> {
    /// Whether a declaration that uses `used` refers to what the local stands for. A use of the local alone is a use of every member of it, and `Local.member` is a use of that member only.
    fn refers(&self, used: &str) -> bool {
        match self.member {
            None => belongs_to(used, self.name),
            Some(member) => used == self.name || member_in(used, self.name) == Some(member),
        }
    }
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

/// The parts of one file at the frontier.
struct Parts<'a> {
    /// The declarations, as indices among those of the file.
    declared: Vec<usize>,
    /// The symbols of the file, when a declaration of it is among the parts.
    symbols: Option<FileSymbols<'a>>,
    load: bool,
    whole: bool,
}

/// A contract the parts of one file are crossed from.
struct Leave {
    contract: ContractId,
    /// The roles of the sites they leave it by, in role order, each with the fewest hops through code of the parts that leave by it.
    roles: Vec<(Role, u32)>,
}

/// The fronts of each file of a `frontier`, which is sorted by file.
fn by_file(frontier: &[Front]) -> impl Iterator<Item = &[Front]> {
    frontier.chunk_by(|left, right| left.unit.file == right.unit.file)
}

/// One layer of the walk: the parts of files it was given, and what they reach.
struct Step<'w, 'a> {
    walk: &'w Walk<'a>,
    crossed: &'w mut Crossed,
    layer: Layer,
    /// The fewest hops through code of the parts of the frontier that leave each contract, when they differ.
    leavers: Leavers,
    /// The hops through code of the parts the step reaches now: one more than the part they come from after a hop through code, as many after a hop across a contract.
    hops: u32,
}

impl<'w, 'a> Step<'w, 'a> {
    fn new(walk: &'w Walk<'a>, crossed: &'w mut Crossed) -> Self {
        Self {
            walk,
            crossed,
            layer: Layer::default(),
            leavers: Leavers::new(Direction::ToCallers),
            hops: 0,
        }
    }

    /// What the `frontier`, which is sorted by file, reaches one hop further.
    fn expand(mut self, frontier: &[Front]) -> Layer {
        // Every file's contracts are worked out, and noted, before the first is crossed: the parts across a contract are as far in hops through code as the fewest of those that leave it, whichever comes first.
        let files: Vec<(&[Front], Vec<Leave>)> = by_file(frontier)
            .map(|fronts| (fronts, self.leaving(fronts)))
            .collect();
        self.note_leavers(&files);
        for (fronts, leaving) in &files {
            self.file(fronts, leaving);
        }
        self.layer
    }

    /// Notes how many hops through code the parts of a frontier that leave each contract took, when the frontier holds parts that took different numbers.
    fn note_leavers(&mut self, files: &[(&[Front], Vec<Leave>)]) {
        let codes = files
            .iter()
            .flat_map(|(fronts, _)| fronts.iter().map(|front| front.code));
        if !self.leavers.start(codes) {
            return;
        }
        for leave in files.iter().flat_map(|(_, leaving)| leaving) {
            for &(role, code) in &leave.roles {
                self.leavers.add(leave.contract, role, code);
            }
        }
    }

    /// What the parts of the `frontier`, which are sorted as for [`Step::expand`], reach by one hop through code that the depth does not let the walk take, and across no contract.
    fn beyond(mut self, frontier: &[Front]) -> Layer {
        for fronts in by_file(frontier) {
            if let Some(first) = fronts.first() {
                let parts = self.parts(first.unit.file, fronts);
                self.through_code(first.unit.file, &parts);
            }
        }
        self.layer
    }

    /// The parts of `file` among the `fronts`.
    fn parts(&self, file: FileId, fronts: &[Front]) -> Parts<'a> {
        let mut parts = Parts {
            declared: Vec::new(),
            symbols: None,
            load: false,
            whole: false,
        };
        for front in fronts {
            match front.unit.part {
                Part::Decl(index) => parts.declared.push(index),
                Part::Load => parts.load = true,
                Part::Whole => parts.whole = true,
            }
        }
        parts.symbols = self
            .walk
            .graph
            .symbols(file)
            .filter(|_| !parts.declared.is_empty());
        parts
    }

    /// The parts of one file that sit at the frontier, which are sorted by hops through code. Imports come before contracts, as in the walk over files, and the contracts go in key order whatever the hops through code of the parts that leave them.
    fn file(&mut self, fronts: &[Front], leaving: &[Leave]) {
        let Some(first) = fronts.first() else {
            return;
        };
        let file = first.unit.file;
        for group in fronts.chunk_by(|left, right| left.code == right.code) {
            // The depth stops the hops through code and not the crossing of a contract.
            if let Some(code) = group.first().map(|front| front.code)
                && code < self.walk.depth
            {
                self.hops = code + 1;
                let parts = self.parts(file, group);
                self.through_code(file, &parts);
            }
        }
        for leave in leaving {
            let roles = leave.roles.iter().map(|&(role, _)| role);
            let crossed = self
                .walk
                .crossing
                .cross_as(self.crossed, roles, leave.contract);
            self.arrive(file, leave.contract, first.code, &crossed);
        }
    }

    /// The hop through code from the `parts` of one file: the declarations that refer to them, the files that import them and the files that load it.
    fn through_code(&mut self, file: FileId, parts: &Parts<'a>) {
        if let Some(symbols) = parts.symbols {
            self.callers_in(file, symbols, &parts.declared);
            self.exports(file, symbols, &parts.declared);
        }
        if parts.load {
            self.loaders(file);
        }
        if parts.whole {
            self.flow(file, BTreeSet::from([Name::Any]));
        }
    }

    /// Reaches a part of a file and says whether it is new. A part found again in the layer by a way with fewer hops through code keeps the fewer.
    fn unit(&mut self, file: FileId, part: Part) -> bool {
        let unit = Unit { file, part };
        if self.walk.reached(unit) {
            return false;
        }
        keep_fewest(&mut self.layer.units, unit, self.hops);
        true
    }

    /// Records a way into `file` from `from`.
    fn list(&mut self, file: FileId, from: FileId, cause: Cause) {
        self.layer.candidates.push(Candidate { file, from, cause });
    }

    /// The declarations that hold `line` most narrowly. Says whether any part is new, and `None` when no declaration holds the line.
    fn holders(&mut self, file: FileId, symbols: FileSymbols<'a>, line: Line) -> Option<bool> {
        let mut owners = narrowest(symbols, line).peekable();
        owners.peek()?;
        Some(owners.fold(false, |found, index| {
            self.unit(file, Part::Decl(index)) | found
        }))
    }

    /// The declarations that hold `line` most narrowly, or the load of the file when none does. Says whether any part is new.
    fn place(&mut self, file: FileId, symbols: FileSymbols<'a>, line: Line) -> bool {
        self.holders(file, symbols, line)
            .unwrap_or_else(|| self.unit(file, Part::Load))
    }

    /// The names of `import` that `name` gets, used where the import is written. A type does not run as the file loads, so an import type outside every declaration reaches nothing, where any other import reaches the load of the file.
    fn use_taken(
        &mut self,
        name: Name<'a>,
        importer: FileId,
        symbols: FileSymbols<'a>,
        import: &Import,
        via: &mut Via<'a>,
    ) {
        if !take_names(self.walk.graph, name, import, via) {
            return;
        }
        via.used |= if import.flags.contains(ImportFlags::TYPE_ONLY) {
            self.holders(importer, symbols, import.line)
                .unwrap_or(false)
        } else {
            self.place(importer, symbols, import.line)
        };
    }

    /// The declarations of `file` that refer to the `declared` ones reached, and its load when it does. A member `Class.member` is also referred to by the declarations outside `Class` that refer to the class, because a call through a value names no class. A class is referred to by the declarations outside it that use one of its members, `Class.other` as well as `Class.member`.
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
            if decl
                .uses()
                .any(|used| refers(&names, &classes, Some(decl.name()), used))
            {
                found |= self.unit(file, Part::Decl(index));
            }
        }
        if symbols
            .on_load()
            .any(|used| refers(&names, &classes, None, used))
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
            let (local, member) = match split_member(decl.name()) {
                Some((class, member)) => (class, Some(member)),
                None => (decl.name(), None),
            };
            let names = self.exported_as(file, symbols, local);
            exported.extend(names.into_iter().map(|name| name.with(member)));
        }
        if exported.contains(&Name::Any) {
            exported = BTreeSet::from([Name::Any]);
        } else {
            // A class reached as a whole is reached in every member.
            let whole: Vec<&str> = exported
                .iter()
                .filter_map(|name| match name {
                    Name::Is(class) => Some(*class),
                    _ => None,
                })
                .collect();
            exported
                .retain(|name| !matches!(name, Name::Member(class, _) if whole.contains(class)));
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
                // An import type binds no local: `import('./a').foo` uses `foo` where it is written, in the declarations whose lines hold it, and outside every declaration it reaches nothing, because a type does not run as the file loads.
                if import.kind == ImportKind::Static
                    && !import.names.is_empty()
                    && import.exported.is_empty()
                {
                    self.use_taken(name, importer, symbols, import, via);
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
                    let member = name.member_taken(taken);
                    if re_export {
                        via.barrel = true;
                        if let (Some(source), Some(target)) =
                            (graph.name_id(taken), graph.name_id(bound))
                        {
                            via.passes.push((source, target));
                        }
                        work.push_back((importer, Name::Is(bound).with(member)));
                        continue;
                    }
                    via.locals.push(Local {
                        name: bound,
                        member,
                    });
                    for exported in self.exported_as(importer, symbols, bound) {
                        via.used = true;
                        work.push_back((importer, exported.with(member)));
                    }
                }
            }
            ImportKind::ReExportAll => {
                // `export *` never forwards `default`.
                if name.exported() != Some("default") {
                    via.barrel = true;
                    if let Some(exported) = name.exported()
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
            // A test that mocks a module runs what replaces it, not the code of the module, so a mock takes no name. A file followed whole is called where the mock is written, as an import for side effects calls it.
            ImportKind::Mock => {
                if name == Name::Any {
                    via.used |= self.place(importer, symbols, import.line);
                }
            }
            // A use of every name, by the declaration whose lines hold the import, or by the load, when it takes the module itself. One that names what it takes uses those names where it is written.
            ImportKind::Dynamic | ImportKind::Require => {
                if takes_module(import) {
                    via.taken = Taken::Everything;
                    via.used |= self.place(importer, symbols, import.line);
                } else {
                    self.use_taken(name, importer, symbols, import, via);
                }
            }
            ImportKind::Reference | ImportKind::Module | ImportKind::Include => {
                via.taken = Taken::Everything;
                via.whole = true;
            }
        }
    }

    /// The declarations of `importer` that refer to one of `locals`, and its load when it does. Says whether any is new.
    fn users(&mut self, importer: FileId, symbols: FileSymbols<'a>, locals: &[Local]) -> bool {
        if locals.is_empty() {
            return false;
        }
        let refers = |used: &str| locals.iter().any(|local| local.refers(used));
        let mut found = false;
        for (index, decl) in symbols.declarations().enumerate() {
            if decl.uses().any(refers) {
                found |= self.unit(importer, Part::Decl(index));
            }
        }
        if symbols.on_load().any(refers) {
            found |= self.unit(importer, Part::Load);
        }
        found
    }

    /// Every file that loads `file`, reached at load: a static import that is not type-only, a re-export, a side-effect import, a dynamic import and `require`, and every mock, because the index does not record whether a mock has a factory and one made without a factory loads the module to mock it.
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

    /// The contracts the `fronts` of one file are crossed from, each with the roles of the sites they leave it by, in role order, and the fewest hops through code of the parts that leave it by each: those of the declarations among them, which are the sites that belong to them and no others, and then every contract of the file when its load or the whole of it is among them, as a file affected in all its names crosses them.
    fn leaving(&self, fronts: &[Front]) -> Vec<Leave> {
        let graph = self.walk.graph;
        let Some(file) = fronts.first().map(|front| front.unit.file) else {
            return Vec::new();
        };
        let mut leaving = Vec::new();
        let declared = |index: usize| {
            fronts
                .iter()
                .filter(|front| front.unit.part == Part::Decl(index))
                .map(|front| front.code)
                .min()
        };
        let symbols = graph.symbols(file).filter(|_| {
            fronts
                .iter()
                .any(|front| matches!(front.unit.part, Part::Decl(_)))
        });
        if let Some(symbols) = symbols {
            for &contract in graph.contracts_of(file) {
                let mut roles: Vec<(Role, u32)> = sites_of(graph.sites(contract), file)
                    .filter_map(|site| {
                        narrowest(symbols, site.line)
                            .filter_map(declared)
                            .min()
                            .map(|code| (site.role, code))
                    })
                    .collect();
                // The first of a role has the fewest hops.
                roles.sort_unstable();
                roles.dedup_by_key(|&mut (role, _)| role);
                if !roles.is_empty() {
                    leaving.push(Leave { contract, roles });
                }
            }
        }
        let loaded = fronts
            .iter()
            .filter(|front| matches!(front.unit.part, Part::Load | Part::Whole))
            .map(|front| front.code)
            .min();
        if let Some(code) = loaded {
            leaving.extend(graph.contracts_of(file).iter().map(|&contract| {
                Leave {
                    contract,
                    roles: roles_of(graph.sites(contract), file)
                        .into_iter()
                        .map(|role| (role, code))
                        .collect(),
                }
            }));
        }
        leaving
    }

    /// The part of each file across `contract` that its site belongs to, or the whole file when it records no declarations. A part across it has the hops through code of the parts of the frontier that leave `contract` and reach it, `code` when they were not told apart.
    fn arrive(
        &mut self,
        from: FileId,
        contract: ContractId,
        code: u32,
        sites: &[(ContractId, &'a ContractSite)],
    ) {
        for &(meeting, site) in sites {
            let file = site.file;
            self.hops = self.leavers.arrival(contract, site.role, code);
            let found = match self.walk.graph.symbols(file) {
                Some(symbols) => self.place(file, symbols, site.line),
                None => self.unit(file, Part::Whole),
            };
            if found {
                let cause = Cause::Contract {
                    role: site.role,
                    contract: meeting,
                };
                self.list(file, from, cause);
            }
        }
    }
}

/// Notes in `via` each name of `import` that `name` gets. Says whether there is one.
fn take_names(graph: &Graph, name: Name<'_>, import: &Import, via: &mut Via<'_>) -> bool {
    let mut takes = false;
    for taken in import.names.iter().filter(|taken| name.takes(taken)) {
        via.taken.name(graph, taken);
        takes = true;
    }
    takes
}

/// Whether a dynamic import or a `require` takes the module itself, so every name of it: it names nothing, or names the namespace, which is `*` and for a `require` also the `default` that `const lib = require('./a')` binds.
fn takes_module(import: &Import) -> bool {
    import.names.is_empty()
        || import
            .names
            .iter()
            .any(|name| name == "*" || (import.kind == ImportKind::Require && name == "default"))
}

/// An import of a file that records no declarations takes the file whole when it takes the name, and by its edge alone when it names nothing. A mock takes it whole only when the file is followed whole.
fn whole_if_taken(graph: &Graph, name: Name<'_>, import: &Import, via: &mut Via<'_>) {
    match import.kind {
        ImportKind::ReExportAll => {
            if name.exported() != Some("default") {
                via.taken = Taken::Everything;
                via.whole = true;
            }
        }
        ImportKind::Static | ImportKind::ReExport if !import.names.is_empty() => {
            via.whole |= take_names(graph, name, import, via);
        }
        ImportKind::Dynamic | ImportKind::Require if !takes_module(import) => {
            via.whole |= take_names(graph, name, import, via);
        }
        ImportKind::Mock => {
            if name == Name::Any {
                via.taken = Taken::Everything;
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
            ImportKind::Mock,
        ]
        .into_iter()
        .any(|kind| edge.has(kind))
}

/// The declarations that hold `line` most narrowly, as indices in the order of the declarations. A member holds a line more narrowly than its class, also when both span the same lines. Nothing when no declaration holds it. Nothing is collected, because a contract site asks this of every site of every contract of a file, so the declarations are visited again instead.
fn narrowest<'g>(symbols: FileSymbols<'g>, line: Line) -> impl Iterator<Item = usize> + 'g {
    let holds_line = move |decl: Declaration<'g>| decl.line() <= line && line <= decl.end();
    let span = |decl: Declaration<'g>| decl.end().saturating_sub(decl.line());
    let width = symbols
        .declarations()
        .filter(|&decl| holds_line(decl))
        .map(span)
        .min();
    let is_narrowest = move |decl: Declaration<'g>| holds_line(decl) && Some(span(decl)) == width;
    symbols
        .declarations()
        .enumerate()
        .filter(move |&(_, decl)| {
            is_narrowest(decl)
                && !symbols
                    .declarations()
                    .any(|other| is_narrowest(other) && member_of(other.name(), decl.name()))
        })
        .map(|(index, _)| index)
}

/// Whether code that uses `used` refers to one of the `names` of the declarations reached, or to a member of them. `classes` are the classes of the reached names that are members. A use of such a class alone is a use of every member of it, and a use of `Class.member` is a use of that member and of `Class`. The code of `owner` belongs to a class it is a member of, which it does not refer to by being in it. The load of a file has no owner.
fn refers(names: &[&str], classes: &[&str], owner: Option<&str>, used: &str) -> bool {
    let outside = |class: &str| owner.is_none_or(|owner| !belongs_to(owner, class));
    holds(names, &used)
        || (holds(classes, &used) && outside(used))
        || class_of(used).is_some_and(|class| holds(names, &class) && outside(class))
}

/// `Class` and `member` for the member `Class.member`.
fn split_member(name: &str) -> Option<(&str, &str)> {
    name.split_once(MEMBER_SEPARATOR)
}

/// `Class` for the member `Class.member`.
fn class_of(name: &str) -> Option<&str> {
    split_member(name).map(|(class, _)| class)
}

/// `member` for the name `class.member`, when `name` is one.
fn member_in<'n>(name: &'n str, class: &str) -> Option<&'n str> {
    name.strip_prefix(class)?.strip_prefix(MEMBER_SEPARATOR)
}

/// Whether `name` is a member of `class`.
fn member_of(name: &str, class: &str) -> bool {
    member_in(name, class).is_some()
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
