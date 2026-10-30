//! `impact`: every file a change can break, how far away it is, and why the walk reached it.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Serialize, Serializer};

use crate::contract::Role;
use crate::fact::FileTags;
use crate::graph::Graph;
use crate::ids::{FileId, IdSet, NameId};
use crate::index::{Freshness, Index};
use crate::path::RepoPath;
use crate::text::{noun, summary};

use super::cross::Crossing;
use super::lookup::{Miss, find, possible_misses};
use super::render::{
    Answer, Entries, Footer, Searched, display_as_text, list, name_list, name_sentence,
};
use super::walk::{self, Cause, Hit, reach, reach_names};

/// What a change is asked about.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImpactRequest {
    /// The paths that changed, relative to the repository root.
    pub files: Vec<String>,
    /// The paths the caller knows were deleted. One the index still holds is traced like any changed file; one it no longer holds is listed, with the unresolved imports that may have pointed at it, and not traced.
    pub deleted: Vec<String>,
    /// Stop the walk after this many hops through code from a changed file: the imports, and with `names` the uses of a name, the loads and the mocks. A hop across a contract is not one of them, so the walk crosses every contract it comes to. A walk by name takes one hop through code when this is `None`.
    pub depth: Option<u32>,
    /// Declarations of the files in `files` to ask about instead of the files: a function, class, method (`Class.method`) or variable. The answer lists their direct callers, declaration by declaration. A name a file does not declare reaches nothing and goes into a note.
    pub names: Vec<String>,
    /// Leave the tests out of the answer, and count them instead. The walk is the same, so the footer still covers the tests it reached.
    pub no_tests: bool,
}

/// The files a change affects.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Impact {
    /// The changed files the index holds, in the order asked.
    pub changed: Vec<String>,
    /// The names asked about in those files, each once, in the order asked. Left out when the files were asked about whole.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
    /// The hops through code the walk was allowed, `None` when it ran to a fixpoint. A walk by name always has one.
    pub depth: Option<u32>,
    /// The files the change reaches, nearest first and then by path, without the tests.
    pub affected: Vec<Affected>,
    /// The files tagged as tests that the change reaches, in the same order. Empty when the request left the tests out.
    pub tests: Vec<Affected>,
    /// How many files tagged as tests the change reaches that the request left out. Left out when there are none.
    #[serde(skip_serializing_if = "is_zero")]
    pub tests_left_out: u32,
    /// The paths reported deleted that the index no longer holds.
    pub deleted: Vec<Deleted>,
    /// The paths the index does not hold that nobody reported deleted.
    pub unknown: Vec<String>,
    /// What a walk by name has to say beside the files it reached: a name a file does not declare, a file followed whole, the members whose callers are found through their classes, all in one note, a walk the depth stopped. Left out when there is nothing to say. The JSON holds each as a sentence, with every member of the note about classes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,
    /// When the change reaches nothing: the unresolved imports that may have meant a changed file.
    pub possible_misses: Vec<Miss>,
    /// What an answer that finds nothing searched.
    pub searched: Searched,
    pub footer: Footer,
}

/// A file a change reaches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Affected {
    pub path: String,
    /// Hops from the nearest changed file.
    pub distance: u32,
    /// The first way the walk got here: of the equally near ways, the one from the file with the smallest path, and from that file an import before a contract.
    pub reason: Reason,
    /// A walk by name: the declarations of the file that the walk reached, which are the callers of what it came from, in byte order of their names. Those asked about are not among them. Left out when there are none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub callers: Vec<Caller>,
    /// A walk by name: the code that runs as the file loads, when the walk reached it. Left out when it did not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load: Option<Load>,
}

/// A declaration the walk by name reached.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Caller {
    /// `run`, or `Runner.start` for a member of a class.
    pub name: String,
    /// The line the declaration starts on.
    pub line: u32,
    /// Hops from the nearest declaration asked about. The file that holds it is listed at this distance or at a nearer one.
    pub distance: u32,
}

/// The code that runs as a file loads, which the walk by name reached.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Load {
    /// Hops from the nearest declaration asked about. The file is listed at this distance or at a nearer one.
    pub distance: u32,
}

