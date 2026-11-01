//! The sites the parse left to the link because a rule needed the value of a constant that another file declares.

use std::collections::BTreeMap;

use gsttt_core::FileId;
use gsttt_core::connector::{LinkFile, LinkInput, LinkedFact};
use gsttt_core::fact::{ContractFact, UnresolvedSite};
use gsttt_core::rules::{CallRule, Outcome, Pack};

use super::{Node, through};
use crate::analyze::serves_http;
use crate::extra::{ClassRoute, Deferred, DeferredTarget, ExportedMaker, Route, RuleRef, TsExtra};
use crate::shape::Shape;

/// What the deferred sites of every file came to, once the constants they name were read.
#[derive(Default)]
pub(super) struct Resolved {
    /// Contracts of the files that declare no host or class for them, which the link finds on its own.
    pub(super) own: Vec<LinkedFact>,
    /// Routes served by a variable, by file, which wait for the prefixes its mounts give it.
    routes: BTreeMap<FileId, Vec<Route>>,
    /// Routes that a decorator declares, by file, which wait for the prefixes of the modules that reach the class.
    class_routes: BTreeMap<FileId, Vec<ClassRoute>>,
    /// Routes that a file serves on an app or router another file makes, by the file that registers the route, the router as its own file knows it, and the route as the rules found it, which wait for the prefixes the mounts of the router give it.
    pub(super) served: Vec<(FileId, Node, ContractFact)>,
    /// The sites whose rule still could not read them.
    pub(super) unread: Vec<(FileId, UnresolvedSite)>,
}

impl Resolved {
    /// The routes of `file` that were left to the link.
    pub(super) fn routes_of(&self, file: FileId) -> &[Route] {
        self.routes.get(&file).map_or(&[], Vec::as_slice)
    }

    /// The class routes of `file` that were left to the link.
    pub(super) fn class_routes_of(&self, file: FileId) -> &[ClassRoute] {
        self.class_routes.get(&file).map_or(&[], Vec::as_slice)
    }
}

/// Reads the constants that the deferred sites of every file name, finds what the files that make a client or a router a site is called on make of it, and applies the rule of each site again.
pub(super) fn resolve(input: &LinkInput<'_, TsExtra>, packs: &[Pack]) -> Resolved {
    let mut resolved = Resolved::default();
    for file in input.files() {
        for deferred in &file.extra.deferred {
            let mut shape = deferred.shape.clone();
            shape.fill(|name| constant(input, file, name));
            let (outcome, served) = match &deferred.target {
                DeferredTarget::Receiver { host } => {
                    match on_made(input, packs, file, deferred, host, &shape) {
                        Some(read) => read,
                        None => continue,
                    }
                }
                // A client that another file makes is read by the rules that take a receiver, so the rule that takes none does not read the call.
                DeferredTarget::Fallback { host, rivals }
                    if made_for(input, packs, file, host, rivals) =>
                {
                    continue;
                }
                _ => match apply(packs, deferred, deferred.prefix.as_deref(), &shape) {
                    Some(outcome) => (outcome, None),
                    None => continue,
                },
            };
            match outcome {
                Outcome::Fact(fact) => match (&deferred.target, served) {
                    (_, Some(node)) => resolved.served.push((file.id, node, fact)),
                    (DeferredTarget::Host(host), None) => {
                        resolved.routes.entry(file.id).or_default().push(Route {
                            host: host.clone(),
                            fact,
                        });
                    }
                    (DeferredTarget::Class { class, inherited }, None) => {
                        resolved
                            .class_routes
                            .entry(file.id)
                            .or_default()
                            .push(ClassRoute {
                                class: class.clone(),
                                fact,
                                inherited: *inherited,
                            });
                    }
                    (
                        DeferredTarget::Own
                        | DeferredTarget::Receiver { .. }
                        | DeferredTarget::Fallback { .. },
                        None,
                    ) => resolved.own.push(LinkedFact::found(file.id, fact)),
                },
                Outcome::Blind(site) => resolved.unread.push((file.id, site)),
                Outcome::Skip => {}
            }
        }
    }
    resolved
}

/// The call rule at `at` and the pack it is in. `None` when the packs have no such rule, which the cache keeps from happening because it is keyed by the packs.
fn call_rule(packs: &[Pack], at: RuleRef) -> Option<(&Pack, &CallRule)> {
    match at {
        RuleRef::Call { pack, rule } => {
            let pack = packs.get(pack)?;
            Some((pack, pack.calls.get(rule)?))
        }
        RuleRef::Decorator { .. } => None,
    }
}

