//! The cross-file half of routing. `parse` records, for every file, which route facts sit under which router
//! variable and which routers the file mounts under a prefix; here the mounts are followed from each route up to the
//! router nobody mounts, and every way up gives the route once more, with the prefixes in front of its path.
//!
//! The route's own fact stays in its file: linked facts are added to the facts a file already has.

use std::collections::BTreeMap;

use gsttt_core::connector::{LinkFile, LinkInput, LinkedFact};
use gsttt_core::contract::{ContractKey, ContractKind, Role};
use gsttt_core::fact::{ContractFact, ImportKind, Resolution};
use gsttt_core::ids::FileId;
use gsttt_core::path::RepoPath;

use crate::extra::{Mount, PyExtra, Target};
use crate::site::join_prefixes;

/// How many mounts are followed upward from a route, so that routers mounted into each other in a circle end.
const MAX_DEPTH: usize = 16;

/// A router: the file it lives in and the variable that holds it. Django's `urlpatterns` is the module itself and
/// has the empty name.
type Router<'a> = (FileId, &'a str);

/// A mount seen from the router it mounts.
struct Parent<'a> {
    /// The router the mount is called on: `app` in `app.include_router(…)`.
    router: Router<'a>,
    prefix: &'a str,
}

/// The route facts of the project once more for each way their router is mounted, behind the mount prefixes.
pub(crate) fn link<'a>(input: &LinkInput<'a, PyExtra>) -> Vec<LinkedFact> {
    let mut parents: BTreeMap<Router<'a>, Vec<Parent<'a>>> = BTreeMap::new();
    for file in input.files().iter().copied() {
        for mount in &file.extra.mounts {
            if let Some(child) = mounted(input, file, mount) {
                parents.entry(child).or_default().push(Parent {
                    router: (file.id, mount.receiver.as_str()),
                    prefix: mount.prefix.as_str(),
                });
            }
        }
    }

    let mut linked = Vec::new();
    for file in input.files().iter().copied() {
        for route in &file.extra.routes {
            let mut prefixes = Vec::new();
            climb(
                &parents,
                (file.id, route.var.as_str()),
                &mut Vec::new(),
                &mut prefixes,
            );
            let serves = file
                .facts
                .contracts
                .iter()
                .filter(|fact| fact.line == route.line && matches!(fact.role, Role::Serves));
            for fact in serves {
                let Some((method, path)) = fact.key.http_parts() else {
                    continue;
                };
                for prefix in &prefixes {
                    let path = join_prefixes(&ContractKind::Http, prefix, path);
                    linked.push(LinkedFact {
                        file: file.id,
                        fact: ContractFact {
                            key: ContractKey::http(method, &path),
                            role: fact.role,
                            line: fact.line,
                            via: fact.via.clone(),
                        },
                    });
                }
            }
        }
    }
    // Two routes of one file can be reached the same way, and the engine only sorts by file.
    linked.sort_by(|a, b| {
        (a.file, a.fact.line, a.fact.key.key()).cmp(&(b.file, b.fact.line, b.fact.key.key()))
    });
    linked.dedup();
    linked
}

/// Every prefix `router` is served under: the prefixes of the mounts between it and a router nobody mounts, outermost
/// first. `trail` holds the prefixes passed so far, innermost first. A router that is not mounted has no prefix to
/// add, so it gives none.
fn climb<'a>(
    parents: &BTreeMap<Router<'a>, Vec<Parent<'a>>>,
    router: Router<'a>,
    trail: &mut Vec<&'a str>,
    found: &mut Vec<String>,
) {
    let Some(mounts) = parents.get(&router) else {
        if !trail.is_empty() {
            let joined = trail.iter().rev().fold(String::new(), |outer, inner| {
                join_prefixes(&ContractKind::Http, &outer, inner)
            });
            found.push(joined);
        }
        return;
    };
    if trail.len() >= MAX_DEPTH {
        return;
    }
    for parent in mounts {
        trail.push(parent.prefix);
        climb(parents, parent.router, trail, found);
        trail.pop();
    }
}

/// The router `mount` of `file` mounts, when it can be found.
fn mounted<'a>(
    input: &LinkInput<'a, PyExtra>,
    file: LinkFile<'a, PyExtra>,
    mount: &'a Mount,
) -> Option<Router<'a>> {
    match &mount.target {
        Target::Module(spec) => {
            let import = file.imports.iter().find(|resolved| {
                resolved.import.kind == ImportKind::Dynamic
                    && resolved.import.line == mount.line
                    && resolved.import.spec == *spec
            })?;
            match import.to {
                Resolution::File { file } => Some((file, "")),
                _ => None,
            }
        }
        Target::Name(name) => named(input, file, name),
    }
}

/// The router an expression names in `file`: a variable of the file itself, or a name its imports bind.
fn named<'a>(
    input: &LinkInput<'a, PyExtra>,
    file: LinkFile<'a, PyExtra>,
    expr: &'a str,
) -> Option<Router<'a>> {
    let (head, rest) = expr.split_once('.').unwrap_or((expr, ""));
    let Some(bind) = file
        .extra
        .binds
        .iter()
        .rev()
        .find(|bind| bind.local == head)
    else {
        return Some((file.id, expr));
    };
    let resolved = file
        .imports
        .iter()
        .find(|resolved| resolved.import.line == bind.line && resolved.import.spec == bind.spec)?;
    let files = resolved.to.files();
    let Some(name) = bind.name.as_deref() else {
        // `import app.api.bookings as bookings`: the alias is the module, and the router is inside it.
        return match files {
            [module] if !rest.is_empty() => Some((*module, rest)),
            _ => None,
        };
    };
    // `from app.api import bookings`: a name that is a module of the resolution is that module, and the router is
    // inside it; any other name is a router that the module itself holds.
    let submodule = files.iter().copied().find(|id| {
        input
            .get(*id)
            .is_some_and(|candidate| module_name(candidate.path) == name)
    });
    match (submodule, files) {
        (Some(module), _) if !rest.is_empty() => Some((module, rest)),
        (None, [module]) if rest.is_empty() => Some((*module, name)),
        _ => None,
    }
}

/// The module a file answers to: `bookings` for `bookings.py` and for `bookings/__init__.py`.
fn module_name(path: &RepoPath) -> &str {
    let file = path.file_name();
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    if stem == "__init__" {
        path.parent().rsplit('/').next().unwrap_or_default()
    } else {
        stem
    }
}
