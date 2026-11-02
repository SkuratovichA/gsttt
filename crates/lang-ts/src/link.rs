//! The link phase: what only the whole repository shows, which is how routers and NestJS modules mount under each other across files, which routes a NestJS controller inherits, how tRPC routers compose, the value of a constant that another file declares, and the client or router that another file makes.

mod deferred;

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::FileId;
use gsttt_core::connector::{LinkFile, LinkInput, Linked, LinkedFact};
use gsttt_core::contract::{ContractKey, ContractKind, Role};
use gsttt_core::fact::{ContractFact, Line, Resolution, UnresolvedSite};
use gsttt_core::rules::{Pack, contract_key};

use self::deferred::Resolved;
use crate::extra::{
    Binding, ClassRoute, ExportTarget, MountTarget, TrpcEntry, TrpcRouter, TsExtra,
};

/// How far a name is followed through imports and re-exports before the chain is given up.
const MAX_DEPTH: usize = 16;

/// A name in a file.
type Node = (FileId, String);

/// The facts that need other files: Express routes under the prefixes their routers are mounted at, NestJS controller routes behind the global prefix of the apps that reach them and the routes they inherit from a base class, tRPC procedures under the routers that compose them, and the contracts of the sites whose rule needed the value of an imported constant or the client or router that an imported name stands for. An Express route and a procedure are found only here, so they key no parsed fact again and the engine keeps every fact the parse returned. A NestJS route a controller declares is parsed as written, and a fact keyed again by [`LinkedFact::rekeyed`] takes its place. The sites that a rule still cannot read once the constants are known come back as unread sites of their file.
pub(crate) fn link(input: &LinkInput<'_, TsExtra>, packs: &[Pack]) -> Linked {
    let resolved = deferred::resolve(input, packs);
    let mut unread = Vec::new();
    let edges = parents(input, &mut unread);
    let mut mounts = Mounts::new(&edges);
    let mut facts = express(input, &resolved, &mut mounts);
    facts.extend(nestjs(input, &resolved, &mut mounts));
    facts.extend(trpc(input));
    facts.extend(resolved.own);
    if mounts.cuts > 0 {
        tracing::debug!(
            cuts = mounts.cuts,
            "mounts close a cycle, which adds no path of its own"
        );
    }
    unread.extend(resolved.unread);
    Linked { facts, unread }
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
    through(input, here, binding, &binding.name, depth + 1)
}

/// Where `export` is defined in the module that `binding`, one of the imports of `file`, leads to: the step from an import to the export it reaches, which `define` and `imported` of the deferred sites share.
fn through(
    input: &LinkInput<'_, TsExtra>,
    file: &LinkFile<'_, TsExtra>,
    binding: &Binding,
    export: &str,
    depth: usize,
) -> Option<Node> {
    export_of(input, module_file(file, &binding.spec)?, export, depth)
}

/// What `file` exports as `name`, as the name it has where it is defined. `export * from` forwards every name but `default`.
///
/// The impact walk follows the same stars in `X(file)` (`crates/core/src/query/walk/exports.rs`). The two cannot share code, because the core knows no language, so the test `a_star_does_not_forward_default_to_a_mount` pins them against each other on one barrel.
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
    if name == "default" {
        return None;
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

/// For every node that some mount attaches, the nodes it is attached to and the path of each mount: Express routers, NestJS modules and controllers, and the roots of the apps with a global prefix.
///
/// A `.use` on a name imported from another file is a mount only when that name is a variable made by a call a rule serves routes on. A computed prefix there is a site the file could not read, added to `unread`.
fn parents(
    input: &LinkInput<'_, TsExtra>,
    unread: &mut Vec<(FileId, UnresolvedSite)>,
) -> BTreeMap<Node, Vec<(Node, String)>> {
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
        for mount in &file.extra.imported_mounts {
            let Some((host, _)) = deferred::maker_of(input, file, &mount.host) else {
                continue;
            };
            if !input
                .get(host.0)
                .is_some_and(|owner| owner.extra.hosts.contains(&host.1))
            {
                continue;
            }
            if mount.computed {
                unread.push((
                    file.id,
                    UnresolvedSite::non_literal(mount.line, &mount.detail),
                ));
            }
            for target in &mount.targets {
                if let Some(child) = mounted_host(input, file, target) {
                    for prefix in &mount.prefixes {
                        parents
                            .entry(child.clone())
                            .or_default()
                            .push((host.clone(), prefix.clone()));
                    }
                }
            }
        }
    }
    parents
}