impl Affected {
    /// `; in run (line 3), stop (line 7)` for the callers of an entry the walk by name reached, then `; at load` when its load was reached. A caller or a load farther from the declarations asked about than the entry says so: `outer (line 5, distance 2)`, `; at load (distance 2)`. Callers are cut after three, as every list of names in an answer is, unless `entries` asks for all of them.
    fn write_callers(&self, f: &mut fmt::Formatter<'_>, entries: Entries) -> fmt::Result {
        if !self.callers.is_empty() {
            let callers: Vec<String> = self
                .callers
                .iter()
                .map(|caller| match self.further(caller.distance) {
                    Some(distance) => format!("{} (line {}, {distance})", caller.name, caller.line),
                    None => format!("{} (line {})", caller.name, caller.line),
                })
                .collect();
            write!(f, "; in {}", name_list(&callers, entries))?;
        }
        if let Some(load) = &self.load {
            f.write_str("; at load")?;
            if let Some(distance) = self.further(load.distance) {
                write!(f, " ({distance})")?;
            }
        }
        Ok(())
    }

    /// `distance 2` for a caller or a load that is not at the distance of its entry, because the line of the entry has said that one already.
    fn further(&self, distance: u32) -> Option<String> {
        (distance != self.distance).then(|| format!("distance {distance}"))
    }
}

/// Why the walk reached a file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "via", rename_all = "kebab-case")]
pub enum Reason {
    /// The file imports `file`, which the walk had reached. `names` are the names it imports that the change can reach, in byte order, and are left out when it takes every name of `file`.
    Imports {
        file: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        names: Vec<String>,
    },
    /// The file re-exports `names` of `file`, which the walk had reached, in byte order of the name `file` has for each, which is followed by ` as <alias>` when the file exports it under another name. They are left out when it passes none on, because `file` has nothing left to export through a star.
    ReExports {
        file: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        names: Vec<String>,
    },
    /// The file has a site with `role` in the contract `key`, which joins a contract of a file the walk had reached.
    Contract { role: Role, key: String },
    /// A walk by name: the file declares `names`, which were asked about, and its own declarations refer to them. They are in byte order.
    Declares { names: Vec<String> },
}

impl Reason {
    /// `imports useFoo, useBar from src/a.ts`, `re-exports fromA from src/a.ts`, `re-exports useFoo as useBar from src/a.ts`, `imports src/a.ts`, `calls http GET /bookings/{}`, `declares useFoo`. Names are cut after three, as every list of names in an answer is, unless `entries` asks for all of them.
    fn write(&self, f: &mut fmt::Formatter<'_>, entries: Entries) -> fmt::Result {
        let (verb, file, names) = match self {
            Self::Imports { file, names } => ("imports", file, names),
            Self::ReExports { file, names } => ("re-exports", file, names),
            Self::Contract { role, key } => return write!(f, "{role} {key}"),
            Self::Declares { names } => return write!(f, "declares {}", name_list(names, entries)),
        };
        if names.is_empty() {
            write!(f, "{verb} {file}")
        } else {
            write!(f, "{verb} {} from {file}", name_list(names, entries))
        }
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write(f, Entries::Capped)
    }
}

/// A path reported deleted, which the walk cannot trace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Deleted {
    pub path: String,
    /// The unresolved imports that may have pointed at the file.
    pub possible_misses: Vec<Miss>,
}

/// What a walk by name has to say beside the files it reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    /// A sentence of its own: a name a file does not declare, a file followed whole, a walk the depth stopped.
    Said(String),
    /// A call through a value names no class, so the callers of these members are found through the declarations that refer to their classes. Each is a member with its class, in byte order of the member. The members stay a list until the note is written, so that the text of an answer cuts them as it cuts every list of names, and the JSON of an answer holds them all.
    ThroughClass(Vec<(String, String)>),
}

impl Note {
    /// The note as a sentence. One member names its class, and several say `their classes`: `callers of Runner.start through a value are found through the declarations that refer to Runner`. The members are cut after three, as every list of names in an answer is, unless `entries` asks for all of them.
    fn write(&self, f: &mut fmt::Formatter<'_>, entries: Entries) -> fmt::Result {
        match self {
            Self::Said(text) => f.write_str(text),
            Self::ThroughClass(members) => {
                let names: Vec<String> = members.iter().map(|(member, _)| member.clone()).collect();
                let classes = match members.as_slice() {
                    [(_, class)] => class.as_str(),
                    _ => "their classes",
                };
                write!(
                    f,
                    "callers of {} through a value are found through the declarations that refer to {classes}",
                    name_sentence(&names, entries)
                )
            }
        }
    }
}

/// The note whole, which is what the JSON of an answer holds.
impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write(f, Entries::All)
    }
}

