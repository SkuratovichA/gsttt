//! The walk behind `impact` asked about names: the declarations that refer to what a change reaches, hop by hop, in place of the files that import it.
//!
//! A hop goes from the declarations the walk has reached in a file to the declarations that refer to them, in the file itself and in the files that import them under the names the file exports them as. A barrel on the way that names what it passes on, and a file that imports a name and exports it again, are listed and add no hop; an `export *` names nothing and gets no row. Code that runs as a file loads is a caller of what it refers to, and a file reached at load reaches the files that load it. A file whose connector records no declarations is followed whole, as the walk over files follows every file.
//!
//! The depth bounds the hops through code: the declarations that refer to a name, the loads and the mocks. A hop across a contract counts in the distance and not in the depth, so every part the walk reaches crosses its contracts, and the part across carries the hops through code of the part it came from.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::contract::Role;
use crate::fact::{
    Import, ImportFlags, ImportKind, Line, MEMBER_SEPARATOR, belongs_to, member_in, member_of,
};
use crate::graph::{ContractSite, Declaration, Edge, FileSymbols, Graph};
use crate::ids::{ContractId, FileId, Id, NameId};
use crate::index::Index;
use crate::path::RepoPath;
use crate::query::cross::{Crossed, Crossing, Direction, Leavers, keep_fewest, sites_of};

use super::overrides::{overridden, overriding};
use super::{Cause, Reached, holds};

/// What the answer says about the walk beside the files it reached.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::query) enum Note {
    /// `file` declares no `name`, so nothing was reached from it by that name.
    Undeclared { file: FileId, name: String },
    /// The connector of `file` records no declarations, so the walk follows the file whole.
    NoDeclarations { file: FileId },
    /// A call through a value names no class, so a file that holds the class of these members and names none of them cannot be told from a caller, and is left out: `files` of them. Each member comes with its class, in byte order of the member.
    HeldClass {
        members: Vec<(String, String)>,
        files: usize,
    },
    /// A file loads the module and reads none of the names asked about, so it cannot be told from a caller of the module, and is left out: `files` of them. It holds the module in a local, or loads it below the top level.
    HeldModule { files: usize },
    /// The member `member` of a class may be run through the same member of a head of the class, `through`, which `file` declares, so its callers are listed.
    Overridden {
        member: String,
        through: String,
        file: FileId,
    },
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
    /// The files the walk met across a contract, sorted and once each, whether or not it listed them. A test that replaces a module still runs the code of one of these when it reaches it without the module.
    pub(in crate::query) across: Vec<FileId>,
}

/// Every file that holds a declaration, or the load, that refers to the `names` of the declarations of `changed` within `depth` hops through code, or at any number of them when there is no depth, with the callers in each. The files of `changed` are listed only when their own code refers to the names.
pub(in crate::query) fn reach_names(
    index: &Index,
    crossing: &Crossing<'_>,
    changed: &[FileId],
    names: &[String],
    depth: Option<u32>,
) -> Walked {
    walk_from(index, crossing, depth, Vec::new(), |walk, notes| {
        let mut asked = changed.to_vec();
        asked.sort_unstable();
        asked.dedup();
        walk.start(&asked, names, notes)
    })
}

/// What a changed file is walked from.
enum Seed<'p> {
    /// The file as a whole: nothing says which of its declarations changed.
    Whole,
    /// The declarations that changed with the declarations that depend on them in the file, whether the code that runs as the file loads does, and the names the file no longer declares or exports, which its importers still say.
    Parts {
        decls: BTreeSet<usize>,
        load: bool,
        removed: &'p [String],
    },
}

/// What changed in one file: the declarations that differ in it, and the names it no longer declares or exports.
pub(in crate::query) struct Changed<'p> {
    pub(in crate::query) file: FileId,
    pub(in crate::query) names: &'p [String],
    pub(in crate::query) removed: &'p [String],
}

/// Like [`reach_names`] for the files of `changed`, each walked from the declarations `parts` names for it, and from the importers of the names it removed. A file with no entry in `parts`, a file without declarations and a file with a name it does not declare are followed whole, and a file whose entry names nothing is not followed, because nothing in it changed. The declarations of the file that use a declaration walked from are walked from too, since they run its new code. When no file is walked by declaration, the walk is the one over files.
pub(in crate::query) fn reach_changed(
    index: &Index,
    crossing: &Crossing<'_>,
    changed: &[FileId],
    parts: &[Changed<'_>],
    depth: Option<u32>,
) -> Walked {
    let mut asked = changed.to_vec();
    asked.sort_unstable();
    asked.dedup();
    let mut seeds: BTreeMap<FileId, Seed<'_>> = BTreeMap::new();
    for file in asked {
        let part = parts.iter().find(|part| part.file == file);
        if let Some(seed) = seed_of(&index.graph, file, part) {
            seeds.insert(file, seed);
        }
    }
    if seeds.values().all(|seed| matches!(seed, Seed::Whole)) {
        let whole: Vec<FileId> = seeds.keys().copied().collect();
        return super::reach(&index.graph, crossing, &whole, depth);
    }
    let removed = seeds
        .iter()
        .flat_map(|(&file, seed)| {
            match seed {
                Seed::Parts { removed, .. } => *removed,
                Seed::Whole => &[],
            }
            .iter()
            .map(move |name| (file, name.as_str()))
        })
        .collect();
    walk_from(index, crossing, depth, removed, |walk, notes| {
        walk.start_seeds(&seeds, notes)
    })
}

/// Where the walk starts in `file`, `None` when nothing in it changed. `part` says which declarations changed and which names are gone, `None` when the request does not say.
fn seed_of<'p>(graph: &Graph, file: FileId, part: Option<&Changed<'p>>) -> Option<Seed<'p>> {
    let Some(&Changed { names, removed, .. }) = part else {
        return Some(Seed::Whole);
    };
    if names.is_empty() && removed.is_empty() {
        return None;
    }
    let Some(symbols) = graph.symbols(file) else {
        return Some(Seed::Whole);
    };
    let mut decls = BTreeSet::new();
    for name in names {
        let before = decls.len();
        decls.extend(
            symbols
                .declarations()
                .enumerate()
                .filter(|(_, decl)| decl.name() == name)
                .map(|(index, _)| index),
        );
        if decls.len() == before {
            return Some(Seed::Whole);
        }
    }
    let (decls, load) = close_over(symbols, decls);
    Some(Seed::Parts {
        decls,
        load,
        removed,
    })
}

