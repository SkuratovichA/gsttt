//! What a Python project says it finds and what it cannot see, for the coverage lines of an answer. The kinds come
//! from the packs in force; the blind spots are written per pack in words an answer can quote, so a project without
//! Flask is never said to be blind to Flask routes.

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::connector::Provide;
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::rules::Pack;

/// A router or blueprint whose variable is made in one file and used in another is not followed from a decorator.
const ROUTERS_ELSEWHERE: &str =
    "routers and blueprints created in another module than the one that declares their routes";

/// `link` sees one project at a time and follows each router up through the mounts it can find. A router that a file
/// of another project mounts is served behind its own prefix alone, and one whose every mount it cannot follow to the
/// end is left out. A link cannot report the sites it did not read, so each way a chain of mounts is cut is named here,
/// in one line; the limits are the ones `link` applies.
const MOUNTS_UNFOLLOWED: &str = "mounts of a router by a file of another Python project, in a cycle, deeper than 32 routers, in more than 64 ways, with a prefix held in a variable, through an import that cannot be resolved, or through a name that a module other than a package's __init__.py re-exports";

/// What Starlette's `mount` does with an application, which FastAPI inherits: no pack reads it.
const APPLICATIONS_MOUNTED: &str =
    "applications mounted under a path with mount, as Starlette does";

/// A file is read as one scope, so a name two imports bind in turn stands for the last of them. A fallback import,
/// `try: import httpx as http` and `except ImportError: import requests as http`, is the case that matters for clients.
const CLIENTS_REBOUND: &str = "a client library imported under one name in try and in except, of which the last import is read";

/// A callee is matched as written and as the imports of its file spell it, and `from module import *` lists no name, so
/// a function it brings in is matched as the bare name it is written as, which no pack names.
const STAR_IMPORTS: &str =
    "names brought in by a star import, which are not matched to the module they come from";

/// A model whose table name a `declared_attr` function gives, or that a mixin gives it, has no `__tablename__` in its
/// body, and a function is not an assignment.
const MIXIN_TABLES: &str = "table names given by a declared_attr or inherited from a mixin";

/// What the `dbapi` pack leaves out. SQL is declared (`CREATE TABLE`) or called (`SELECT`), so each is told under both.
const DBAPI_GAPS: [&str; 3] = [
    "execute and executemany calls on a receiver of more than three segments",
    "SQL held in a variable",
    "SQL built by a function call",
];

/// Gaps of every Python project, whatever its packs: contracts no pack of this connector reads.
fn always() -> Vec<(ContractKind, Role, &'static str)> {
    vec![
        (
            ContractKind::Graphql,
            Role::Serves,
            "GraphQL servers (Strawberry, Graphene) are not read",
        ),
        (
            ContractKind::Queue,
            Role::Serves,
            "task queues (Celery, RQ, Dramatiq) are not read",
        ),
    ]
}

