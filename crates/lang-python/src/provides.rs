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

/// `link` sees one project at a time and follows each router up through the mounts it can find, so a mount it cannot
/// read leaves the router served behind its own prefix alone.
const MOUNTS_UNFOLLOWED: &str = "mounts of a router by a file of another Python project, in a cycle, or in more ways than are followed";

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
    match pack {
        "fastapi" => vec![
            serves("prefixes held in variables"),
            serves(ROUTERS_ELSEWHERE),
            serves("routes registered by calls to add_api_route"),
            serves(MOUNTS_UNFOLLOWED),
        ],
        "flask" => vec![
            serves("Flask routes with several methods beyond the first"),
            serves("prefixes held in variables"),
            serves(ROUTERS_ELSEWHERE),
            serves("routes registered by calls to add_url_rule and class-based views"),
            serves(MOUNTS_UNFOLLOWED),
        ],
        "django" => vec![
            serves("prefixes held in variables"),
            serves("re_path and regex routes"),
            serves("routes made by Django REST framework routers"),
            serves(MOUNTS_UNFOLLOWED),
            tables("Django default table names"),
        ],
        "requests" => vec![
            calls("URLs held in variables"),
            calls("HTTP clients other than requests and httpx"),
            calls("sessions created in another module than the one that uses them"),
        ],
        "httpx" => vec![
            calls("URLs held in variables"),
            calls("HTTP clients other than requests and httpx"),
            calls("clients created in another module than the one that uses them"),
        ],
        "sqlalchemy" | "alembic" => vec![tables("table names held in variables")],
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