/// The declarations of a file that run the new code of the `seeded` ones, with the seeded ones: those that use them, and those that use those, whether the file loads by calling them or not. Says whether the code that runs as the file loads is among them.
fn close_over(symbols: FileSymbols<'_>, mut seeded: BTreeSet<usize>) -> (BTreeSet<usize>, bool) {
    let mut pending: Vec<usize> = (0..symbols.len())
        .filter(|index| !seeded.contains(index))
        .collect();
    let mut fresh: Vec<usize> = seeded.iter().copied().collect();
    let mut load = false;
    while !fresh.is_empty() {
        let (names, classes) = referred(symbols, &fresh);
        // Inside one file a subclass and a value of the class are reached through the use of the class itself, so the closure needs no heirs.
        let reach = Reach {
            symbols,
            names: &names,
            classes: &classes,
            heirs: &[],
        };
        load = load || reach.loads() == Refers::Yes;
        fresh.clear();
        pending.retain(|&index| {
            let refers_to_fresh = reach.uses(index) == Refers::Yes;
            if refers_to_fresh {
                fresh.push(index);
            }
            !refers_to_fresh
        });
        seeded.extend(fresh.iter().copied());
    }
    (seeded, load)
}

/// The names of the declarations at `indices`, and the classes of those that are members, each sorted and once.
fn referred<'g>(symbols: FileSymbols<'g>, indices: &[usize]) -> (Vec<&'g str>, Vec<&'g str>) {
    let mut names: Vec<&str> = indices
        .iter()
        .filter_map(|&index| symbols.declaration(index))
        .map(Declaration::name)
        .collect();
    names.sort_unstable();
    names.dedup();
    let mut classes: Vec<&str> = names.iter().filter_map(|name| class_of(name)).collect();
    classes.sort_unstable();
    classes.dedup();
    (names, classes)
}