/// What the rules of the pack named `pack` cannot see: `(kind, role, words)`.
fn gaps(pack: &str) -> Vec<(ContractKind, Role, &'static str)> {
    let serves = |words| (ContractKind::Http, Role::Serves, words);
    let calls = |words| (ContractKind::Http, Role::Calls, words);
    let tables = |words| (ContractKind::Table, Role::Declares, words);
    let table_calls = |words| (ContractKind::Table, Role::Calls, words);
    match pack {
        "fastapi" => vec![
            serves("prefixes held in variables"),
            serves(ROUTERS_ELSEWHERE),
            serves("routes registered by calls to add_api_route"),
            serves(APPLICATIONS_MOUNTED),
            serves(MOUNTS_UNFOLLOWED),
            serves(STAR_IMPORTS),
        ],
        "flask" => vec![
            serves("Flask routes with several methods beyond the first"),
            serves("Flask routes whose methods are held in a variable, which are read as GET"),
            serves("prefixes held in variables"),
            serves(ROUTERS_ELSEWHERE),
            serves("routes registered by calls to add_url_rule and class-based views"),
            serves(MOUNTS_UNFOLLOWED),
            serves(STAR_IMPORTS),
        ],
        "django" => vec![
            serves("prefixes held in variables"),
            serves("re_path and regex routes"),
            serves("routes made by Django REST framework routers"),
            serves(MOUNTS_UNFOLLOWED),
            serves(STAR_IMPORTS),
            tables("Django default table names"),
        ],
        "requests" => vec![
            calls("URLs held in variables"),
            calls("HTTP clients other than requests and httpx"),
            calls("sessions created in another module than the one that uses them"),
            calls(CLIENTS_REBOUND),
            calls(STAR_IMPORTS),
        ],
        "httpx" => vec![
            calls("URLs held in variables"),
            calls("HTTP clients other than requests and httpx"),
            calls("clients created in another module than the one that uses them"),
            calls(CLIENTS_REBOUND),
            calls(STAR_IMPORTS),
        ],
        "sqlalchemy" => vec![
            tables("table names held in variables"),
            tables(MIXIN_TABLES),
            tables(STAR_IMPORTS),
        ],
        "alembic" => vec![
            tables("table names held in variables"),
            tables(STAR_IMPORTS),
        ],
        "dbapi" => DBAPI_GAPS
            .iter()
            .flat_map(|&words| [tables(words), table_calls(words)])
            .collect(),
        "env" => vec![
            (
                ContractKind::Env,
                Role::Calls,
                "variable names held in variables",
            ),
            (
                ContractKind::Env,
                Role::Calls,
                "settings read by environs, django-environ or python-decouple",
            ),
            (
                ContractKind::Env,
                Role::Calls,
                "fields of a settings class that derives from another settings class",
            ),
            (
                ContractKind::Env,
                Role::Calls,
                "an alias written inside a field's annotation",
            ),
            (ContractKind::Env, Role::Calls, STAR_IMPORTS),
        ],
        _ => Vec::new(),
    }
}

/// The kinds of contract the rules of `pack` name, with the role they give them.
fn kinds(pack: &Pack) -> impl Iterator<Item = (ContractKind, Role)> + '_ {
    let calls = pack.calls.iter().map(|rule| (rule.kind.clone(), rule.role));
    let decorators = pack
        .decorators
        .iter()
        .map(|rule| (rule.kind.clone(), rule.role));
    let chains = pack
        .chains
        .iter()
        .map(|rule| (rule.kind.clone(), rule.role));
    let assigns = pack
        .assigns
        .iter()
        .map(|rule| (rule.kind.clone(), rule.role));
    calls.chain(decorators).chain(chains).chain(assigns)
}

/// What is known of one kind and role while the packs are read; sets keep the words sorted and free of repeats.
#[derive(Default)]
struct Entry<'a> {
    via: BTreeSet<&'a str>,
    blind_spots: BTreeSet<&'a str>,
}

/// One entry per kind and role, in that order, with the packs that find it and the gaps they leave.
pub(crate) fn provides(packs: &[Pack]) -> Vec<Provide> {
    let mut entries: BTreeMap<(ContractKind, Role), Entry<'_>> = BTreeMap::new();
    for pack in packs {
        for kind_and_role in kinds(pack) {
            entries
                .entry(kind_and_role)
                .or_default()
                .via
                .insert(pack.name.as_str());
        }
        for (kind, role, words) in gaps(&pack.name) {
            entries
                .entry((kind, role))
                .or_default()
                .blind_spots
                .insert(words);
        }
    }
    for (kind, role, words) in always() {
        entries
            .entry((kind, role))
            .or_default()
            .blind_spots
            .insert(words);
    }
    entries
        .into_iter()
        .map(|((kind, role), entry)| Provide {
            kind,
            role,
            via: entry.via.into_iter().map(str::to_owned).collect(),
            blind_spots: entry.blind_spots.into_iter().map(str::to_owned).collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::{MAX_DEPTH, MAX_WAYS};

    #[test]
    fn the_blind_spot_for_mounts_names_the_limits_link_applies() {
        // The words are a copy of the numbers in `link`, which they would silently outlive.
        assert!(MOUNTS_UNFOLLOWED.contains(&format!("deeper than {MAX_DEPTH} routers")));
        assert!(MOUNTS_UNFOLLOWED.contains(&format!("in more than {MAX_WAYS} ways")));
    }
}
