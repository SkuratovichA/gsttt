//! `path`: a shortest chain of imports and contract hops from one file to another.

use std::fmt;

use serde::Serialize;

use crate::contract::Role;
use crate::fact::ImportKind;
use crate::graph::{Edge, Graph};
use crate::ids::{ContractId, FileId, IdSet, IdVec};
use crate::index::{Freshness, Index};
use crate::text::noun;

use super::cross::{Crossed, Crossing, Direction};
use super::lookup::find;
use super::neighbours::kind_label;
use super::render::{Answer, Entries, Footer, display_as_text, rows};

/// How a chain gets from one file to the next.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "via", rename_all = "kebab-case")]
pub enum Link {
    /// The file imports the next one, with these kinds of import.
    Import {
        kinds: Vec<ImportKind>,
        /// Whether the only import is a `mod` declaration, which owns the next file instead of using it.
        structural: bool,
    },
    /// The next file has a site with `role` in a contract that the file has a site in, and `key` is the key of that site.
    Contract { role: Role, key: String },
}

/// `by import`, `by mod declaration`, `by http GET /bookings/{}`.
impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // A `mod` declaration owns its child instead of using it, so a reader should not take it for a use.
            Self::Import {
                structural: true, ..
            } => write!(f, "by {}", kind_label(ImportKind::Module)),
            Self::Import { .. } => f.write_str("by import"),
            Self::Contract { key, .. } => write!(f, "by {key}"),
        }
    }
}

/// A file of a chain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Step {
    pub path: String,
    /// How the chain got here from the step before it; `None` for the first step.
    pub by: Option<Link>,
}

/// A shortest chain from one file to another.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Route {
    /// The file the chain starts at as the index names it, or as it was asked when the index does not hold it.
    pub from: String,
    pub to: String,
    /// The paths asked about that the index does not hold. No chain is searched for while this is not empty.
    pub unknown: Vec<String>,
    /// The chain from `from` to `to`, both included. Empty when no chain joins them.
    pub steps: Vec<Step>,
    /// How many files `from` reaches, set when both files are in the index and no chain joins them.
    pub reach: Option<u32>,
    pub footer: Footer,
}

/// A shortest chain from `from` to `to`, through imports and contracts.
///
/// A hop is an import, a hop from a file that calls a contract to a file that serves or declares it, or a hop from a file that serves a contract to a file that declares it. HTTP hops go to the sites of every key that meets the key. Of several shortest chains the answer reaches each file from the file with the smallest path among those one hop nearer; from that file an import comes before a contract, and contracts go in key order.
pub fn path(index: &Index, freshness: Freshness, from: &str, to: &str) -> Route {
    let (from_name, start) = locate(index, from);
    let (to_name, goal) = locate(index, to);
    let mut unknown: Vec<String> = [(&from_name, start), (&to_name, goal)]
        .into_iter()
        .filter(|(_, file)| file.is_none())
        .map(|(name, _)| name.clone())
        .collect();
    unknown.dedup();
    let (Some(start), Some(goal)) = (start, goal) else {
        return Route {
            from: from_name,
            to: to_name,
            unknown,
            steps: Vec::new(),
            reach: None,
            footer: Footer::new(index, freshness, start.into_iter().chain(goal)),
        };
    };

    let chase = Chase::run(index, start, goal);
    let chain = chase.chain(start, goal);
    let reach = chain
        .is_none()
        .then(|| u32::try_from(chase.seen.len().saturating_sub(1)).unwrap_or(u32::MAX));
    let files = chain.as_ref().map_or_else(
        || vec![start, goal],
        |chain| chain.iter().map(|&(file, _)| file).collect(),
    );
    let steps = chain
        .iter()
        .flatten()
        .filter_map(|&(file, link)| {
            Some(Step {
                path: index.graph.path(file)?.to_string(),
                by: link.and_then(|hop| hop.link(&index.graph)),
            })
        })
        .collect();
    Route {
        from: from_name,
        to: to_name,
        unknown,
        steps,
        reach,
        footer: Footer::new(index, freshness, files),
    }
}

/// The name to print for `raw` and the file it names, when the index holds one.
fn locate(index: &Index, raw: &str) -> (String, Option<FileId>) {
    match find(index, raw) {
        Ok((file, name)) => (name.to_string(), Some(file)),
        Err(name) => (name, None),
    }
}

