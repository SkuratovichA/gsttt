//! Routes that only show across files: a router built in one file and mounted in another, a scope in `main.rs` that prefixes handlers declared elsewhere.
//!
//! Reading a file records, for each chain of method calls or handler that serves routes, a [`Group`] with the routes it serves and the routers it mounts, and for each function the groups it hands back. Linking follows the mounts from the groups nothing mounts, collecting the prefixes on the way, and reports every route of a mounted router once for each mount, keyed again with the prefix in front. Each reported fact names the route the file declares, so the engine drops that one: a router is served under its mounts and under no other path. A mount at the root is reported only for a router that is mounted under a prefix as well, because a router that is merged and nothing else is served by the route the file declares. A router that nothing mounts is not reported, and neither is a route below a cap, so both keep the paths they are written with.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::connector::{LinkFile, LinkInput, LinkedFact};
use gsttt_core::contract::ContractKind;
use gsttt_core::fact::{ContractFact, Resolution};
use gsttt_core::ids::FileId;
use gsttt_core::path::RepoPath;
use gsttt_core::rules::contract_key;
use serde::{Deserialize, Serialize};

use crate::imports;

/// How deep routers may be mounted in one another before the walk stops, so a generated chain of thousands cannot exhaust the stack. No program nests routers anywhere near this.
pub(crate) const MAX_DEPTH: usize = 64;

/// How many times one walk may enter a router, so that routers mounted twice at every level, which give a path for each way down, cannot take the index with them.
pub(crate) const MAX_VISITS: usize = 50_000;

/// The cap that cut a walk short.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cap {
    /// Routers nested deeper than [`MAX_DEPTH`].
    Depth,
    /// More entries into routers than [`MAX_VISITS`].
    Visits,
}

impl Cap {
    fn name(self) -> &'static str {
        match self {
            Self::Depth => "depth",
            Self::Visits => "visits",
        }
    }

    fn limit(self) -> usize {
        match self {
            Self::Depth => MAX_DEPTH,
            Self::Visits => MAX_VISITS,
        }
    }
}

/// The walk from one router that a cap cut short, so the routes below the cut keep the paths they are written with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cut {
    file: FileId,
    anchor: u32,
    cap: Cap,
}

/// What linking needs from one file. The engine caches it with the file's facts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Extra {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) groups: Vec<Group>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) functions: Vec<Function>,
}

/// Routes that are served together: a chain of method calls such as `Router::new().route(..).nest(..)`, or a handler that an attribute gives a path to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Group {
    /// Where the chain or the handler starts in the file, which is also how the routes of the facts are told apart.
    pub(crate) anchor: u32,
    /// What a `scope` puts in front of everything in the group.
    pub(crate) base: Prefix,
    /// The routes the group serves itself, as the file's facts have them.
    pub(crate) routes: Vec<ContractFact>,
    pub(crate) mounts: Vec<Mount>,
}

/// Another router put inside a group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Mount {
    pub(crate) prefix: Prefix,
    pub(crate) target: Target,
}

/// A function and the routers it hands back, for a router that is built in one place and mounted in another.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Function {
    pub(crate) name: String,
    pub(crate) returns: Vec<Target>,
}

/// Which router an expression stands for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Target {
    /// A group of the same file, by its anchor.
    Local(u32),
    /// A function to follow, by the path of the call as written: `["bookings", "router"]`.
    Path(Vec<String>),
}

/// A route prefix as the source gives it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Prefix {
    /// Known text. Empty when there is no prefix.
    Literal(String),
    /// Worked out at run time, so what sits behind it cannot be placed.
    Computed,
}

impl Prefix {
    /// No prefix at all.
    pub(crate) fn none() -> Self {
        Self::Literal(String::new())
    }
}

/// The routes of the files of one project, once for every prefix they are mounted behind. `resolve` says which file the module path written in a file reaches. A walk that a cap cuts short is logged here, and every project names the caps among its blind spots, because the link step returns facts or an error and never both.
pub(crate) fn link(
    input: &LinkInput<'_, Extra>,
    resolve: &dyn Fn(&RepoPath, &str) -> Resolution,
) -> Vec<LinkedFact> {
    let (facts, cuts) = follow(input, resolve);
    for cut in cuts {
        let file = input.get(cut.file).map(|file| file.path.as_str());
        tracing::warn!(
            file,
            anchor = cut.anchor,
            cap = cut.cap.name(),
            limit = cut.cap.limit(),
            "router mounts cut at a cap, so the routes below the cut keep the paths they are written with"
        );
    }
    facts
}

