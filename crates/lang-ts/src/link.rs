//! The link phase: what only the whole repository shows, which is how routers mount under each other across files and
//! how tRPC routers compose.

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::FileId;
use gsttt_core::connector::{LinkFile, LinkInput, LinkedFact};
use gsttt_core::contract::{ContractKey, ContractKind, Role};
use gsttt_core::fact::{ContractFact, Line, Resolution};
use gsttt_core::rules::contract_key;

use crate::extra::{ExportTarget, MountTarget, TrpcEntry, TrpcRouter, TsExtra};

/// How far a name is followed through imports and re-exports before the chain is given up.
const MAX_DEPTH: usize = 16;

/// A name in a file.
type Node = (FileId, String);

/// The facts that need other files: routes under the prefixes their routers are mounted at, and tRPC procedures under
/// the routers that compose them.
pub(crate) fn link(input: &LinkInput<'_, TsExtra>) -> Vec<LinkedFact> {
    let mut linked = express(input);
    linked.extend(trpc(input));
    linked
}

/// The file a specifier written in `file` leads to.
fn module_file(file: &LinkFile<'_, TsExtra>, spec: &str) -> Option<FileId> {
    file.imports.iter().find_map(|resolved| match &resolved.to {
        Resolution::File { file } if resolved.import.spec == spec => Some(*file),
        _ => None,
    })
}

/// Where `name`, written in `file`, is defined: itself, or what the file it was imported from exports.
fn define(input: &LinkInput<'_, TsExtra>, file: FileId, name: &str, depth: usize) -> Option<Node> {
    let here = input.get(file)?;
    let Some(binding) = here
        .extra
        .bindings
        .iter()
        .find(|binding| binding.local == name)
    else {
        return Some((file, name.to_owned()));
    };
    if depth >= MAX_DEPTH {
        return None;
    }
    export_of(
        input,
        module_file(here, &binding.spec)?,
        &binding.name,
        depth + 1,
    )
}

/// What `file` exports as `name`, as the name it has where it is defined.
fn export_of(
    input: &LinkInput<'_, TsExtra>,
    file: FileId,
    name: &str,
    depth: usize,
) -> Option<Node> {
    if depth >= MAX_DEPTH {
        return None;
    }
    let here = input.get(file)?;
    if let Some(export) = here.extra.exports.iter().find(|export| export.name == name) {
        return match &export.target {
            ExportTarget::Local(local) => define(input, file, local, depth + 1),
            ExportTarget::From {
                spec,
                name: original,
            } => export_of(input, module_file(here, spec)?, original, depth + 1),
        };
    }
    here.extra
        .exports
        .iter()
        .filter(|export| export.name == "*")
        .find_map(|export| match &export.target {
            ExportTarget::From { spec, .. } => {
                export_of(input, module_file(here, spec)?, name, depth + 1)
            }
            ExportTarget::Local(_) => None,
        })
}

fn express(input: &LinkInput<'_, TsExtra>) -> Vec<LinkedFact> {
    let mut parents: BTreeMap<Node, Vec<(Node, String)>> = BTreeMap::new();
    for file in input.files() {
        for mount in &file.extra.mounts {
            for target in &mount.targets {
                if let Some(child) = mounted_host(input, file, target) {
                    parents
                        .entry(child)
                        .or_default()
                        .push(((file.id, mount.host.clone()), mount.prefix.clone()));
                }
            }
        }
    }
    let mut mounts = Mounts {
        parents: &parents,
        memo: BTreeMap::new(),
        cuts: 0,
    };
    let mut linked = Vec::new();
    for file in input.files() {
        for route in &file.extra.routes {
            let node = (file.id, route.host.clone());
            let mut prefixes = mounts.prefixes_of(&node, &mut Vec::new());
            // A router that is only reached through a cycle is served where it is written.
            if prefixes.is_empty() {
                prefixes.insert(String::new());
            }
            for prefix in prefixes {
                linked.push(LinkedFact {
                    file: file.id,
                    fact: rekey(&route.fact, &prefix),
                });
            }
        }
    }
    if mounts.cuts > 0 {
        tracing::debug!(
            cuts = mounts.cuts,
            "mounts close a cycle and are cut where they close"
        );
    }
    linked
}

/// The router or app a mount attaches, when it is one that was made in some file.
fn mounted_host(
    input: &LinkInput<'_, TsExtra>,
    file: &LinkFile<'_, TsExtra>,
    target: &MountTarget,
) -> Option<Node> {
    let (defined_in, local) = match target {
        MountTarget::Local(name) => define(input, file.id, name, 0)?,
        MountTarget::Module(spec) => export_of(input, module_file(file, spec)?, "default", 0)?,
    };
    let owner = input.get(defined_in)?;
    owner
        .extra
        .hosts
        .contains(&local)
        .then_some((defined_in, local))
}

/// The mounts of a run, with the paths of each node remembered so a router under many mounts is walked once.
struct Mounts<'a> {
    parents: &'a BTreeMap<Node, Vec<(Node, String)>>,
    memo: BTreeMap<Node, BTreeSet<String>>,
    /// How many mounts were skipped because they close a cycle.
    cuts: usize,
}