fn express(
    input: &LinkInput<'_, TsExtra>,
    resolved: &Resolved,
    mounts: &mut Mounts<'_>,
) -> Vec<LinkedFact> {
    let mut linked = Vec::new();
    for file in input.files() {
        for route in file.extra.routes.iter().chain(resolved.routes_of(file.id)) {
            let node = (file.id, route.host.clone());
            for prefix in mounts.served_under(&node) {
                linked.push(LinkedFact::found(file.id, rekey(&route.fact, &prefix)));
            }
        }
    }
    // A route on a router another file makes is served under the mounts of that router, and stays a site of the file that registers it.
    for (file, node, fact) in &resolved.served {
        for prefix in mounts.served_under(node) {
            linked.push(LinkedFact::found(*file, rekey(fact, &prefix)));
        }
    }
    linked
}

/// The routes of NestJS controllers behind the global prefix of the apps that reach them, and the routes a controller inherits from the base class it extends.
///
/// A route a controller declares is already parsed, so it is keyed again once for every path the controller is served under, the empty one included when some module reaches it without a prefix; a controller that no app prefixes keeps the fact as parsed. A route of a class without the decorator was not parsed, so it is found here, in the file of the base class and in the file of each controller that extends it. A route whose path is an imported constant was not parsed either, and is found here like a base class route: behind every prefix its controller is served under, or as written when no app prefixes it.
fn nestjs(
    input: &LinkInput<'_, TsExtra>,
    resolved: &Resolved,
    mounts: &mut Mounts<'_>,
) -> Vec<LinkedFact> {
    let mut linked = Vec::new();
    for file in input.files() {
        for route in file
            .extra
            .class_routes
            .iter()
            .filter(|route| !route.inherited)
        {
            let prefixes = mounts.served_under(&(file.id, route.class.clone()));
            if prefixes.iter().all(String::is_empty) {
                continue;
            }
            for prefix in prefixes {
                let key = rekey(&route.fact, &prefix).key;
                linked.push(LinkedFact::rekeyed(file.id, &route.fact, key));
            }
        }
        // A route read here was never parsed, so no own fact stands in its place: every key it has is found, and a controller that no app prefixes serves it as written.
        for route in resolved
            .class_routes_of(file.id)
            .iter()
            .filter(|route| !route.inherited)
        {
            let prefixes = mounts.served_under(&(file.id, route.class.clone()));
            if prefixes.iter().all(String::is_empty) {
                linked.push(LinkedFact::found(file.id, route.fact.clone()));
                continue;
            }
            for prefix in prefixes {
                linked.push(LinkedFact::found(file.id, rekey(&route.fact, &prefix)));
            }
        }
        for inherit in file
            .extra
            .inherits
            .iter()
            .chain(resolved.inherits_of(file.id))
        {
            let Some((base_file, base)) = define(input, file.id, &inherit.base, 0) else {
                continue;
            };
            let Some(owner) = input.get(base_file) else {
                continue;
            };
            let inherited: Vec<&ClassRoute> = owner
                .extra
                .class_routes
                .iter()
                .chain(resolved.class_routes_of(owner.id))
                .filter(|route| route.inherited && route.class == base)
                .collect();
            for prefix in mounts.served_under(&(file.id, inherit.class.clone())) {
                let full = join_path(&prefix, &inherit.prefix);
                for route in &inherited {
                    let fact = rekey(&route.fact, &full);
                    let at_controller = ContractFact {
                        line: inherit.line,
                        ..fact.clone()
                    };
                    linked.push(LinkedFact::found(base_file, fact));
                    linked.push(LinkedFact::found(file.id, at_controller));
                }
            }
        }
    }
    // Two controllers that extend one base class under the same path, and two prefixes that key alike, report a site once.
    LinkedFact::sort_unique(&mut linked);
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
///
/// Nodes that mount each other in a ring are one component, found before any walk. The rings of NestJS are made of `@Module` mounts, which carry no path, so every node of a ring is served under the paths of the mounts that lead into the ring from outside, and the ring is read once. A ring with a path on a mount of its own, which only Express routers mounted under paths can make, gives a node different paths depending on where a walk enters it, so that component is walked path by path.
struct Mounts<'a> {
    parents: &'a BTreeMap<Node, Vec<(Node, String)>>,
    /// The component of every node that has parents, as an index into `components`.
    component_of: BTreeMap<&'a Node, usize>,
    components: Vec<Component<'a>>,
    memo: BTreeMap<Node, BTreeSet<String>>,
    /// How many mounts lie inside a ring. They add no path of their own, so this only reports how much of the graph is cyclic.
    cuts: usize,
    /// How many mounts the walk path by path skipped because they close a cycle, which tells it whether the paths it found may be remembered.
    walk_cuts: usize,
}

