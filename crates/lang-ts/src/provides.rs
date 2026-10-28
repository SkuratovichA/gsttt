//! What a project promises about contracts, and what it cannot see.

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::connector::Provide;
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::rules::Pack;

/// One entry per kind and role the active packs produce, naming the packs behind it and the blind spots of reading code
/// for it.
pub(crate) fn provides(packs: &[Pack]) -> Vec<Provide> {
    let mut groups: BTreeMap<(ContractKind, Role), BTreeSet<&str>> = BTreeMap::new();
    for pack in packs {
        let rules = pack
            .calls
            .iter()
            .map(|rule| (&rule.kind, rule.role))
            .chain(pack.decorators.iter().map(|rule| (&rule.kind, rule.role)))
            .chain(pack.chains.iter().map(|rule| (&rule.kind, rule.role)))
            .chain(pack.assigns.iter().map(|rule| (&rule.kind, rule.role)));
        for (kind, role) in rules {
            groups
                .entry((kind.clone(), role))
                .or_default()
                .insert(pack.name.as_str());
        }
        // Resolver maps and tRPC routers are read by shape, whatever the rules of the pack say.
        let by_shape = match pack.name.as_str() {
            "graphql" => Some(ContractKind::Graphql),
            "trpc" => Some(ContractKind::Trpc),
            _ => None,
        };
        if let Some(kind) = by_shape {
            groups
                .entry((kind, Role::Serves))
                .or_default()
                .insert(pack.name.as_str());
        }
    }
    groups
        .into_iter()
        .map(|((kind, role), via)| Provide {
            blind_spots: blind_spots(&kind, role),
            via: via.into_iter().map(str::to_owned).collect(),
            kind,
            role,
        })
        .collect()
}

fn blind_spots(kind: &ContractKind, role: Role) -> Vec<String> {
    let spots: &[&str] = match (kind, role) {
        (ContractKind::Http, Role::Serves) => &[
            "file-system routes (Next.js, Remix, SvelteKit) are not read",
            "routers mounted from another package are not followed",
            "prefixes and paths held in variables are not followed",
            "a global prefix set at start-up (NestJS setGlobalPrefix) is not applied",
        ],
        (ContractKind::Http, Role::Calls) => &[
            "URLs held in variables are not followed",
            "clients created in another file are not recognised",
        ],
        (ContractKind::Graphql, Role::Serves) => &[
            "resolvers built at run time are not read",
            "schema-first resolvers outside a resolvers map are not read",
        ],
        (ContractKind::Trpc, Role::Serves) => {
            &["routers merged or built at run time are not followed"]
        }
        (ContractKind::Trpc, Role::Calls) => {
            &["clients under a name other than trpc or api are not recognised"]
        }
        (ContractKind::Table, Role::Calls) => &[
            "raw SQL and query builders are not read",
            "clients under another name are not recognised",
        ],
        (ContractKind::Table, Role::Declares) => &["tables named at run time are not read"],
        (ContractKind::Env, Role::Calls) => {
            &["variables read through an alias of the environment object are not seen"]
        }
        _ => &[],
    };
    spots.iter().map(|spot| (*spot).to_owned()).collect()
}