/// [`link`], with the walks that a cap cut short.
fn follow(
    input: &LinkInput<'_, Extra>,
    resolve: &dyn Fn(&RepoPath, &str) -> Resolution,
) -> (Vec<LinkedFact>, Vec<Cut>) {
    let graph = Graph::new(input, resolve);
    let edges: Vec<Vec<Edge<'_>>> = graph.nodes.iter().map(|node| graph.edges(node)).collect();
    let mut mounted = vec![false; graph.nodes.len()];
    for target in edges.iter().flatten().flat_map(|edge| &edge.to) {
        if let Some(flag) = mounted.get_mut(*target) {
            *flag = true;
        }
    }
    let mut walk = Walk {
        graph: &graph,
        edges: &edges,
        seen: BTreeSet::new(),
        visits: 0,
        cut: None,
        out: Vec::new(),
        bare: Vec::new(),
        prefixed: BTreeSet::new(),
    };
    let mut cuts = Vec::new();
    // A group nothing mounts is where a program starts serving, so the prefixes start there. Groups that only mount each other in a ring are never reached.
    for (root, _) in mounted.iter().enumerate().filter(|(_, mounted)| !**mounted) {
        // Each of them has a budget of its own, so one that fans out cannot leave the next with none.
        walk.visits = 0;
        walk.visit(root, &[], &mut Vec::new());
        if let (Some(cap), Some(node)) = (walk.cut.take(), graph.nodes.get(root)) {
            cuts.push(Cut {
                file: node.file,
                anchor: node.group.anchor,
                cap,
            });
        }
    }
    let Walk {
        mut out,
        bare,
        prefixed,
        ..
    } = walk;
    // Dropping the route a file declares for a mount under a prefix takes the mount at the root with it, so that one is reported too. A router that is merged and nothing else keeps the route the file declares.
    out.extend(
        bare.into_iter()
            .filter(|(node, _)| prefixed.contains(node))
            .map(|(_, fact)| fact),
    );
    let mut facts = out;
    facts.sort_by(|a, b| {
        (a.file, a.fact.line, &a.fact.key, &a.fact.via, &a.replaces).cmp(&(
            b.file,
            b.fact.line,
            &b.fact.key,
            &b.fact.via,
            &b.replaces,
        ))
    });
    facts.dedup();
    (facts, cuts)
}

/// A group, with the file it is in.
struct Node<'a> {
    file: FileId,
    group: &'a Group,
}

/// What a mount of a group reaches, and the prefix it puts on it.
struct Edge<'a> {
    prefix: &'a Prefix,
    to: Vec<usize>,
}

/// The groups of a project, and what is needed to follow a path from one of them to another.
struct Graph<'a> {
    input: &'a LinkInput<'a, Extra>,
    resolve: &'a dyn Fn(&RepoPath, &str) -> Resolution,
    nodes: Vec<Node<'a>>,
    /// The place of a group in `nodes`, by its file and anchor.
    index: BTreeMap<(FileId, u32), usize>,
}

impl<'a> Graph<'a> {
    fn new(
        input: &'a LinkInput<'a, Extra>,
        resolve: &'a dyn Fn(&RepoPath, &str) -> Resolution,
    ) -> Self {
        let mut nodes = Vec::new();
        let mut index = BTreeMap::new();
        for file in input.files().iter().copied() {
            for group in &file.extra.groups {
                index.insert((file.id, group.anchor), nodes.len());
                nodes.push(Node {
                    file: file.id,
                    group,
                });
            }
        }
        Self {
            input,
            resolve,
            nodes,
            index,
        }
    }