/// A set of nodes that reach each other through mounts, or one node alone.
struct Component<'a> {
    nodes: Vec<&'a Node>,
    /// A mount between two of its nodes has a path, so a path through it depends on where the walk entered.
    walked: bool,
}

impl<'a> Mounts<'a> {
    fn new(parents: &'a BTreeMap<Node, Vec<(Node, String)>>) -> Self {
        let nodes: Vec<&Node> = parents.keys().collect();
        let index: BTreeMap<&Node, usize> = nodes
            .iter()
            .enumerate()
            .map(|(at, node)| (*node, at))
            .collect();
        // A node that nothing is mounted under is on no ring, so a mount into it is left out.
        let leads_to: Vec<Vec<usize>> = parents
            .values()
            .map(|above| {
                above
                    .iter()
                    .filter_map(|(parent, _)| index.get(parent).copied())
                    .collect()
            })
            .collect();
        let mut components: Vec<Component<'a>> = Vec::new();
        let mut component_of = BTreeMap::new();
        for (node, id) in nodes.into_iter().zip(strongly_connected(&leads_to)) {
            if components.len() <= id {
                components.resize_with(id + 1, || Component {
                    nodes: Vec::new(),
                    walked: false,
                });
            }
            if let Some(component) = components.get_mut(id) {
                component.nodes.push(node);
            }
            component_of.insert(node, id);
        }
        let mut cuts = 0;
        for (node, above) in parents {
            let Some(&id) = component_of.get(node) else {
                continue;
            };
            for (parent, mount) in above {
                if component_of.get(parent) != Some(&id) {
                    continue;
                }
                cuts += 1;
                // A mount of a node under itself is never part of a simple path.
                if parent != node
                    && !mount.is_empty()
                    && let Some(component) = components.get_mut(id)
                {
                    component.walked = true;
                }
            }
        }
        Self {
            parents,
            component_of,
            components,
            memo: BTreeMap::new(),
            cuts,
            walk_cuts: 0,
        }
    }

    /// Every path a node is served under: the paths of its parents followed by its own mount; the empty path for a node nothing mounts. A mount that would close a cycle adds no path, so a node reached only through a cycle has none.
    fn prefixes_of(&mut self, node: &Node) -> BTreeSet<String> {
        if let Some(known) = self.memo.get(node) {
            return known.clone();
        }
        let Some((id, component)) = self
            .component_of
            .get(node)
            .and_then(|&id| Some((id, self.components.get(id)?)))
        else {
            return BTreeSet::from([String::new()]);
        };
        if component.walked {
            return self.walk_simple_paths(node, &mut Vec::new());
        }
        let parents = self.parents;
        let members = component.nodes.clone();
        let exits: Vec<(&Node, &str)> = members
            .iter()
            .filter_map(|member| parents.get(*member))
            .flatten()
            .filter(|(parent, _)| self.component_of.get(parent) != Some(&id))
            .map(|(parent, mount)| (parent, mount.as_str()))
            .collect();
        let mut prefixes = BTreeSet::new();
        for (parent, mount) in exits {
            for above in self.prefixes_of(parent) {
                prefixes.insert(join_path(&above, mount));
            }
        }
        for member in members {
            self.memo.insert(member.clone(), prefixes.clone());
        }
        prefixes
    }