/// How the walk got to a file.
#[derive(Clone, Copy, Debug)]
enum How {
    Import(Edge),
    Contract { role: Role, contract: ContractId },
}

/// The file the walk came from, and how.
#[derive(Clone, Copy, Debug)]
struct Hop {
    from: FileId,
    how: How,
}

impl Hop {
    fn link(self, graph: &Graph) -> Option<Link> {
        match self.how {
            How::Import(edge) => Some(Link::Import {
                kinds: edge.kinds().collect(),
                structural: edge.is_structural(),
            }),
            How::Contract { role, contract } => Some(Link::Contract {
                role,
                key: graph.contract(contract)?.to_string(),
            }),
        }
    }
}

/// A breadth-first search from one file along imports and contract hops, level by level so that the first hop to a file is on a shortest chain, and each level in path order so that a file is reached from the smallest path one hop nearer, which is the same on every run.
struct Chase<'a> {
    graph: &'a Graph,
    crossing: Crossing<'a>,
    seen: IdSet<FileId>,
    /// The hop that first reached each file.
    parent: IdVec<FileId, Option<Hop>>,
    /// The contracts crossed so far, from callers and servers towards declarers.
    crossed: Crossed,
    next: Vec<FileId>,
}

impl<'a> Chase<'a> {
    /// Searches from `start` until `goal` is reached or nothing more is.
    fn run(index: &'a Index, start: FileId, goal: FileId) -> Self {
        let graph = &index.graph;
        let mut chase = Self {
            graph,
            crossing: Crossing::new(index),
            seen: IdSet::with_universe(graph.file_count()),
            parent: (0..graph.file_count()).map(|_| None).collect(),
            crossed: Crossed::new(Direction::ToDeclarers, graph),
            next: Vec::new(),
        };
        chase.seen.insert(start);
        let mut frontier = vec![start];
        while !frontier.is_empty() && !chase.seen.contains(goal) {
            for &file in &frontier {
                chase.expand(file);
            }
            frontier = std::mem::take(&mut chase.next);
            frontier.sort_unstable();
        }
        chase
    }

    fn expand(&mut self, file: FileId) {
        let graph = self.graph;
        for &(imported, edge) in graph.dependencies(file) {
            self.visit(imported, file, How::Import(edge));
        }
        for &contract in graph.contracts_of(file) {
            for (meeting, site) in self.crossing.cross(&mut self.crossed, file, contract) {
                let how = How::Contract {
                    role: site.role,
                    contract: meeting,
                };
                self.visit(site.file, file, how);
            }
        }
    }

    fn visit(&mut self, file: FileId, from: FileId, how: How) {
        if !self.seen.insert(file) {
            return;
        }
        if let Some(slot) = self.parent.get_mut(file) {
            *slot = Some(Hop { from, how });
        }
        self.next.push(file);
    }

    /// The files from `start` to `goal` with the hop that reached each, `start` first; `None` when the search never reached `goal`.
    fn chain(&self, start: FileId, goal: FileId) -> Option<Vec<(FileId, Option<Hop>)>> {
        if !self.seen.contains(goal) {
            return None;
        }
        let mut chain = Vec::new();
        let mut at = goal;
        while at != start {
            let hop = self.parent.get(at).copied().flatten()?;
            chain.push((at, Some(hop)));
            at = hop.from;
        }
        chain.push((start, None));
        chain.reverse();
        Some(chain)
    }
}

impl Answer for Route {
    fn held(&self) -> bool {
        self.unknown.is_empty()
    }

    fn write_text(&self, f: &mut fmt::Formatter<'_>, entries: Entries) -> fmt::Result {
        if !self.unknown.is_empty() {
            for path in &self.unknown {
                writeln!(f, "{path} is not in the index")?;
            }
        } else if let Some(reach) = self.reach {
            writeln!(
                f,
                "no chain from {} to {}: {} reaches {}",
                self.from,
                self.to,
                self.from,
                noun(reach, "file")
            )?;
        } else {
            let hops = u32::try_from(self.steps.len().saturating_sub(1)).unwrap_or(u32::MAX);
            writeln!(
                f,
                "path from {} to {}, {}",
                self.from,
                self.to,
                noun(hops, "hop")
            )?;
            rows(f, &self.steps, entries, |f, step| match &step.by {
                Some(link) => write!(f, "{}  {link}", step.path),
                None => write!(f, "{}", step.path),
            })?;
        }
        self.footer.write(f)
    }
}

display_as_text!(Route);
