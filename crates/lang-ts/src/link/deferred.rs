//! The sites the parse left to the link: those whose rule needed the value of a constant that another file declares, and the calls on a client or a router that another file makes, which are read on what that file makes under the name, or on what this file makes with a name that another file declares.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use gsttt_core::FileId;
use gsttt_core::connector::{LinkFile, LinkInput, LinkedFact};
use gsttt_core::fact::{ContractFact, UnresolvedSite};
use gsttt_core::rules::{CallRule, Outcome, Pack, serves_http};

use super::{Node, through};
use crate::extra::{ClassRoute, Deferred, DeferredTarget, LinkMaker, Route, RuleRef, TsExtra};
use crate::shape::Shape;

/// What the deferred sites of every file came to, once the constants they name were read and the clients and routers they are called on were found.
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
    let reader = Reader::new(input);
    for file in input.files() {
        for deferred in &file.extra.deferred {
            let mut shape = deferred.shape.clone();
            shape.fill(|name| reader.constant(file, name, None));
            let (outcome, served) = match &deferred.target {
                DeferredTarget::Receiver { host } => {
                    match on_made(&reader, packs, file, deferred, host, &shape) {
                        Some(read) => read,
                        None => continue,
                    }
                }
                DeferredTarget::Made { host } => {
                    match on_own(&reader, packs, file, deferred, host, &shape) {
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
                        | DeferredTarget::Made { .. }
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
        return Some(rule.apply(
            &pack.name,
            pack.relative_without_base,
            shape,
            prefix,
            deferred.line,
            &deferred.what,
        ));
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
) -> Option<(Node, &'a LinkMaker)> {
    // A member of the variable (`api.v1`) is not the variable.
    let (node, None) = imported(input, file, host)? else {
        return None;
    };
    let maker = made_in(input.get(node.0)?.extra, &node.1)?;
    Some((node, maker))
}

/// The variable `name`, which the file that `extra` is of makes with a call that some rule takes a receiver from.
fn made_in<'a>(extra: &'a TsExtra, name: &str) -> Option<&'a LinkMaker> {
    let found = extra
        .makers
        .binary_search_by(|maker| maker.name.as_str().cmp(name))
        .ok()?;
    extra.makers.get(found)
}

/// What the rule of a call on `host` makes of the call when `host`, written in `file`, is a client or a router that another file makes: the prefix is the one the rule takes from the call that made it, read against the constants of the file that made it, and the call is a blind spot when a rule that serves routes finds a prefix there that is not text. The node is the router the route is served on, for a rule that serves routes. `None` when `host` is no such variable, or the rule takes no receiver from the call that made it.
fn on_made(
    reader: &Reader<'_, '_>,
    packs: &[Pack],
    file: &LinkFile<'_, TsExtra>,
    deferred: &Deferred,
    host: &str,
    shape: &Shape,
) -> Option<(Outcome, Option<Node>)> {
    let (node, maker) = maker_of(reader.input, file, host)?;
    let owner = reader.input.get(node.0)?;
    on_maker(reader, packs, owner, node, maker, deferred, shape)
}

/// What the rule of a call on `host` makes of the call when `host` is a client or a router that `file` makes itself, with a call that names something another file declares. `None` when the file keeps no such variable, or the rule takes no receiver from the call that made it.
fn on_own(
    reader: &Reader<'_, '_>,
    packs: &[Pack],
    file: &LinkFile<'_, TsExtra>,
    deferred: &Deferred,
    host: &str,
    shape: &Shape,
) -> Option<(Outcome, Option<Node>)> {
    let maker = made_in(file.extra, host)?;
    on_maker(
        reader,
        packs,
        file,
        (file.id, host.to_owned()),
        maker,
        deferred,
        shape,
    )
}

/// What the rule of `deferred` makes of the call `shape` on the variable `maker` stands for, which `owner` declares as `node`.
fn on_maker(
    reader: &Reader<'_, '_>,
    packs: &[Pack],
    owner: &LinkFile<'_, TsExtra>,
    node: Node,
    maker: &LinkMaker,
    deferred: &Deferred,
    shape: &Shape,
) -> Option<(Outcome, Option<Node>)> {
    let (pack, rule) = call_rule(packs, deferred.rule)?;
    let receiver = rule.receiver.as_ref()?;
    if !receiver.from.matches(&maker.callee) {
        return None;
    }
    let mut made = maker.shape.clone();
    made.fill(|name| reader.constant(owner, name, None));
    let reading = receiver.reading(&made);
    let Ok(prefix) = rule.behind(&reading) else {
        return Some((Outcome::blind(deferred.line, &deferred.what), None));
    };
    let outcome = rule.apply(
        &pack.name,
        pack.relative_without_base,
        shape,
        prefix,
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

/// The constants a read is inside of, innermost first, so that a constant which comes back to itself through the names it is built from is cut.
struct Inside<'r> {
    constant: &'r Node,
    outer: Option<&'r Inside<'r>>,
}

impl Inside<'_> {
    fn contains(&self, node: &Node) -> bool {
        self.constant == node || self.outer.is_some_and(|outer| outer.contains(node))
    }
}

/// What the link reads of the constants of the files, and the text each constant read as once it was read. A constant that many others name is read once, not once for every way to it: a stored constant reads as text only when everything it names does, so a constant that names one on the way to itself reads as none whichever of them the read starts from, and what a constant reads as does not depend on the constants it is read inside of.
struct Reader<'a, 'i> {
    input: &'a LinkInput<'i, TsExtra>,
    read: RefCell<HashMap<Node, Option<String>>>,
}

impl<'a, 'i> Reader<'a, 'i> {
    fn new(input: &'a LinkInput<'i, TsExtra>) -> Self {
        Self {
            input,
            read: RefCell::default(),
        }
    }

    /// The text that `name`, written in `file`, stands for in the file it is imported from: `LOGIN_PATH` taken by name, `Env.API_URL` for a member, `ROUTES.users.byId` for a nested property, and `paths.LOGIN_PATH` for a constant taken through a namespace. A constant built from names that other files declare is read in the file that declares it, inside the constants in `inside`, and only when it reads whole: one that comes back to itself has no text.
    fn constant(
        &self,
        file: &LinkFile<'_, TsExtra>,
        name: &str,
        inside: Option<&Inside<'_>>,
    ) -> Option<String> {
        let ((defined_in, local), path) = imported(self.input, file, name)?;
        let key = match path {
            Some(path) => format!("{local}.{path}"),
            None => local,
        };
        let node = (defined_in, key);
        if inside.is_some_and(|inside| inside.contains(&node)) {
            return None;
        }
        if let Some(read) = self.read.borrow().get(&node) {
            return read.clone();
        }
        let owner = self.input.get(defined_in)?;
        let constants = &owner.extra.constants;
        let found = constants
            .binary_search_by(|(known, _)| known.as_str().cmp(&node.1))
            .ok()?;
        let (_, value) = constants.get(found)?;
        let here = Inside {
            constant: &node,
            outer: inside,
        };
        let read = value.read(&|name| self.constant(owner, name, Some(&here)));
        self.read.borrow_mut().insert(node, read.clone());
        read
    }
}
