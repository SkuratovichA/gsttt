//! `impact`: every file a change can break, how far away it is, and why the walk reached it.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

use crate::contract::Role;
use crate::fact::FileTags;
use crate::graph::Graph;
use crate::ids::{FileId, IdSet, NameId};
use crate::index::{Freshness, Index};
use crate::path::RepoPath;
use crate::text::{noun, summary};

use super::cross::Crossing;
use super::lookup::{Miss, find, possible_misses};
use super::render::{Answer, Entries, Footer, Searched, display_as_text, list, name_list};
use super::walk::{Cause, reach};

/// What a change is asked about.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImpactRequest {
    /// The paths that changed, relative to the repository root.
    pub files: Vec<String>,
    /// The paths the caller knows were deleted. One the index still holds is traced like any changed file; one it no longer holds is listed, with the unresolved imports that may have pointed at it, and not traced.
    pub deleted: Vec<String>,
    /// Stop the walk after this many hops from a changed file.
    pub depth: Option<u32>,
}

/// The files a change affects.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Impact {
    /// The changed files the index holds, in the order asked.
    pub changed: Vec<String>,
    /// The hops the walk was allowed, `None` when it ran to a fixpoint.
    pub depth: Option<u32>,
    /// The files the change reaches, nearest first and then by path, without the tests.
    pub affected: Vec<Affected>,
    /// The files tagged as tests that the change reaches, in the same order.
    pub tests: Vec<Affected>,
    /// The paths reported deleted that the index no longer holds.
    pub deleted: Vec<Deleted>,
    /// The paths the index does not hold that nobody reported deleted.
    pub unknown: Vec<String>,
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
    /// The file re-exports `names` of `file`, which the walk had reached, in byte order. They are left out when it passes none on, because `file` has nothing left to export through a star.
    ReExports {
        file: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        names: Vec<String>,
    },
    /// The file has a site with `role` in the contract `key`, which joins a contract of a file the walk had reached.
    Contract { role: Role, key: String },
}

impl Reason {
    /// `imports useFoo, useBar from src/a.ts`, `re-exports fromA from src/a.ts`, `imports src/a.ts`, `calls http GET /bookings/{}`. Names are cut after three, as every list of names in an answer is, unless `entries` asks for all of them.
    fn write(&self, f: &mut fmt::Formatter<'_>, entries: Entries) -> fmt::Result {
        let (verb, file, names) = match self {
            Self::Imports { file, names } => ("imports", file, names),
            Self::ReExports { file, names } => ("re-exports", file, names),
            Self::Contract { role, key } => return write!(f, "{role} {key}"),
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
    let reached = reach(graph, &crossing, &changed, request.depth);
    let mut affected = Vec::new();
    let mut tests = Vec::new();
    for reached in &reached {
        let (Some(path), Some(reason)) = (graph.path(reached.file), reason(graph, &reached.cause))
        else {
            continue;
        };
        let entry = Affected {
            path: path.to_string(),
            distance: reached.distance,
            reason,
        };
        if graph.tags(reached.file).contains(FileTags::TEST) {
            tests.push(entry);
        } else {
            affected.push(entry);
        }
    }

    let changed_paths: Vec<&RepoPath> = changed
        .iter()
        .filter_map(|&file| graph.path(file))
        .collect();
    let misses = if reached.is_empty() {
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
        depth: request.depth,
        footer: Footer::new(
            index,
            freshness,
            changed
                .iter()
                .copied()
                .chain(reached.iter().map(|reached| reached.file)),
        ),
        affected,
        tests,
        deleted,
        unknown,
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

fn reason(graph: &Graph, cause: &Cause) -> Option<Reason> {
    match cause {
        Cause::Imports { file, names } => Some(Reason::Imports {
            file: graph.path(*file)?.to_string(),
            names: spell(graph, names)?,
        }),
        Cause::ReExports { file, names } => Some(Reason::ReExports {
            file: graph.path(*file)?.to_string(),
            names: spell(graph, names)?,
        }),
        Cause::Contract { role, contract } => Some(Reason::Contract {
            role: *role,
            key: graph.contract(*contract)?.to_string(),
        }),
    }
}

/// The names behind `ids`. Ids are numbered in byte order, so they come out sorted.
fn spell(graph: &Graph, ids: &[NameId]) -> Option<Vec<String>> {
    ids.iter()
        .map(|&id| graph.name(id).map(str::to_owned))
        .collect()
}

impl Answer for Impact {
    fn write_text(&self, f: &mut fmt::Formatter<'_>, entries: Entries) -> fmt::Result {
        let mut asked = self.changed.clone();
        asked.extend(self.deleted.iter().map(|deleted| deleted.path.clone()));
        asked.extend(self.unknown.iter().cloned());
        if asked.is_empty() {
            f.write_str("impact of no files")?;
        } else {
            write!(f, "impact of {}", summary(&asked))?;
        }
        if let Some(depth) = self.depth {
            write!(f, ", within {}", noun(depth, "hop"))?;
        }
        f.write_str("\n")?;

        let affected_line = |f: &mut fmt::Formatter<'_>, affected: &Affected| {
            write!(f, "{}  distance {}  ", affected.path, affected.distance)?;
            affected.reason.write(f, entries)
        };
        list(f, "affected files", &self.affected, entries, affected_line)?;
        list(f, "tests", &self.tests, entries, affected_line)?;
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
        if !self.changed.is_empty() && self.affected.is_empty() && self.tests.is_empty() {
            writeln!(
                f,
                "nothing is affected by a change to {} among {}",
                summary(&self.changed),
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