/// A note is a sentence in JSON.
impl Serialize for Note {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Whether a count is zero, for a key of the answer that is left out when there is nothing to count.
fn is_zero(count: &u32) -> bool {
    *count == 0
}

/// Which files a change to `request.files` affects, through imports followed backwards and through the contracts the files share.
pub fn impact(index: &Index, freshness: Freshness, request: &ImpactRequest) -> Impact {
    let graph = &index.graph;
    let mut changed: Vec<FileId> = Vec::new();
    let mut seen = IdSet::with_universe(graph.file_count());
    // Paths the index does not hold, by the path to print, with the order each was first asked in and whether the caller reported it deleted under any spelling. A change can name thousands of files nobody indexed, so each one must cost a lookup and not a scan of the others.
    let mut absent: BTreeMap<String, (usize, bool)> = BTreeMap::new();
    let asked = request
        .files
        .iter()
        .map(|path| (path, false))
        .chain(request.deleted.iter().map(|path| (path, true)));
    for (raw, reported_deleted) in asked {
        match find(index, raw) {
            Ok((file, _)) => {
                if seen.insert(file) {
                    changed.push(file);
                }
            }
            Err(path) => {
                let order = absent.len();
                absent.entry(path).or_insert((order, false)).1 |= reported_deleted;
            }
        }
    }

    let absent = in_asked_order(absent);
    let crossing = Crossing::new(index);
    let mut names: Vec<String> = Vec::new();
    for name in &request.names {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    let (hits, notes, depth): (Vec<Hit>, Vec<walk::Note>, Option<u32>) = if names.is_empty() {
        let reached = reach(graph, &crossing, &changed, request.depth);
        (
            reached.into_iter().map(Hit::from).collect(),
            Vec::new(),
            request.depth,
        )
    } else {
        // A walk by name takes the direct callers unless a depth is asked.
        let depth = request.depth.unwrap_or(1);
        let walked = reach_names(index, &crossing, &changed, &names, depth);
        (walked.hits, walked.notes, Some(depth))
    };
    let mut affected = Vec::new();
    let mut tests = Vec::new();
    let mut tests_left_out = 0;
    for hit in &hits {
        // A file without a path cannot be named in the answer, so it is the only one the walk reaches and the answer leaves out.
        let Some(path) = graph.path(hit.file) else {
            continue;
        };
        let entry = Affected {
            path: path.to_string(),
            distance: hit.distance,
            reason: reason(graph, &hit.cause),
            callers: callers(graph, hit),
            load: hit.load.map(|distance| Load { distance }),
        };
        if !graph.tags(hit.file).contains(FileTags::TEST) {
            affected.push(entry);
        } else if request.no_tests {
            tests_left_out += 1;
        } else {
            tests.push(entry);
        }
    }

    let changed_paths: Vec<&RepoPath> = changed
        .iter()
        .filter_map(|&file| graph.path(file))
        .collect();
    let misses = if hits.is_empty() {
        possible_misses(index, &changed_paths)
    } else {
        Vec::new()
    };
    let deleted = absent
        .iter()
        .filter(|(_, deleted)| *deleted)
        .map(|(path, _)| Deleted {
            path: path.clone(),
            possible_misses: RepoPath::normalize(path)
                .map(|path| possible_misses(index, &[&path]))
                .unwrap_or_default(),
        })
        .collect();
    let unknown = absent
        .into_iter()
        .filter(|(_, deleted)| !*deleted)
        .map(|(path, _)| path)
        .collect();

    Impact {
        changed: changed_paths.iter().map(ToString::to_string).collect(),
        names,
        depth,
        footer: Footer::new(
            index,
            freshness,
            changed
                .iter()
                .copied()
                .chain(hits.iter().map(|hit| hit.file)),
        ),
        affected,
        tests,
        tests_left_out,
        deleted,
        unknown,
        notes: notes
            .into_iter()
            .map(|walked| note(graph, walked))
            .collect(),
        possible_misses: misses,
        searched: Searched::of(index),
    }
}

/// The absent paths with whether each was reported deleted, in the order they were first asked. The map holds them by path, which is the order of nothing a person typed.
fn in_asked_order(absent: BTreeMap<String, (usize, bool)>) -> Vec<(String, bool)> {
    let mut asked: Vec<(usize, String, bool)> = absent
        .into_iter()
        .map(|(path, (order, deleted))| (order, path, deleted))
        .collect();
    asked.sort_unstable_by_key(|&(order, ..)| order);
    asked
        .into_iter()
        .map(|(_, path, deleted)| (path, deleted))
        .collect()
}

/// The declarations a walk by name reached in the file of `hit`, each with the line it starts on and its distance.
fn callers(graph: &Graph, hit: &Hit) -> Vec<Caller> {
    let Some(symbols) = graph.symbols(hit.file).filter(|_| !hit.callers.is_empty()) else {
        return Vec::new();
    };
    hit.callers
        .iter()
        .filter_map(|&(index, distance)| {
            let declaration = symbols.declaration(index)?;
            Some(Caller {
                name: declaration.name().to_owned(),
                line: declaration.line(),
                distance,
            })
        })
        .collect()
}

/// What a walk by name says beside the files it reached. A file the graph holds no path for is left empty, as in a reason.
fn note(graph: &Graph, walked: walk::Note) -> Note {
    let path = |file: FileId| {
        graph
            .path(file)
            .map(ToString::to_string)
            .unwrap_or_default()
    };
    match walked {
        walk::Note::Undeclared { file, name } => {
            Note::Said(format!("{} declares no {name}", path(file)))
        }
        walk::Note::NoDeclarations { file } => Note::Said(format!(
            "{} is followed whole because its connector records no declarations",
            path(file)
        )),
        walk::Note::ThroughClass { members } => Note::ThroughClass(members),
        walk::Note::Stopped => Note::Said("the walk stopped with callers left to follow; ask about a caller by name, or with a larger depth, to go on".to_owned()),
    }
}

/// Why the walk reached a file, spelled as far as the graph can. A reached file is never dropped for a reason that cannot be spelled in full: a list of names with one the graph holds no spelling for is left out whole, so the reason reads as the file taken in all its names, which is never less than the walk found (a shorter list would say that the file takes fewer names than it does), and a file or a contract the graph does not hold is left empty.
fn reason(graph: &Graph, cause: &Cause) -> Reason {
    let path = |file: FileId| {
        graph
            .path(file)
            .map(ToString::to_string)
            .unwrap_or_default()
    };
    match cause {
        Cause::Imports { file, names } => Reason::Imports {
            file: path(*file),
            names: spell(graph, names).unwrap_or_default(),
        },
        Cause::ReExports { file, names } => Reason::ReExports {
            file: path(*file),
            names: spell_re_exports(graph, names).unwrap_or_default(),
        },
        Cause::Contract { role, contract } => Reason::Contract {
            role: *role,
            key: graph
                .contract(*contract)
                .map(ToString::to_string)
                .unwrap_or_default(),
        },
        Cause::Declares { names } => Reason::Declares {
            names: names.clone(),
        },
    }
}

/// The names behind `ids`, `None` when the graph holds no spelling for one of them. Ids are numbered in byte order, so they come out sorted.
fn spell(graph: &Graph, ids: &[NameId]) -> Option<Vec<String>> {
    ids.iter()
        .map(|&id| graph.name(id).map(str::to_owned))
        .collect()
}

/// The names a file re-exports, each as `name` or, when the file exports it under another name, `name as alias`. `None` when the graph holds no spelling for one of them.
fn spell_re_exports(graph: &Graph, names: &[(NameId, NameId)]) -> Option<Vec<String>> {
    names
        .iter()
        .map(|&(source, exported_as)| {
            let source_name = graph.name(source)?;
            if source == exported_as {
                Some(source_name.to_owned())
            } else {
                Some(format!("{source_name} as {}", graph.name(exported_as)?))
            }
        })
        .collect()
}

impl Answer for Impact {
    fn write_text(&self, f: &mut fmt::Formatter<'_>, entries: Entries) -> fmt::Result {
        let mut asked = self.changed.clone();
        asked.extend(self.deleted.iter().map(|deleted| deleted.path.clone()));
        asked.extend(self.unknown.iter().cloned());
        let files = if asked.is_empty() {
            "no files".to_owned()
        } else {
            summary(&asked)
        };
        if self.names.is_empty() {
            write!(f, "impact of {files}")?;
        } else {
            write!(f, "impact of {} in {files}", summary(&self.names))?;
        }
        if let Some(depth) = self.depth {
            write!(f, ", within {} through code", noun(depth, "hop"))?;
        }
        f.write_str("\n")?;

        let affected_line = |f: &mut fmt::Formatter<'_>, affected: &Affected| {
            write!(f, "{}  distance {}  ", affected.path, affected.distance)?;
            affected.reason.write(f, entries)?;
            affected.write_callers(f, entries)
        };
        list(f, "affected files", &self.affected, entries, affected_line)?;
        list(f, "tests", &self.tests, entries, affected_line)?;
        if self.tests_left_out > 0 {
            writeln!(f, "{} left out", noun(self.tests_left_out, "test file"))?;
        }
        for deleted in &self.deleted {
            writeln!(f, "{} is deleted, so it is not traced", deleted.path)?;
            let heading = format!("possible misses for {}", deleted.path);
            list(f, &heading, &deleted.possible_misses, entries, |f, miss| {
                write!(f, "{miss}")
            })?;
        }
        for path in &self.unknown {
            writeln!(f, "{path} is not in the index")?;
        }
        for note in &self.notes {
            f.write_str("note: ")?;
            note.write(f, entries)?;
            f.write_str("\n")?;
        }
        if !self.changed.is_empty()
            && self.affected.is_empty()
            && self.tests.is_empty()
            && self.tests_left_out == 0
        {
            let subject = if self.names.is_empty() {
                summary(&self.changed)
            } else {
                format!("{} in {}", summary(&self.names), summary(&self.changed))
            };
            writeln!(
                f,
                "nothing is affected by a change to {subject} among {}",
                self.searched
            )?;
            list(
                f,
                "possible misses",
                &self.possible_misses,
                entries,
                |f, miss| write!(f, "{miss}"),
            )?;
        }
        self.footer.write(f)
    }
}

display_as_text!(Impact);

#[cfg(test)]
mod tests {
    use crate::ids::ContractId;
    use crate::testing::{Tree, owned};