/// The walk from the parts `start` reaches first, run layer by layer until it reaches nothing new. The first layer also holds the importers of the `removed` names of files, sorted by file, which are no part of any file the walk can start from.
fn walk_from<'r>(
    index: &'r Index,
    crossing: &Crossing<'_>,
    depth: Option<u32>,
    removed: Vec<(FileId, &'r str)>,
    start: impl FnOnce(&mut Walk<'_>, &mut BTreeSet<Note>) -> Vec<Front>,
) -> Walked {
    let mut walk = Walk {
        index,
        graph: &index.graph,
        crossing,
        depth,
        files: BTreeMap::new(),
        across: BTreeSet::new(),
        through_class: BTreeSet::new(),
        held: BTreeSet::new(),
        held_modules: BTreeSet::new(),
        heads: BTreeSet::new(),
    };
    let mut crossed = Crossed::new(Direction::ToCallers, &index.graph);
    let mut notes = BTreeSet::new();
    let mut frontier = start(&mut walk, &mut notes);
    let mut distance = 0;
    let mut removed = removed;
    while !frontier.is_empty() || !removed.is_empty() {
        let layer = Step::new(&walk, &mut crossed).expand(&frontier, &removed);
        removed.clear();
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
    // A file left out may be listed for another reason, and then it is no file the note is about. One note for every member, so that the answer can cut the list of them as it cuts every list of names.
    let outside = |file: &FileId| !hits.iter().any(|hit| hit.file == *file);
    // A call through a value names a class, or a module is held where no class is: a top-level name has no class, and the files that hold its module are the ones that load it and read none of its names.
    let (classed, mut modules) = if walk.through_class.is_empty() {
        (BTreeSet::new(), walk.held.clone())
    } else {
        (walk.held.clone(), BTreeSet::new())
    };
    modules.extend(walk.held_modules.iter().copied());
    let held = classed.iter().filter(|file| outside(file)).count();
    if held > 0 {
        notes.insert(Note::HeldClass {
            members: walk.through_class.into_iter().collect(),
            files: held,
        });
    }
    let modules = modules.iter().filter(|file| outside(file)).count();
    if modules > 0 {
        notes.insert(Note::HeldModule { files: modules });
    }
    Walked {
        hits,
        notes: notes.into_iter().collect(),
        across: walk.across.into_iter().collect(),
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

    /// Whether an import that takes `taken` from the file gets this name. A namespace takes every name, and a member gets the import of its class. What the importer then uses of it is `member_taken`: a namespace is a caller only of the names it uses.
    fn takes(self, taken: &str) -> bool {
        self.exported()
            .is_none_or(|name| taken == "*" || taken == name)
    }

    /// The part of what an import that takes `taken` binds that the importer must use to be a caller. A namespace holds the name under a name of its own, `ns.name`, so the importer uses that name, and for a member the class, because through a namespace a class is read as a whole. `Any` keeps every use.
    fn member_taken(self, taken: &str) -> Option<&'a str> {
        match self {
            Self::Is(name) | Self::Member(name, _) if taken == "*" => Some(name),
            Self::Member(_, member) => Some(member),
            Self::Is(_) | Self::Any => None,
        }
    }

    /// The name a file reads on a module it holds in a local to use this name of it: the name, or the member for a member of a class, and none for every name and for the default, which the local itself stands for.
    fn read_on_a_module(self) -> Option<&'a str> {
        match self {
            Self::Is("default") | Self::Any => None,
            Self::Is(name) => Some(name),
            Self::Member(_, member) => Some(member),
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
    /// The files the layer met across a contract.
    across: Vec<FileId>,
    /// Each member with its class.
    through_class: BTreeSet<(String, String)>,
    /// The files that hold a class or a value of it where the walk asked about a member of it, and name none.
    held: BTreeSet<FileId>,
    /// The files that load the module and read none of the names asked about.
    held_modules: BTreeSet<FileId>,
}

struct Walk<'a> {
    index: &'a Index,
    graph: &'a Graph,
    crossing: &'a Crossing<'a>,
    /// The hops through code a part may be reached by before the walk takes no more, none when there is no limit.
    depth: Option<u32>,
    files: BTreeMap<FileId, Touch>,
    /// The files the walk met across a contract.
    across: BTreeSet<FileId>,
    /// The members of classes the walk looked for the callers of through their class, each with its class.
    through_class: BTreeSet<(String, String)>,
    /// The files that hold a class or a value of it where the walk asked about a member of it, and name none.
    held: BTreeSet<FileId>,
    /// The files that load the module and read none of the names asked about.
    held_modules: BTreeSet<FileId>,
    /// The declarations the walk started from only because the member asked about overrides them. A sibling that implements them is no caller, so the walk lists no overrides of them.
    heads: BTreeSet<Unit>,
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
            for name in names {
                self.start_overridden(file, name, &mut started, notes);
            }
        }
        started
            .into_iter()
            .map(|unit| Front { unit, code: 0 })
            .collect()
    }

    /// Marks what each seed names as reached at distance 0 and returns it as the first frontier. A file followed whole is reached whole.
    fn start_seeds(
        &mut self,
        seeds: &BTreeMap<FileId, Seed<'_>>,
        notes: &mut BTreeSet<Note>,
    ) -> Vec<Front> {
        let mut started = BTreeSet::new();
        for (&file, seed) in seeds {
            let touch = self.files.entry(file).or_default();
            match seed {
                Seed::Whole => {
                    touch.whole = Some(START);
                    started.insert(Unit {
                        file,
                        part: Part::Whole,
                    });
                }
                Seed::Parts { decls, load, .. } => {
                    for &index in decls {
                        touch.decls.insert(index, START);
                        started.insert(Unit {
                            file,
                            part: Part::Decl(index),
                        });
                    }
                    if *load {
                        touch.load = Some(START);
                        started.insert(Unit {
                            file,
                            part: Part::Load,
                        });
                    }
                    let graph = self.graph;
                    for name in decls
                        .iter()
                        .filter_map(|&index| graph.symbols(file)?.declaration(index))
                        .map(Declaration::name)
                    {
                        self.start_overridden(file, name, &mut started, notes);
                    }
                }
            }
        }
        started
            .into_iter()
            .map(|unit| Front { unit, code: 0 })
            .collect()
    }

    /// Marks the same member of each head of the class of `name`, a member `Class.member` declared in `file`, as reached at distance 0: a value of the head may run the member, so those that hold the head may call it.
    fn start_overridden(
        &mut self,
        file: FileId,
        name: &str,
        started: &mut BTreeSet<Unit>,
        notes: &mut BTreeSet<Note>,
    ) {
        for (owner, place) in overridden(self.index, file, name) {
            let Some(decl) = self
                .graph
                .symbols(owner)
                .and_then(|symbols| symbols.declaration(place))
            else {
                continue;
            };
            self.files
                .entry(owner)
                .or_default()
                .decls
                .insert(place, START);
            let unit = Unit {
                file: owner,
                part: Part::Decl(place),
            };
            if started.insert(unit) {
                self.heads.insert(unit);
            }
            notes.insert(Note::Overridden {
                member: name.to_owned(),
                through: decl.name().to_owned(),
                file: owner,
            });
        }
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

    /// The parts reached by `depth` hops through code, which take no more of them. A walk with no depth has none.
    fn at_depth(&self) -> Vec<Front> {
        self.files
            .iter()
            .flat_map(|(&file, touch)| {
                touch
                    .parts()
                    .filter(|&(_, hops)| self.depth == Some(hops.code))
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
        self.held.extend(layer.held);
        self.held_modules.extend(layer.held_modules);
        self.across.extend(layer.across);
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
        Cause::Overrides { .. } => 1,
        Cause::Imports { .. } => 2,
        Cause::ReExports { .. } => 3,
        Cause::Contract { .. } => 4,
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
        (
            Cause::Overrides { file, names },
            Cause::Overrides {
                file: same,
                names: more,
            },
        ) if file == same => union(names, more),
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
    /// Whether it uses a name beyond its locals: it exports one again, takes names by a dynamic import, a `require` or an import type, or calls a file followed whole where it mocks it without replacing it.
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
    /// The member the walk asked about when the local is a namespace, whose `member` is the class and is all the namespace is told of it: `place` for `lib` with the member `Base`, when the walk is at `Base.place`.
    inner: Option<&'a str>,
}

impl<'a> Local<'a> {
    /// The member that a name taking the members of what the local stands for takes: none for a local the walk reached no member of.
    fn passes(&self) -> Option<&'a str> {
        self.member.and(self.inner.or(self.member))
    }

    /// Whether a name that takes the members of `of` takes them from what the local stands for. Through a namespace `of` is written `lib.Base`.
    fn extends(&self, of: &str) -> bool {
        match self.inner {
            Some(_) => member_in(of, self.name) == self.member,
            None => of == self.name,
        }
    }

    /// Whether a declaration that uses `used` refers to what the local stands for. A use of the local alone is a use of the member only where the file reads the member on some value, and `Local.member` is a use of that member only, which for a namespace is a name of the file and for a class a member of it.
    fn refers(&self, symbols: FileSymbols<'_>, used: &str) -> Refers {
        let Some(member) = self.member else {
            return Refers::from(belongs_to(used, self.name));
        };
        if used == self.name {
            // A namespace or a class passed away names nothing by itself: it is a use of the member where the file names the member, which for a namespace is the member of its class that the walk asked about.
            return if symbols.reads_member(self.inner.unwrap_or(member)) {
                Refers::Yes
            } else {
                Refers::Held
            };
        }
        let Some(rest) = member_in(used, self.name) else {
            return Refers::No;
        };
        match self.inner {
            // `lib.Base` alone is a use of `Base` as a whole, and `lib.Base.place` of the member.
            Some(inner) if rest == member => {
                if symbols.reads_member(inner) {
                    Refers::Yes
                } else {
                    Refers::Held
                }
            }
            Some(inner) => {
                Refers::from(member_in(rest, member).is_some_and(|name| belongs_to(name, inner)))
            }
            None => Refers::from(belongs_to(rest, member)),
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

/// The roles of some sites, each with the fewest hops through code of the sites in it, in role order.
fn by_role(sites: impl Iterator<Item = (Role, u32)>) -> Vec<(Role, u32)> {
    let mut roles: Vec<(Role, u32)> = sites.collect();
    // The first of a role has the fewest hops.
    roles.sort_unstable();
    roles.dedup_by_key(|&mut (role, _)| role);
    roles
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
    leavers: Leavers<'a>,
    /// The hops through code of the parts the step reaches now: one more than the part they come from after a hop through code, as many after a hop across a contract.
    hops: u32,
}

impl<'w, 'a> Step<'w, 'a> {
    fn new(walk: &'w Walk<'a>, crossed: &'w mut Crossed) -> Self {
        Self {
            walk,
            crossed,
            layer: Layer::default(),
            leavers: Leavers::new(Direction::ToCallers, walk.crossing),
            hops: 0,
        }
    }

    /// What the `frontier`, which is sorted by file, reaches one hop further, and what the importers of the `removed` names reach.
    fn expand(mut self, frontier: &[Front], removed: &[(FileId, &'a str)]) -> Layer {
        // Every file's contracts are worked out, and noted, before the first is crossed: the parts across a contract are as far in hops through code as the fewest of those that leave it, whichever comes first.
        let files: Vec<(&[Front], Vec<Leave>)> = by_file(frontier)
            .map(|fronts| (fronts, self.leaving(fronts)))
            .collect();
        self.note_leavers(&files);
        for (fronts, leaving) in &files {
            self.file(fronts, leaving);
        }
        self.gone(removed);
        self.layer
    }

    /// The importers of the names that files no longer declare or export, one hop through code from the file. A removed member is the member of the class the file exports now, or the class itself when that is gone too.
    fn gone(&mut self, removed: &[(FileId, &'a str)]) {
        if !super::within(self.walk.depth, 0) {
            return;
        }
        self.hops = 1;
        for group in removed.chunk_by(|left, right| left.0 == right.0) {
            let Some(&(file, _)) = group.first() else {
                continue;
            };
            let symbols = self.walk.graph.symbols(file);
            let mut exported = BTreeSet::new();
            for &(_, name) in group {
                match (symbols, split_member(name)) {
                    (Some(symbols), Some(_)) => {
                        self.exported_decl(file, symbols, name, &mut exported);
                    }
                    _ => {
                        exported.insert(Name::Is(name));
                    }
                }
            }
            self.flow_exported(file, exported);
        }
    }

    /// Notes how many hops through code the parts of a frontier that leave each contract took, when the frontier holds parts that took different numbers.
    fn note_leavers(&mut self, files: &[(&[Front], Vec<Leave>)]) {
        let codes = files
            .iter()
            .flat_map(|(fronts, _)| fronts.iter().map(|front| front.code));
        if !self.leavers.start(codes) {
            return;
        }
        for (fronts, leaving) in files {
            let Some(file) = fronts.first().map(|front| front.unit.file) else {
                continue;
            };
            for leave in leaving {
                for &(role, code) in &leave.roles {
                    self.leavers.add(file, leave.contract, role, code);
                }
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
                && super::within(self.walk.depth, code)
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
                .cross_as(self.crossed, file, roles, leave.contract);
            self.arrive(file, leave.contract, first.code, &crossed);
        }
    }

    /// The hop through code from the `parts` of one file: the declarations that refer to them, the files that import them and the files that load it.
    fn through_code(&mut self, file: FileId, parts: &Parts<'a>) {
        if let Some(symbols) = parts.symbols {
            self.callers_in(file, symbols, &parts.declared);
            self.exports(file, symbols, &parts.declared);
            self.overriders(file, symbols, &parts.declared);
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

    /// The names of `import` that `name` gets, used where the import is written. A type does not run as the file loads, so an import type outside every declaration reaches nothing, where any other import reaches the load of the file. A type that names a class reads no member of it: asked about a member, the file is a holder of the class, and its declarations are callers only where the file reads that member.
    fn use_taken(
        &mut self,
        name: Name<'a>,
        importer: FileId,
        symbols: FileSymbols<'a>,
        import: &Import,
        via: &mut Via<'a>,
    ) {
        if import.flags.contains(ImportFlags::TYPE_ONLY)
            && let Name::Member(class, member) = name
            && import.names.iter().any(|taken| taken == class)
            && !symbols.reads_member(member)
        {
            self.layer.held.insert(importer);
            return;
        }
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

    /// The declarations of `file` that refer to the `declared` ones reached, and its load when it does. A member `Class.member` is also referred to by the declarations outside `Class` that refer to the class in a file that reads `member` on some value, because a call through a value names no class. A class is referred to by the declarations outside it that use one of its members, `Class.other` as well as `Class.member`.
    fn callers_in(&mut self, file: FileId, symbols: FileSymbols<'a>, declared: &[usize]) {
        let (names, classes) = referred(symbols, declared);
        for member in &names {
            if let Some(class) = class_of(member) {
                self.layer
                    .through_class
                    .insert(((*member).to_owned(), class.to_owned()));
            }
        }
        let mut heirs: Vec<(&'a str, &'a str)> = Vec::new();
        for name in &names {
            if let Some((class, member)) = split_member(name) {
                for heir in heirs_of(symbols, |of| of == class, member) {
                    heirs.push((heir, member));
                }
            }
        }
        let reach = Reach {
            symbols,
            names: &names,
            classes: &classes,
            heirs: &heirs,
        };
        let mut found = false;
        let mut held = false;
        for index in 0..symbols.len() {
            match reach.uses(index) {
                Refers::Yes => found |= self.unit(file, Part::Decl(index)),
                Refers::Held => held = true,
                Refers::No => {}
            }
        }
        match reach.loads() {
            Refers::Yes => found |= self.unit(file, Part::Load),
            Refers::Held => held = true,
            Refers::No => {}
        }
        // The file that declares the member is not a holder to ask about.
        if held && self.walk.asked(file).is_empty() {
            self.layer.held.insert(file);
        }
        if !found {
            return;
        }
        let asked = self.walk.asked(file);
        if !asked.is_empty() {
            self.list(file, file, Cause::Declares { names: asked });
        }
    }

    /// The files that declare what the `declared` members override or are overridden by. A head the walk started from because of the member asked about is listed for the declaration the member overrides; any other member lists the declarations that override it, which are reached as its callers are. The siblings of a head stay out.
    fn overriders(&mut self, file: FileId, symbols: FileSymbols<'a>, declared: &[usize]) {
        for &index in declared {
            let Some(decl) = symbols.declaration(index) else {
                continue;
            };
            let unit = Unit {
                file,
                part: Part::Decl(index),
            };
            if self.walk.heads.contains(&unit) {
                self.list(
                    file,
                    file,
                    Cause::Declares {
                        names: vec![decl.name().to_owned()],
                    },
                );
                continue;
            }
            for (owner, place) in overriding(self.walk.index, file, index) {
                if self.unit(owner, Part::Decl(place)) {
                    self.list(
                        owner,
                        file,
                        Cause::Overrides {
                            file,
                            names: vec![decl.name().to_owned()],
                        },
                    );
                }
            }
        }
    }

    /// The names `file` exports the `declared` ones as, which its importers take. The class of a member is the name it is exported as, because the member is not. A file whose exports are unknown could export them as any name.
    fn exports(&mut self, file: FileId, symbols: FileSymbols<'a>, declared: &[usize]) {
        let mut exported = BTreeSet::new();
        for decl in declared
            .iter()
            .filter_map(|&index| symbols.declaration(index))
        {
            self.exported_decl(file, symbols, decl.name(), &mut exported);
        }
        self.flow_exported(file, exported);
    }

    /// Adds to `exported` the names `file` exports the declaration `name` as, with its member when it is one, and the members of the names that take the members of its class.
    fn exported_decl(
        &self,
        file: FileId,
        symbols: FileSymbols<'a>,
        name: &'a str,
        exported: &mut BTreeSet<Name<'a>>,
    ) {
        let (local, member) = match split_member(name) {
            Some((class, member)) => (class, Some(member)),
            None => (name, None),
        };
        let names = self.exported_as(file, symbols, local);
        exported.extend(names.into_iter().map(|name| name.with(member)));
        // A name that takes the members of the class takes this one too.
        if let Some(member) = member {
            for heir in heirs_of(symbols, |of| of == local, member) {
                let names = self.exported_as(file, symbols, heir);
                exported.extend(names.into_iter().map(|name| name.with(Some(member))));
            }
        }
    }

    /// Follows what `file` exports to its importers.
    fn flow_exported(&mut self, file: FileId, mut exported: BTreeSet<Name<'a>>) {
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

    /// Whether `importer` re-exports `name` by name from a file, as `export { name } from './b'` does, which beats a name an `export *` of the same file brings in.
    fn re_exports_by_name(&self, importer: FileId, name: &str) -> bool {
        self.walk
            .index
            .records
            .get(importer.index())
            .is_some_and(|record| {
                record.imports.iter().any(|resolved| {
                    resolved.import.kind == ImportKind::ReExport
                        && resolved
                            .import
                            .exported
                            .iter()
                            .any(|exported| exported == name)
                })
            })
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

    /// Follows the `names` of `file` to the files that import them. A barrel that names what it passes on, and a file that imports a name and exports it again, are listed and pass the name to their own importers within the same hop.
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
                    self.by_name(
                        name,
                        from,
                        importer,
                        symbols,
                        &resolved.import,
                        &mut via,
                        work,
                    );
                }
                None => whole_if_taken(walk.graph, name, &resolved.import, &mut via),
            }
        }
        if let Some(symbols) = symbols {
            self.pass_on(importer, symbols, &mut via, work);
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

    /// The names of `importer` that take the members of a local with a member, which the importer uses as that local: each is a local too, and its importers get the member in the same hop. Whether the importer is listed is left to its own uses of the locals.
    fn pass_on(
        &mut self,
        importer: FileId,
        symbols: FileSymbols<'a>,
        via: &mut Via<'a>,
        work: &mut VecDeque<(FileId, Name<'a>)>,
    ) {
        let mut heirs = Vec::new();
        for local in &via.locals {
            if let Some(member) = local.passes() {
                for heir in heirs_of(symbols, |of| local.extends(of), member) {
                    heirs.push((heir, member));
                }
            }
        }
        for (heir, member) in heirs {
            via.locals.push(Local {
                name: heir,
                member: Some(member),
                inner: None,
            });
            for exported in self.exported_as(importer, symbols, heir) {
                work.push_back((importer, exported.with(Some(member))));
            }
        }
    }

    /// One import of `from` by a file that records its declarations.
    fn by_name(
        &mut self,
        name: Name<'a>,
        from: FileId,
        importer: FileId,
        symbols: FileSymbols<'a>,
        import: &'a Import,
        via: &mut Via<'a>,
        work: &mut VecDeque<(FileId, Name<'a>)>,
    ) {
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
                self.bind_names(name, from, importer, symbols, import, via, work);
            }
            ImportKind::ReExportAll => {
                let graph = self.walk.graph;
                // A declaration that uses its own name reads the one the star brings in before it binds it again, as `INSTALLED_APPS = INSTALLED_APPS + [..]` after `from .base import *` does: it uses the name of the file, so it is a user of it even where the star hands the name on no further.
                if let Name::Is(listed) = name
                    && symbols
                        .declarations()
                        .any(|decl| decl.name() == listed && decl.uses().any(|used| used == listed))
                {
                    via.taken.name(graph, listed);
                    via.locals.push(Local {
                        name: listed,
                        member: None,
                        inner: None,
                    });
                }
                // `export *` never forwards `default`, nor a name the importer binds itself: its own wins, so `from .base import *` followed by `DEBUG = False` hands on the new `DEBUG` and not the one of `base`, and `export { helper } from './b'` beside `export * from './a'` hands on the `helper` of `b`.
                let shadowed = name.exported().is_some_and(|listed| {
                    (symbols.declarations().any(|decl| decl.name() == listed)
                        && exports_name(graph, importer, listed))
                        || self.re_exports_by_name(importer, listed)
                });
                if name.exported() != Some("default") && !shadowed {
                    // Asked about a name, it names nothing, so it gets no row, unless it lists that name among the names it exports itself, as a package's `__all__` does; a file followed whole keeps it.
                    via.barrel |= name == Name::Any;
                    if let Name::Is(listed) = name
                        && let Some(id) = graph.name_id(listed)
                        && graph
                            .exports(importer)
                            .is_some_and(|own| own.binary_search(&id).is_ok())
                    {
                        via.barrel = true;
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
            // A test that mocks a module runs what replaces it, not the code of the module, so a mock takes no name. A file followed whole is called where a mock is written that does not replace it, as an import for side effects calls it; one that replaces it calls nothing of it.
            ImportKind::Mock => {
                if name == Name::Any && !replaces(import.flags) {
                    via.used |= self.place(importer, symbols, import.line);
                }
            }
            // A use of every name, by the declaration whose lines hold the import, or by the load, when it takes the module itself. One that names what it takes uses those names where it is written.
            ImportKind::Dynamic | ImportKind::Require => {
                if !import.names.is_empty() && import.exported.len() == import.names.len() {
                    self.bind_names(name, from, importer, symbols, import, via, work);
                } else if takes_module(import) {
                    // A local below the top level holds the module, and the file uses a name of it where it reads that name on a value.
                    if import.kind == ImportKind::Require
                        && !import.names.is_empty()
                        && name
                            .read_on_a_module()
                            .is_some_and(|read| !symbols.reads_member(read))
                    {
                        self.layer.held_modules.insert(importer);
                        return;
                    }
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

    /// What an import that binds each name it takes makes of `name`, which `from` exports: the locals its declarations refer to, and the names it exports again.
    fn bind_names(
        &mut self,
        name: Name<'a>,
        from: FileId,
        importer: FileId,
        symbols: FileSymbols<'a>,
        import: &'a Import,
        via: &mut Via<'a>,
        work: &mut VecDeque<(FileId, Name<'a>)>,
    ) {
        let graph = self.walk.graph;
        let re_export = import.kind == ImportKind::ReExport;
        for (taken, bound) in import.names.iter().zip(&import.exported) {
            // A name that a package hands out may be one of its modules, and the file reached is then that module itself, which the local stands for as a namespace does. A module that exports the name itself hands out that value when the import names the module, so `from app.logger import logger` binds the object and not the module of that stem, while `from . import cli` still binds the module `cli`, whatever it declares.
            let module = import.flags.contains(ImportFlags::MODULES)
                && graph
                    .path(from)
                    .is_some_and(|path| is_module_named(path, taken))
                && !(names_module(&import.spec, taken) && exports_name(graph, from, taken));
            // A require hands back the module, so a file that exports one value or names no default has that module taken for the one name the local binds, and its uses of the local are uses of the names.
            let widened = module
                || (taken == "default"
                    && name
                        .exported()
                        .is_some_and(|exported| exported != "default")
                    && (import.kind == ImportKind::Require
                        || !exports_name(graph, from, "default")));
            let taken = if widened { "*" } else { taken.as_str() };
            if !name.takes(taken) {
                continue;
            }
            via.taken.name(graph, taken);
            let member = name.member_taken(taken);
            if re_export {
                // A re-export of a class names none of its members, and `export * as ns` names no name of the file, so the walk goes through and the file gets no row.
                via.barrel |= member.is_none();
                if let (Some(source), Some(target)) = (graph.name_id(taken), graph.name_id(bound)) {
                    via.passes.push((source, target));
                }
                work.push_back((importer, Name::Is(bound).with(member)));
                continue;
            }
            let inner = match name {
                Name::Member(_, asked) if taken == "*" => Some(asked),
                _ => None,
            };
            via.locals.push(Local {
                name: bound,
                member,
                inner,
            });
            // A file that declares members of what it imports exports the name to start a walk at them, not to pass it on: it calls nothing of the import by that.
            if symbols
                .declarations()
                .any(|decl| member_of(decl.name(), bound))
            {
                continue;
            }
            for exported in self.exported_as(importer, symbols, bound) {
                // A file that hands a member's class on names no member of it, so it is no caller. Taking a module reads none of the names in it, so the file is listed for what its code uses of the module, and only the names its importers take of it are passed on.
                via.used |= member.is_none() && !module;
                work.push_back((importer, exported.with(member)));
            }
        }
    }

    /// The declarations of `importer` that refer to one of `locals`, and its load when it does. Says whether any is new.
    fn users(&mut self, importer: FileId, symbols: FileSymbols<'a>, locals: &[Local]) -> bool {
        if locals.is_empty() {
            return false;
        }
        let mut found = false;
        let mut held = false;
        for (index, decl) in symbols.declarations().enumerate() {
            match refers_decl(locals, symbols, Some(decl.name()), decl.uses()) {
                Refers::Yes => found |= self.unit(importer, Part::Decl(index)),
                Refers::Held => held = true,
                Refers::No => {}
            }
        }
        match refers_decl(locals, symbols, None, symbols.on_load()) {
            Refers::Yes => found |= self.unit(importer, Part::Load),
            Refers::Held => held = true,
            Refers::No => {}
        }
        if held {
            self.layer.held.insert(importer);
        }
        found
    }

    /// Every file that loads `file`, reached at load: a static import that is not type-only, a re-export, a side-effect import, a dynamic import and `require`, and every mock that does not replace the file, because one made without a factory loads the module to mock it.
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
                let roles = by_role(sites_of(graph.sites(contract), file).filter_map(|site| {
                    narrowest(symbols, site.line)
                        .filter_map(declared)
                        .min()
                        .map(|code| (site.role, code))
                }));
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
            leaving.extend(graph.contracts_of(file).iter().map(|&contract| Leave {
                contract,
                roles: by_role(sites_of(graph.sites(contract), file).map(|site| (site.role, code))),
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
            self.layer.across.push(file);
            self.hops = self.leavers.arrival(contract, site, code);
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

/// Whether `file` lists `name` among its exports.
fn exports_name(graph: &Graph, file: FileId, name: &str) -> bool {
    graph
        .name_id(name)
        .zip(graph.exports(file))
        .is_some_and(|(id, listed)| listed.binary_search(&id).is_ok())
}

/// Whether a dynamic import or a `require` takes the module itself, so every name of it: it names nothing, or names the namespace, which is `*` and for a `require` also the `default` that `const lib = require('./a')` binds.
fn takes_module(import: &Import) -> bool {
    import.names.is_empty()
        || import
            .names
            .iter()
            .any(|name| name == "*" || (import.kind == ImportKind::Require && name == "default"))
}

/// An import of a file that records no declarations takes the file whole when it takes the name, and by its edge alone when it names nothing. A mock takes it whole only when the file is followed whole and the mock does not replace it.
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
            if name == Name::Any && !replaces(import.flags) {
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

/// Whether the declaration `owner` takes the members of `used`: `class B extends A`, `const a = new A()`. Naming `A` there says where the members come from and reads none of them, and a read through it is written `B.member`. The load of a file has no owner, and runs the heritage of its classes, so there any heir of `used` counts.
fn takes_members(symbols: FileSymbols<'_>, owner: Option<&str>, used: &str) -> bool {
    symbols
        .heirs()
        .any(|(name, of)| of == used && owner.is_none_or(|owner| name == owner))
}

/// Whether `name` takes the members of another name in the file.
fn is_heir(symbols: FileSymbols<'_>, name: &str) -> bool {
    symbols.heirs().any(|(heir, _)| heir == name)
}

/// Whether the code of `owner` (the load when none) that makes the `uses` refers to one of `locals`. Asked about a member, code that takes the members of the class it names refers to it at most as a holder, and so does code that hands on such a class by its name, which names no member of it.
fn refers_decl<'u>(
    locals: &[Local<'_>],
    symbols: FileSymbols<'_>,
    owner: Option<&str>,
    uses: impl Iterator<Item = &'u str>,
) -> Refers {
    let mut best = Refers::No;
    for used in uses {
        for local in locals {
            let refers = local.refers(symbols, used);
            let capped = refers == Refers::Yes
                && local.member.is_some()
                && (takes_members(symbols, owner, used)
                    || (used == local.name && is_heir(symbols, used)));
            best = best.max(if capped { Refers::Held } else { refers });
        }
        if best == Refers::Yes {
            break;
        }
    }
    best
}

/// Whether `path` is the module a package hands out as `taken`: a file of that name, or the `__init__` of a directory of that name. Python is the connector that sets [`ImportFlags::MODULES`].
fn is_module_named(path: &RepoPath, taken: &str) -> bool {
    let name = path.file_name();
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    stem == taken
        || (stem == "__init__"
            && path
                .parent()
                .rsplit('/')
                .next()
                .is_some_and(|dir| dir == taken))
}

/// Whether the specifier of an import ends with the module `taken`, as `app.logger` and `.logger` do for `logger`, so that `taken` is looked up inside that module and not among the names of the package around it.
fn names_module(spec: &str, taken: &str) -> bool {
    spec.rsplit('.').next() == Some(taken)
}

/// Whether a mock with `flags` replaces its module with a factory, so the importer runs none of the code of that module.
const fn replaces(flags: ImportFlags) -> bool {
    flags.contains(ImportFlags::REPLACES)
}

/// Whether the importer of `edge` runs the code of the file it imports as it loads. A mock does, unless it replaces the file: the edge keeps a flag only when every import behind it carries it, so an edge that has `REPLACES` is made of mocks that replace the file and nothing else.
fn loads(edge: Edge) -> bool {
    !edge.type_only()
        && !replaces(edge.flags())
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

/// Whether code refers to a name the walk reached. A class or a value of it that is used alone is a use of its members, but a call through a value names no class, so the file must also name the member it calls: `Held` is a use that would count but for that.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Refers {
    No,
    Held,
    Yes,
}

impl From<bool> for Refers {
    fn from(refers: bool) -> Self {
        if refers { Self::Yes } else { Self::No }
    }
}

/// The names the walk reached in one file, which the code of that file may refer to.
struct Reach<'a, 'g> {
    symbols: FileSymbols<'g>,
    /// The names of the declarations reached.
    names: &'a [&'a str],
    /// The classes of the reached names that are members.
    classes: &'a [&'a str],
    /// The names that take a reached member from its class, each with the member.
    heirs: &'a [(&'a str, &'a str)],
}

impl Reach<'_, '_> {
    /// Whether the declaration at `index` refers to what was reached.
    fn uses(&self, index: usize) -> Refers {
        self.symbols
            .declaration(index)
            .map_or(Refers::No, |decl| self.best(decl.uses(), Some(decl.name())))
    }

    /// Whether the code that runs as the file loads refers to what was reached.
    fn loads(&self) -> Refers {
        self.best(self.symbols.on_load(), None)
    }

    fn best<'u>(&self, uses: impl Iterator<Item = &'u str>, owner: Option<&str>) -> Refers {
        let mut best = Refers::No;
        for used in uses {
            best = best.max(self.refers(owner, used));
            if best == Refers::Yes {
                break;
            }
        }
        best
    }

    /// Whether code that uses `used` refers to one of the `names`, or to a member of them. A use of `Class.member` is a use of that member and of `Class`, and a use of a class reached as a whole is a use of every member. A use of such a class alone, or of a value of it, is a use of the members reached only where the file names one of them. The code of `owner` belongs to a class it is a member of, which it does not refer to by being in it. The load of a file has no owner.
    fn refers(&self, owner: Option<&str>, used: &str) -> Refers {
        let outside = |class: &str| owner.is_none_or(|owner| !belongs_to(owner, class));
        if holds(self.names, &used)
            || class_of(used).is_some_and(|class| holds(self.names, &class) && outside(class))
            || self
                .heirs
                .iter()
                .any(|&(heir, member)| member_in(used, heir) == Some(member))
        {
            return Refers::Yes;
        }
        let names_member = |member: &str| self.symbols.reads_member(member);
        let class_held = holds(self.classes, &used)
            && outside(used)
            && self
                .names
                .iter()
                .any(|name| member_in(name, used).is_some_and(names_member));
        let heir_held = self
            .heirs
            .iter()
            .any(|&(heir, member)| used == heir && outside(heir) && names_member(member));
        if class_held || heir_held {
            // Naming a class only to take its members reads none of them.
            let takes = takes_members(self.symbols, owner, used);
            return if takes { Refers::Held } else { Refers::Yes };
        }
        let held = (holds(self.classes, &used) && outside(used))
            || self
                .heirs
                .iter()
                .any(|&(heir, _)| used == heir && outside(heir));
        if held { Refers::Held } else { Refers::No }
    }
}

/// The names of a file that take `member` from what `extends` accepts as the head they take the members of, through the heirs of the heirs, without a name that declares the member itself, which keeps its own callers and passes its own member on. A private member is not taken. Names come in the order of the file's heirs, once each.
fn heirs_of<'g>(
    symbols: FileSymbols<'g>,
    extends: impl Fn(&str) -> bool,
    member: &str,
) -> Vec<&'g str> {
    let mut found: Vec<&'g str> = Vec::new();
    if member.starts_with('#') {
        return found;
    }
    let declares = |heir: &str| {
        symbols
            .declarations()
            .any(|decl| member_in(decl.name(), heir) == Some(member))
    };
    loop {
        let before = found.len();
        for (name, of) in symbols.heirs() {
            if !found.contains(&name) && (extends(of) || found.contains(&of)) && !declares(name) {
                found.push(name);
            }
        }
        if found.len() == before {
            return found;
        }
    }
}

/// `Class` and `member` for the member `Class.member`.
fn split_member(name: &str) -> Option<(&str, &str)> {
    name.split_once(MEMBER_SEPARATOR)
}

/// `Class` for the member `Class.member`.
fn class_of(name: &str) -> Option<&str> {
    split_member(name).map(|(class, _)| class)
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
