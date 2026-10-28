//! What a project promises about contracts, and what it cannot see.

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::connector::Provide;
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::rules::Pack;

/// One entry per kind and role the active packs produce, naming the packs behind it and the blind spots of reading code for it.
pub(crate) fn provides(packs: &[Pack]) -> Vec<Provide> {
    let mut groups: BTreeMap<(ContractKind, Role), BTreeSet<&str>> = BTreeMap::new();
    for pack in packs {
        let rules = pack
            .calls
            .iter()
            .map(|rule| (&rule.kind, rule.role))
            .chain(pack.decorators.iter().map(|rule| (&rule.kind, rule.role)))
            .chain(pack.chains.iter().map(|rule| (&rule.kind, rule.role)))
            .chain(pack.assigns.iter().map(|rule| (&rule.kind, rule.role)))
            .chain(pack.imports.iter().map(|rule| (&rule.kind, rule.role)));
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
            "routes on an app or router received as a function parameter are not read: the receiver must be made in the same file",
            "a mount path array with a non-literal element is read for its literal elements only",
            "routers that mount each other in a cycle are followed until the cycle closes",
        ],
        (ContractKind::Http, Role::Calls) => &[
            "URLs held in variables are not followed",
            "clients created in another file are not recognised: an axios.create receiver must be made in the file that calls it",
        ],
        (ContractKind::Graphql, Role::Serves) => &[
            "resolvers built at run time are not read",
            "schema-first resolvers outside a resolvers map are not read",
        ],
        (ContractKind::Trpc, Role::Serves) => &[
            "routers merged or built at run time are not followed",
            "router entries that are spread, read from a member (other.router) or built by another call are not followed",
        ],
        (ContractKind::Trpc, Role::Calls) => {
            &["clients under a name other than trpc or api are not recognised"]
        }
        (ContractKind::Table, Role::Calls) => &[
            "raw SQL and query builders are not read",
            "clients under a root other than the chain roots of the pack are recognised only through an alias of this, or a name the same file declares with one of the root types of the pack, directly or through a type alias of that file",
            "a root type is not read when the file imports it under another name (PrismaClient as Base), reaches it through a renamed namespace (P.TransactionClient) or combines it with other types (PrismaClient & {…})",
            "model types are read only from the packages the pack names: those of a client generated to a custom output path, or of a package that re-exports the client with export * from '@prisma/client', name no table",
            "an enum imported by name from @prisma/client reads as a table of that name that no model declares",
        ],
        (ContractKind::Table, Role::Declares) => &["tables named at run time are not read"],
        (ContractKind::Env, Role::Calls) => {
            &["variables read through an alias of the environment object are not seen"]
        }
        _ => &[],
    };
    spots.iter().map(|spot| (*spot).to_owned()).collect()
}