    use super::*;

    /// A name the graph holds no spelling for. The builder numbers a name together with its spelling, so no graph the testing module builds has an edge that carries such an id, and the cause below is made by hand with the id the builder would fall back on.
    const UNSPELLED: NameId = NameId::from_raw(u32::MAX);

    /// The graph of `src/a.ts` importing `foo` from `src/b.ts`, with the ids of `src/b.ts` and of `foo`.
    fn graph() -> (Index, FileId, NameId) {
        let index = Tree::new()
            .imports_names("src/a.ts", "src/b.ts", &["foo"])
            .build();
        let file = index.graph.file_id("src/b.ts").unwrap();
        let foo = index.graph.name_id("foo").unwrap();
        (index, file, foo)
    }

    #[test]
    fn names_the_graph_spells_stay_in_the_reason() {
        let (index, file, foo) = graph();
        let imports = Cause::Imports {
            file,
            names: vec![foo],
        };
        assert_eq!(
            reason(&index.graph, &imports),
            Reason::Imports {
                file: "src/b.ts".to_owned(),
                names: owned(&["foo"]),
            }
        );
        let re_exports = Cause::ReExports {
            file,
            names: vec![(foo, foo)],
        };
        assert_eq!(
            reason(&index.graph, &re_exports),
            Reason::ReExports {
                file: "src/b.ts".to_owned(),
                names: owned(&["foo"]),
            }
        );
    }

