//! What a Go project says it finds and what it cannot see, for the coverage lines of an answer. The kinds come from the packs in force; the blind spots are written per pack in words an answer can quote.

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::connector::Provide;
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::rules::Pack;

/// SQL is declared (`CREATE TABLE`) or called (`SELECT`), so each gap is told under both.
const SQL_GAPS: [&str; 1] = ["SQL held in a variable or built by a call"];

/// Gaps of every Go project, whatever its packs: contracts no pack of this connector reads.
fn always() -> Vec<(ContractKind, Role, &'static str)> {
    vec![
        (
            ContractKind::Queue,
            Role::Serves,
            "gRPC and queues are not read",
        ),
        (
            ContractKind::Grpc,
            Role::Serves,
            "gRPC and queues are not read",
        ),
    ]
}

/// What no router pack sees, then the gaps of the pack itself.
fn router_gaps(own: &[&'static str]) -> Vec<(ContractKind, Role, &'static str)> {
    std::iter::once("a router handed to another function, or mounted with Mount, keeps no prefix: its routes are read under the path written at the call")
        .chain(own.iter().copied())
        .map(|words| (ContractKind::Http, Role::Serves, words))
        .collect()
}

/// What the rules of the pack named `pack` cannot see: `(kind, role, words)`.
fn gaps(pack: &str) -> Vec<(ContractKind, Role, &'static str)> {
    match pack {
        "net-http" => vec![(
            ContractKind::Http,
            Role::Calls,
            "HTTP calls on an http.Client held in a variable",
        )],
        "gin" | "echo" => router_gaps(&[
            "a route whose verb is text, Handle on gin or Add on echo, is read on a router made in the file, or held in a parameter, a field or a package variable typed in the same file",
        ]),
        "chi" => router_gaps(&[
            "chi routes are read on a router made in the file, or held in a parameter, a field or a package variable typed in the same file",
        ]),
        "fiber" => router_gaps(&[
            "fiber routes are read on a router made in the file, or held in a parameter, a field or a package variable typed in the same file",
        ]),
        "gorilla" => router_gaps(&[
            "gorilla routes built with Path(...).Methods(...).HandlerFunc(...) are not read",
        ]),
        "database-sql" => SQL_GAPS
            .iter()
            .flat_map(|&words| {
                [
                    (ContractKind::Table, Role::Declares, words),
                    (ContractKind::Table, Role::Calls, words),
                ]
            })
            .collect(),
        "env" => vec![(
            ContractKind::Env,
            Role::Calls,
            "variable names held in variables",
        )],
        _ => Vec::new(),
    }
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
        for contract in pack.contracts() {
            entries
                .entry(contract)
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