    /// [`Mounts::prefixes_of`], with the empty path for a node that only a ring reaches, which is served where it is written.
    fn served_under(&mut self, node: &Node) -> BTreeSet<String> {
        let mut prefixes = self.prefixes_of(node);
        if prefixes.is_empty() {
            prefixes.insert(String::new());
        }
        prefixes
    }

    /// The paths of a node by following every simple path up through its parents, skipping a parent already on the stack. Only a walk that met no cycle is remembered, because a cut makes the answer depend on where the walk began. It costs the number of simple paths, so it serves only the components whose mounts have paths.
    fn walk_simple_paths(&mut self, node: &Node, visiting: &mut Vec<Node>) -> BTreeSet<String> {
        if let Some(known) = self.memo.get(node) {
            return known.clone();
        }
        let parents = self.parents;
        let Some(edges) = parents.get(node) else {
            return BTreeSet::from([String::new()]);
        };
        let cuts_before = self.walk_cuts;
        visiting.push(node.clone());
        let mut prefixes = BTreeSet::new();
        for (parent, mount) in edges {
            if visiting.contains(parent) {
                self.walk_cuts += 1;
                continue;
            }
            for above in self.walk_simple_paths(parent, visiting) {
                prefixes.insert(join_path(&above, mount));
            }
        }
        visiting.pop();
        if self.walk_cuts == cuts_before {
            self.memo.insert(node.clone(), prefixes.clone());
        }
        prefixes
    }
}

/// What the walk of [`strongly_connected`] knows of a node.
#[derive(Clone, Copy)]
struct Slot {
    /// When the walk first reached the node, `UNSEEN` before it did.
    index: usize,
    /// The lowest `index` that the walk below the node has reached on the stack.
    low: usize,
    on_stack: bool,
    component: usize,
}

/// The `index` of a node the walk has not reached.
const UNSEEN: usize = usize::MAX;

/// The strongly connected component of every node, numbered from 0, of the graph where `leads_to[n]` lists the nodes that `n` has an edge to. Tarjan's algorithm with a stack of its own in place of recursion, so a long chain of modules costs memory and not the call stack.
fn strongly_connected(leads_to: &[Vec<usize>]) -> Vec<usize> {
    let mut walk = Tarjan {
        leads_to,
        slots: vec![
            Slot {
                index: UNSEEN,
                low: 0,
                on_stack: false,
                component: UNSEEN,
            };
            leads_to.len()
        ],
        open: Vec::new(),
        order: 0,
        components: 0,
    };
    for root in 0..leads_to.len() {
        if walk
            .slots
            .get(root)
            .is_some_and(|slot| slot.index == UNSEEN)
        {
            walk.from(root);
        }
    }
    walk.slots.into_iter().map(|slot| slot.component).collect()
}

struct Tarjan<'g> {
    leads_to: &'g [Vec<usize>],
    slots: Vec<Slot>,
    /// The nodes reached whose component is not closed yet.
    open: Vec<usize>,
    order: usize,
    components: usize,
}