/// What the rule that read `deferred` makes of `shape`, given `prefix`. `None` when the packs have no such rule.
fn apply(
    packs: &[Pack],
    deferred: &Deferred,
    prefix: Option<&str>,
    shape: &Shape,
) -> Option<Outcome> {
    if let Some((pack, rule)) = call_rule(packs, deferred.rule) {
        return Some(rule.apply(&pack.name, shape, prefix, deferred.line, &deferred.what));
    }
    let RuleRef::Decorator { pack, rule } = deferred.rule else {
        return None;
    };
    let pack = packs.get(pack)?;
    Some(
        pack.decorators
            .get(rule)?
            .apply(&pack.name, shape, prefix, deferred.line, &deferred.what),
    )
}

/// The variable that `host`, written in `file`, is imported as, when the file it leads to makes it with a call that some rule takes a receiver from, and the node it is in that file.
fn maker_of<'a>(
    input: &LinkInput<'a, TsExtra>,
    file: &LinkFile<'_, TsExtra>,
    host: &str,
) -> Option<(Node, &'a ExportedMaker)> {
    // A member of the variable (`api.v1`) is not the variable.
    let (node, None) = imported(input, file, host)? else {
        return None;
    };
    let makers = &input.get(node.0)?.extra.makers;
    let found = makers
        .binary_search_by(|maker| maker.name.as_str().cmp(&node.1))
        .ok()?;
    Some((node, makers.get(found)?))
}

/// What the rule of a call on `host` makes of the call when `host`, written in `file`, is a client or a router that another file makes: the prefix is the one the rule takes from the call that made it, read against the constants of the file that made it. The node is the router the route is served on, for a rule that serves routes. `None` when `host` is no such variable, or the rule takes no receiver from the call that made it.
fn on_made(
    input: &LinkInput<'_, TsExtra>,
    packs: &[Pack],
    file: &LinkFile<'_, TsExtra>,
    deferred: &Deferred,
    host: &str,
    shape: &Shape,
) -> Option<(Outcome, Option<Node>)> {
    let (pack, rule) = call_rule(packs, deferred.rule)?;
    let receiver = rule.receiver.as_ref()?;
    let (node, maker) = maker_of(input, file, host)?;
    if !receiver.from.matches(&maker.callee) {
        return None;
    }
    let owner = input.get(node.0)?;
    let mut made = maker.shape.clone();
    made.fill(|name| constant(input, owner, name));
    let prefix = receiver.read(&made);
    let outcome = rule.apply(
        &pack.name,
        shape,
        Some(&prefix),
        deferred.line,
        &deferred.what,
    );
    Some((outcome, serves_http(&rule.kind, rule.role).then_some(node)))
}

/// Whether `host`, written in `file`, is a variable that another file makes with a call that some rule of `rivals` takes a receiver from.
fn made_for(
    input: &LinkInput<'_, TsExtra>,
    packs: &[Pack],
    file: &LinkFile<'_, TsExtra>,
    host: &str,
    rivals: &[RuleRef],
) -> bool {
    let Some((_, maker)) = maker_of(input, file, host) else {
        return false;
    };
    rivals.iter().any(|rival| {
        call_rule(packs, *rival)
            .and_then(|(_, rule)| rule.receiver.as_ref())
            .is_some_and(|receiver| receiver.from.matches(&maker.callee))
    })
}

/// The export that `name`, written in `file`, reaches when its first segment is a name the file imports: the node where it is defined, and the rest of the name below it. A namespace names the export in the segment after it.
fn imported<'n>(
    input: &LinkInput<'_, TsExtra>,
    file: &LinkFile<'_, TsExtra>,
    name: &'n str,
) -> Option<(Node, Option<&'n str>)> {
    let (root, rest) = match name.split_once('.') {
        Some((root, rest)) => (root, Some(rest)),
        None => (name, None),
    };
    let binding = file
        .extra
        .bindings
        .iter()
        .find(|binding| binding.local == root)?;
    let (export, path) = if binding.name == "*" {
        let rest = rest?;
        match rest.split_once('.') {
            Some((export, path)) => (export, Some(path)),
            None => (rest, None),
        }
    } else {
        (binding.name.as_str(), rest)
    };
    Some((through(input, file, binding, export, 0)?, path))
}

/// The text that `name`, written in `file`, stands for in the file it is imported from: `LOGIN_PATH` taken by name, `Env.API_URL` for a member, `ROUTES.users.byId` for a nested property, and `paths.LOGIN_PATH` for a constant taken through a namespace.
fn constant(
    input: &LinkInput<'_, TsExtra>,
    file: &LinkFile<'_, TsExtra>,
    name: &str,
) -> Option<String> {
    let ((defined_in, local), path) = imported(input, file, name)?;
    let constants = &input.get(defined_in)?.extra.constants;
    let key = match path {
        Some(path) => format!("{local}.{path}"),
        None => local,
    };
    let found = constants
        .binary_search_by(|(known, _)| known.as_str().cmp(&key))
        .ok()?;
    constants.get(found).map(|(_, value)| value.clone())
}