impl Mounts<'_> {
    /// Every path a node is served under: the prefixes of its parents followed by its own mount; the empty path for a
    /// node nothing mounts. A mount that would close a cycle adds no path, so a node reached only through a cycle has
    /// none. Only a walk that met no cycle is remembered, because a cut makes the answer depend on where the walk began.
    fn prefixes_of(&mut self, node: &Node, visiting: &mut Vec<Node>) -> BTreeSet<String> {
        if let Some(known) = self.memo.get(node) {
            return known.clone();
        }
        let parents = self.parents;
        let Some(edges) = parents.get(node) else {
            return BTreeSet::from([String::new()]);
        };
        let cuts_before = self.cuts;
        visiting.push(node.clone());
        let mut prefixes = BTreeSet::new();
        for (parent, mount) in edges {
            if visiting.contains(parent) {
                self.cuts += 1;
                continue;
            }
            for above in self.prefixes_of(parent, visiting) {
                prefixes.insert(join_path(&above, mount));
            }
        }
        visiting.pop();
        if self.cuts == cuts_before {
            self.memo.insert(node.clone(), prefixes.clone());
        }
        prefixes
    }
}

fn join_path(above: &str, below: &str) -> String {
    match (above.is_empty(), below.is_empty()) {
        (true, _) => below.to_owned(),
        (_, true) => above.to_owned(),
        _ => format!(
            "{}/{}",
            above.trim_end_matches('/'),
            below.trim_start_matches('/')
        ),
    }
}

/// The route as it is served under `prefix`.
fn rekey(fact: &ContractFact, prefix: &str) -> ContractFact {
    let key = fact.key.http_parts().map_or_else(
        || fact.key.clone(),
        |(method, path)| contract_key(&ContractKind::Http, Some(prefix), path, Some(method)),
    );
    ContractFact {
        key,
        role: fact.role,
        line: fact.line,
        via: fact.via.clone(),
    }
}

fn trpc(input: &LinkInput<'_, TsExtra>) -> Vec<LinkedFact> {
    let mut routers: BTreeMap<Node, &TrpcRouter> = BTreeMap::new();
    for file in input.files() {
        for router in &file.extra.routers {
            routers.insert((file.id, router.id.clone()), router);
        }
    }
    let mut composed: BTreeSet<Node> = BTreeSet::new();
    for file in input.files() {
        for router in &file.extra.routers {
            for entry in &router.entries {
                if let Some(child) = child_node(input, file, entry) {
                    composed.insert(child);
                }
            }
        }
    }
    let mut linked = Vec::new();
    for node in routers.keys().filter(|node| !composed.contains(*node)) {
        emit(input, &routers, node, "", &mut Vec::new(), &mut linked);
    }
    linked
}

/// The router a property of a router stands for, when it is one.
fn child_node(
    input: &LinkInput<'_, TsExtra>,
    file: &LinkFile<'_, TsExtra>,
    entry: &TrpcEntry,
) -> Option<Node> {
    match entry {
        TrpcEntry::Nested { id, .. } => Some((file.id, id.clone())),
        TrpcEntry::Ref { local, .. } => define(input, file.id, local, 0),
        TrpcEntry::Procedure { .. } => None,
    }
}

/// Emits the procedures of `node` under `prefix`, and of the routers it composes under their property names.
fn emit(
    input: &LinkInput<'_, TsExtra>,
    routers: &BTreeMap<Node, &TrpcRouter>,
    node: &Node,
    prefix: &str,
    path: &mut Vec<Node>,
    linked: &mut Vec<LinkedFact>,
) {
    let (Some(router), Some(file)) = (routers.get(node), input.get(node.0)) else {
        return;
    };
    if path.contains(node) {
        return;
    }
    path.push(node.clone());
    for entry in &router.entries {
        match entry {
            TrpcEntry::Procedure { name, line } => {
                linked.push(procedure_fact(node.0, prefix, name, *line));
            }
            TrpcEntry::Nested { name, .. } => {
                if let Some(child) = child_node(input, file, entry) {
                    emit(
                        input,
                        routers,
                        &child,
                        &join_dot(prefix, name),
                        path,
                        linked,
                    );
                }
            }
            TrpcEntry::Ref { name, line, .. } => {
                let Some(child) = child_node(input, file, entry) else {
                    continue;
                };
                if routers.contains_key(&child) {
                    emit(
                        input,
                        routers,
                        &child,
                        &join_dot(prefix, name),
                        path,
                        linked,
                    );
                } else if is_procedure(input, &child) {
                    linked.push(procedure_fact(node.0, prefix, name, *line));
                }
            }
        }
    }
    path.pop();
}

/// A procedure served at `name` under `prefix`, reported at the line of the entry that lists it.
fn procedure_fact(file: FileId, prefix: &str, name: &str, line: Line) -> LinkedFact {
    LinkedFact {
        file,
        fact: ContractFact {
            key: ContractKey::new(ContractKind::Trpc, &join_dot(prefix, name)),
            role: Role::Serves,
            line,
            via: "trpc".to_owned(),
        },
    }
}

/// Whether `node` is a variable that its file initialises with a procedure call.
fn is_procedure(input: &LinkInput<'_, TsExtra>, node: &Node) -> bool {
    input
        .get(node.0)
        .is_some_and(|file| file.extra.procedures.contains(&node.1))
}

fn join_dot(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}.{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mounts_join_without_doubled_slashes() {
        assert_eq!(join_path("", "/api"), "/api");
        assert_eq!(join_path("/api", ""), "/api");
        assert_eq!(join_path("/api/", "/bookings"), "/api/bookings");
        assert_eq!(join_dot("", "booking"), "booking");
        assert_eq!(join_dot("booking", "get"), "booking.get");
    }
}