impl Tarjan<'_> {
    /// Walks everything `root` reaches that is not reached yet.
    fn from(&mut self, root: usize) {
        // A node and the position of the next edge to follow from it.
        let mut calls = vec![(root, 0)];
        self.enter(root);
        while let Some(&(node, at)) = calls.last() {
            let Some(&to) = self.leads_to.get(node).and_then(|edges| edges.get(at)) else {
                calls.pop();
                self.leave(node, calls.last().map(|&(caller, _)| caller));
                continue;
            };
            if let Some(call) = calls.last_mut() {
                call.1 += 1;
            }
            match self.slots.get(to).copied() {
                Some(Slot { index: UNSEEN, .. }) => {
                    self.enter(to);
                    calls.push((to, 0));
                }
                Some(Slot {
                    index,
                    on_stack: true,
                    ..
                }) => self.lower(node, index),
                _ => {}
            }
        }
    }

    fn enter(&mut self, node: usize) {
        if let Some(slot) = self.slots.get_mut(node) {
            *slot = Slot {
                index: self.order,
                low: self.order,
                on_stack: true,
                component: UNSEEN,
            };
        }
        self.order += 1;
        self.open.push(node);
    }

    fn lower(&mut self, node: usize, to: usize) {
        if let Some(slot) = self.slots.get_mut(node) {
            slot.low = slot.low.min(to);
        }
    }

    /// `node` has no edge left to follow: when nothing below it reaches above it, it closes a component, and its caller learns how high the walk below it went.
    fn leave(&mut self, node: usize, caller: Option<usize>) {
        let Some(Slot { index, low, .. }) = self.slots.get(node).copied() else {
            return;
        };
        if low == index {
            while let Some(member) = self.open.pop() {
                if let Some(slot) = self.slots.get_mut(member) {
                    slot.on_stack = false;
                    slot.component = self.components;
                }
                if member == node {
                    break;
                }
            }
            self.components += 1;
        }
        if let Some(caller) = caller {
            self.lower(caller, low);
        }
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
    LinkedFact::found(
        file,
        ContractFact {
            key: ContractKey::new(ContractKind::Trpc, &join_dot(prefix, name)),
            role: Role::Serves,
            line,
            via: "trpc".to_owned(),
        },
    )
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
    use proptest::prelude::*;

    use super::*;

    fn node(name: &str) -> Node {
        (FileId::from_raw(0), name.to_owned())
    }

    /// Records that `parent` mounts `child` under `path`.
    fn mount(
        parents: &mut BTreeMap<Node, Vec<(Node, String)>>,
        child: &str,
        parent: &str,
        path: &str,
    ) {
        parents
            .entry(node(child))
            .or_default()
            .push((node(parent), path.to_owned()));
    }

    #[test]
    fn a_ring_of_modules_that_import_each_other_is_read_once() {
        const RING: usize = 24;
        let mut parents = BTreeMap::new();
        mount(&mut parents, "m0", "app", "api");
        for child in 0..RING {
            mount(&mut parents, &format!("c{child}"), &format!("m{child}"), "");
            for parent in (0..RING).filter(|parent| *parent != child) {
                mount(
                    &mut parents,
                    &format!("m{child}"),
                    &format!("m{parent}"),
                    "",
                );
            }
        }
        let mut mounts = Mounts::new(&parents);
        let api = BTreeSet::from(["api".to_owned()]);
        for at in 0..RING {
            assert_eq!(mounts.prefixes_of(&node(&format!("m{at}"))), api);
            assert_eq!(mounts.prefixes_of(&node(&format!("c{at}"))), api);
        }
        assert_eq!(mounts.cuts, RING * (RING - 1));
    }

    #[test]
    fn a_ring_that_nothing_outside_mounts_has_no_path() {
        let mut parents = BTreeMap::new();
        mount(&mut parents, "a", "b", "");
        mount(&mut parents, "b", "a", "");
        mount(&mut parents, "c", "c", "x");
        let mut mounts = Mounts::new(&parents);
        assert!(mounts.prefixes_of(&node("a")).is_empty());
        assert_eq!(
            mounts.served_under(&node("a")),
            BTreeSet::from([String::new()])
        );
        assert!(mounts.prefixes_of(&node("c")).is_empty());
    }

    proptest! {
        #[test]
        fn components_give_the_paths_of_the_walk_of_every_simple_path(
            edges in prop::collection::vec(
                (0..7usize, 0..7usize, prop::sample::select(vec!["", "a", "b"])),
                0..16,
            ),
        ) {
            let mut parents = BTreeMap::new();
            for (child, parent, path) in &edges {
                mount(&mut parents, &format!("n{child}"), &format!("n{parent}"), path);
            }
            let mut by_components = Mounts::new(&parents);
            let mut by_paths = Mounts::new(&parents);
            for at in 0..7 {
                let at = node(&format!("n{at}"));
                prop_assert_eq!(
                    by_components.prefixes_of(&at),
                    by_paths.walk_simple_paths(&at, &mut Vec::new())
                );
            }
        }
    }

    #[test]
    fn mounts_join_without_doubled_slashes() {
        assert_eq!(join_path("", "/api"), "/api");
        assert_eq!(join_path("/api", ""), "/api");
        assert_eq!(join_path("/api/", "/bookings"), "/api/bookings");
        assert_eq!(join_dot("", "booking"), "booking");
        assert_eq!(join_dot("booking", "get"), "booking.get");
    }
}