    #[test]
    fn one_name_the_graph_cannot_spell_leaves_the_whole_file_as_the_reason() {
        let (index, file, foo) = graph();
        let imports = Cause::Imports {
            file,
            names: vec![foo, UNSPELLED],
        };
        assert_eq!(
            reason(&index.graph, &imports),
            Reason::Imports {
                file: "src/b.ts".to_owned(),
                names: Vec::new(),
            }
        );
        let re_exports = Cause::ReExports {
            file,
            names: vec![(foo, foo), (foo, UNSPELLED)],
        };
        assert_eq!(
            reason(&index.graph, &re_exports),
            Reason::ReExports {
                file: "src/b.ts".to_owned(),
                names: Vec::new(),
            }
        );
    }

    #[test]
    fn a_file_or_a_contract_the_graph_does_not_hold_leaves_its_part_of_the_reason_empty() {
        let (index, ..) = graph();
        let imports = Cause::Imports {
            file: FileId::from_raw(u32::MAX),
            names: Vec::new(),
        };
        assert_eq!(
            reason(&index.graph, &imports),
            Reason::Imports {
                file: String::new(),
                names: Vec::new(),
            }
        );
        let contract = Cause::Contract {
            role: Role::Calls,
            contract: ContractId::from_raw(u32::MAX),
        };
        assert_eq!(
            reason(&index.graph, &contract),
            Reason::Contract {
                role: Role::Calls,
                key: String::new(),
            }
        );
    }
}
