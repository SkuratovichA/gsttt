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
            "routers mounted from another package, and constants imported from another package, are not followed: the link reads one project at a time",
            "a route path is read from a string, a template or a concatenation, and from a constant (a const, an enum member, a static property or a property of a constant object) declared in the same file or in a file of the same project that it is imported from; a path held in a function-local value, or in a key that an object spread follows, is not read",
            "a static property or an object property that the program assigns again is read with the value it declares",
            "a NestJS global prefix is applied only when it is a string or a constant of the same file set on an app that NestFactory.create(<name>) makes in the same file, and the exclude option is not read, so excluded routes get the prefix too; RouterModule paths are not read",
            "NestJS modules are followed through @Module imports and controllers written as names or forwardRef(() => Name); modules made by a call (forRoot, register), names read from a namespace (ns.Module) and anonymous default exports are not",
            "a NestJS controller serves the routes of the class it extends only when that class is named directly (not ns.Base), carries no @Controller of its own, and is its direct base",
            "routes on an app or router received as a function parameter are not read: the receiver must be made in the same file",
            "a mount path is read from a string or a constant of the same file, not from an imported constant, and a mount path array with an element that is not text is read for its other elements only",
            "a NestJS controller whose @Controller prefix is not text (an imported constant, a call) serves no route and is reported as a site the file could not read: its own routes and those of the class it extends are not served",
            "routers that mount each other in a cycle are followed until the cycle closes",
        ],
        (ContractKind::Http, Role::Calls) => &[
            "a URL is read from a string, a template, a concatenation or new URL(…), and from a constant declared in the same file or in a file of the same project that it is imported from; a URL held in a function-local value or a parameter, and a constant of another package, are not read",
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
            "clients under a root other than the chain roots of the pack are recognised only through an alias of this, or a name the same file declares with one of the root types of the pack, directly or through a type alias of that file; a name that the file also binds anywhere else to anything but this or a root type is not recognised (function a(tx2: PrismaClient) beside items.map(tx2 => tx2) reads no table), and a root type that the file declares as a type of its own is that local type",
            "a root type is not read when the file imports it under another name (PrismaClient as Base), reaches it through a renamed namespace (P.TransactionClient), writes it as typeof a name, or aliases an import type of it (type Db = import('@prisma/client').PrismaClient)",
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