    fn edges(&self, node: &Node<'a>) -> Vec<Edge<'a>> {
        node.group
            .mounts
            .iter()
            .map(|mount| Edge {
                prefix: &mount.prefix,
                to: self.reach(node.file, &mount.target),
            })
            .collect()
    }

    /// The groups that `target`, written in `file`, stands for. A function is followed once, however many ways lead to it, which is also what ends functions that return each other's routers.
    fn reach(&self, file: FileId, target: &'a Target) -> Vec<usize> {
        let mut groups = Vec::new();
        let mut followed = BTreeSet::new();
        let mut pending = vec![(file, target)];
        while let Some((file, target)) = pending.pop() {
            match target {
                Target::Local(anchor) => groups.extend(self.index.get(&(file, *anchor))),
                Target::Path(names) => {
                    let Some(home) = self.input.get(file).copied() else {
                        continue;
                    };
                    let Some((owner, name)) = self.locate(&home, names) else {
                        continue;
                    };
                    let Some(owner) = self.input.get(owner).copied() else {
                        continue;
                    };
                    if followed.insert((owner.id, name)) {
                        pending.extend(
                            owner
                                .extra
                                .functions
                                .iter()
                                .filter(|function| function.name == name)
                                .flat_map(|function| &function.returns)
                                .map(|returned| (owner.id, returned)),
                        );
                    }
                }
            }
        }
        groups.sort_unstable();
        groups.dedup();
        groups
    }

    /// The file a function called through `names` is in, and its name there. A name on its own is a function of the file or something it brought in with `use`; `Self::name` is a function of the file; anything longer names the module that holds the function.
    fn locate(&self, home: &LinkFile<'a, Extra>, names: &'a [String]) -> Option<(FileId, &'a str)> {
        let (name, module) = names.split_last()?;
        match module {
            [] => {
                if home
                    .extra
                    .functions
                    .iter()
                    .any(|function| function.name == *name)
                {
                    return Some((home.id, name.as_str()));
                }
                let brought = home
                    .imports
                    .iter()
                    .find(|resolved| imports::binding(&resolved.import) == Some(name.as_str()))?;
                let Resolution::File { file } = &brought.to else {
                    return None;
                };
                // `use a::b as c` binds `c`, but the function is still called `b` where it is defined.
                Some((*file, brought.import.spec.rsplit("::").next()?))
            }
            [only] if only == "Self" => Some((home.id, name.as_str())),
            _ => match (self.resolve)(home.path, &module.join("::")) {
                Resolution::File { file } => Some((file, name.as_str())),
                _ => None,
            },
        }
    }
}

/// A walk down the mounts, from the groups nothing mounts.
struct Walk<'a> {
    graph: &'a Graph<'a>,
    edges: &'a [Vec<Edge<'a>>],
    /// The groups reached and the prefix they were reached with, so a router mounted twice the same way is read once.
    seen: BTreeSet<(usize, String)>,
    /// Entries into routers since the walk from the current root began.
    visits: usize,
    /// The cap that cut the walk from the current root, if one did.
    cut: Option<Cap>,
    /// The facts for the mounts behind a prefix.
    out: Vec<LinkedFact>,
    /// The facts for the mounts at the root, with the group they are of.
    bare: Vec<(usize, LinkedFact)>,
    /// The groups that were reported behind a prefix.
    prefixed: BTreeSet<usize>,
}

impl<'a> Walk<'a> {
    /// Visits `node` entered behind the prefix `entering`, parts in the order they are written. `stack` holds the groups being visited, so a router that mounts itself does not go round for ever.
    fn visit(&mut self, node: usize, entering: &[&'a str], stack: &mut Vec<usize>) {
        let (graph, edges) = (self.graph, self.edges);
        let Some(current) = graph.nodes.get(node) else {
            return;
        };
        // A scope whose prefix is computed puts everything in it at a path that cannot be told.
        let Prefix::Literal(base) = &current.group.base else {
            return;
        };
        let here = with(entering, base);
        if stack.len() >= MAX_DEPTH {
            self.cut.get_or_insert(Cap::Depth);
            return;
        }
        if self.visits >= MAX_VISITS {
            self.cut.get_or_insert(Cap::Visits);
            return;
        }
        if stack.contains(&node) || !self.seen.insert((node, here.join("/"))) {
            return;
        }
        self.visits += 1;
        // A group that starts a walk is reported only behind the prefix of its own scope; one entered through a mount is reported at the root as well.
        if !stack.is_empty() || !here.is_empty() {
            self.report(node, current, &here);
        }
        stack.push(node);
        for edge in edges.get(node).into_iter().flatten() {
            let Prefix::Literal(prefix) = edge.prefix else {
                continue;
            };
            let below = with(&here, prefix);
            for target in &edge.to {
                self.visit(*target, &below, stack);
            }
        }
        stack.pop();
    }

    /// Reports the routes of the group `index`, which is `node`, behind `prefix`, each keying again the route that the file declares. A mount at the root keys it as it is written, and waits in `bare` until the walk is over.
    fn report(&mut self, index: usize, node: &Node<'a>, prefix: &[&str]) {
        let behind = prefix.join("/");
        for route in &node.group.routes {
            let Some((method, path)) = route.key.http_parts() else {
                continue;
            };
            let key = contract_key(&ContractKind::Http, Some(&behind), path, Some(method));
            let fact = LinkedFact::rekeyed(node.file, route, key);
            if prefix.is_empty() {
                self.bare.push((index, fact));
            } else {
                self.prefixed.insert(index);
                self.out.push(fact);
            }
        }
    }
}

/// `parts` with `part` after them, unless it is nothing but slashes. Slashes between the parts are the key's to sort out.
fn with<'a>(parts: &[&'a str], part: &'a str) -> Vec<&'a str> {
    let mut parts = parts.to_vec();
    if !part.trim_matches('/').is_empty() {
        parts.push(part);
    }
    parts
}
