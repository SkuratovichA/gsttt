//! The sites the parse left to the link because a rule needed the value of a constant that another file declares.

use std::collections::BTreeMap;

use gsttt_core::FileId;
use gsttt_core::connector::{LinkFile, LinkInput, LinkedFact};
use gsttt_core::fact::UnresolvedSite;
use gsttt_core::rules::{Outcome, Pack};

use super::{export_of, module_file};
use crate::extra::{ClassRoute, Deferred, DeferredTarget, Route, RuleRef, TsExtra};
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

/// Reads the constants that the deferred sites of every file name, and applies the rule of each site again.
pub(super) fn resolve(input: &LinkInput<'_, TsExtra>, packs: &[Pack]) -> Resolved {
    let mut resolved = Resolved::default();
    for file in input.files() {
        for deferred in &file.extra.deferred {
            let mut shape = deferred.shape.clone();
            shape.fill(|name| constant(input, file, name));
            let Some(outcome) = apply(packs, deferred, &shape) else {
                continue;
            };
            match outcome {
                Outcome::Fact(fact) => match &deferred.target {
                    DeferredTarget::Own => resolved.own.push(LinkedFact::found(file.id, fact)),
                    DeferredTarget::Host(host) => {
                        resolved.routes.entry(file.id).or_default().push(Route {
                            host: host.clone(),
                            fact,
                        });
                    }
                    DeferredTarget::Class { class, inherited } => {
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
                },
                Outcome::Blind(site) => resolved.unread.push((file.id, site)),
                Outcome::Skip => {}
            }
        }
    }
    resolved
}

/// What the rule that read `deferred` makes of `shape`. `None` when the packs have no such rule, which the cache keeps from happening because it is keyed by the packs.
fn apply(packs: &[Pack], deferred: &Deferred, shape: &Shape) -> Option<Outcome> {
    let prefix = deferred.prefix.as_deref();
    match deferred.rule {
        RuleRef::Call { pack, rule } => {
            let pack = packs.get(pack)?;
            Some(pack.calls.get(rule)?.apply(
                &pack.name,
                shape,
                prefix,
                deferred.line,
                &deferred.what,
            ))
        }
        RuleRef::Decorator { pack, rule } => {
            let pack = packs.get(pack)?;
            Some(pack.decorators.get(rule)?.apply(
                &pack.name,
                shape,
                prefix,
                deferred.line,
                &deferred.what,
            ))
        }
    }
}

/// The text that `name`, written in `file`, stands for in the file it is imported from: `LOGIN_PATH` taken by name, `Env.API_URL` for a member, `ROUTES.users.byId` for a nested property, and `paths.LOGIN_PATH` for a constant taken through a namespace.
fn constant(
    input: &LinkInput<'_, TsExtra>,
    file: &LinkFile<'_, TsExtra>,
    name: &str,
) -> Option<String> {
    let (root, rest) = match name.split_once('.') {
        Some((root, rest)) => (root, Some(rest)),
        None => (name, None),
    };
    let binding = file
        .extra
        .bindings
        .iter()
        .find(|binding| binding.local == root)?;
    let target = module_file(file, &binding.spec)?;
    // A namespace names the export in the segment after it.
    let (export, path) = if binding.name == "*" {
        let rest = rest?;
        match rest.split_once('.') {
            Some((export, path)) => (export, Some(path)),
            None => (rest, None),
        }
    } else {
        (binding.name.as_str(), rest)
    };
    let (defined_in, local) = export_of(input, target, export, 0)?;
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
