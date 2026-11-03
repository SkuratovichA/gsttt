//! What a project promises about contracts, and what it cannot see.

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::connector::Provide;
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::rules::Pack;

/// One entry per kind and role the active packs produce, naming the packs behind it and the blind spots of reading code for it.
pub(crate) fn provides(packs: &[Pack]) -> Vec<Provide> {
    let mut groups: BTreeMap<(ContractKind, Role), BTreeSet<&str>> = BTreeMap::new();
    for pack in packs {
        // The import rules are read here and by no other connector, so the pack does not list them.
        let imports = pack
            .imports
            .iter()
            .map(|rule| (rule.kind.clone(), rule.role));
        for contract in pack.contracts().chain(imports) {
            groups
                .entry(contract)
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
        .map(|((kind, role), via)| {
            let mut blind_spots = blind_spots(&kind, role);
            // Said only for a project that has the pack, so the projects of the other table packs keep their answer.
            if kind == ContractKind::Table && role == Role::Calls && via.contains("sql-drivers") {
                blind_spots.insert(0, SQL_DRIVERS_BLIND_SPOT.to_owned());
            }
            Provide {
                blind_spots,
                via: via.into_iter().map(str::to_owned).collect(),
                kind,
                role,
            }
        })
        .collect()
}

/// What the `sql-drivers` pack does not read.
const SQL_DRIVERS_BLIND_SPOT: &str = "SQL drivers are read from a literal or a template given to query, execute, prepare or exec; a query given as an object (pool.query({ text })) is not read, and a statement whose table or whole clause is computed does not parse and names no table";

fn blind_spots(kind: &ContractKind, role: Role) -> Vec<String> {
    let spots: &[&str] = match (kind, role) {
        (ContractKind::Http, Role::Serves) => &[
            "file-system routes (Next.js, Remix, SvelteKit) are not read",
            "routers mounted from another package are followed only when the mounting file imports them, directly or through a package it imports: a router of another package that no such file mounts keeps the path it is written with, and a route of another package that only its own rules read from a call it makes is served at the path written there",
            "a route path is read from a string, a template or a concatenation, and from a constant (a const, an enum member, a static property or a property of a constant object) declared in the same file, in the function or block around the route, or in a file of the same project that it is imported from; a path held in a parameter, in a variable that is not a `const` with a plain name, or in a key that an object spread follows, is not read",
            "a static property or an object property that the program assigns again is read with the value it declares",
            "a NestJS global prefix is applied only when it is a string or a constant of the same file set on an app that NestFactory.create(<name>) makes in the same file, and the exclude option is not read, so excluded routes get the prefix too; RouterModule paths are not read",
            "NestJS modules are followed through @Module imports and controllers written as names or forwardRef(() => Name); modules made by a call (forRoot, register), names read from a namespace (ns.Module) and anonymous default exports are not",
            "a NestJS controller serves the routes of the class it extends only when that class is named directly (not ns.Base), carries no @Controller of its own, and is its direct base",
            "routes on an app or router received as a function parameter are not read: the receiver must be made in the file that registers the route, or be imported from a file of the same project that makes it; a name is looked up in the variables the file makes with no scope resolution, so a parameter or a variable inside a function that is named like a router or app the file makes is read as that one",
            "a mount on an app or router imported from another file (importedApp.use('/x', router)) is read only when the name is the app or router itself, not a member of it (server.app.use) and not a name from a package; the routes of a router that no read mount reaches are served at the paths they are written with",
            "a mount path is read from a string or a constant, of the same file or imported, when a router follows it, and a mount path array with an element that is not text is read for its other elements only",
            "a NestJS controller whose @Controller prefix is not text (a call, a constant that is not text) serves no route and is reported as a site the file could not read: its own routes and those of the class it extends are not served",
            "a NestJS @Controller({ path: [...] }) serves its routes under the first path of the array that is text only, where @Controller([...]) serves them under every path",
            "routers that mount each other in a cycle are followed until the cycle closes",
        ],
        (ContractKind::Http, Role::Calls) => &[
            "a URL is read from a string, a template, a concatenation or new URL(…), and from a constant declared in the same file, in the function or block around the call, or in a file of the same project that it is imported from; a URL held in a parameter or in a variable that is not a `const` with a plain name is not read; a constant of another package is read when the file imports it",
            "a client is recognised when axios.create makes it in the file that calls it, or in a file of the same project that it is imported from; a client received as a parameter or injected is not, and a name is looked up in the variables the file makes with no scope resolution, so a parameter or a variable inside a function that is named like a client the file makes is read as that one",
        ],
        (ContractKind::Graphql, Role::Serves) => &[
            "resolvers built at run time are not read",
            "schema-first resolvers outside a resolvers map are not read",
        ],
        (ContractKind::Trpc, Role::Serves) => &[
            "routers merged or built at run time are not followed, and a router merged from another package is read at its own names",
            "router entries that are spread, read from a member (other.router) or built by another call are not followed",
        ],
        (ContractKind::Trpc, Role::Calls) => {
            &["clients under a name other than trpc or api are not recognised"]
        }
        (ContractKind::Table, Role::Calls) => &[
            "query builders and SQL built at run time are not read; raw SQL is read from a literal or a template given to $queryRaw, $executeRaw, $queryRawUnsafe or $executeRawUnsafe on a receiver of up to three segments, and from Prisma.sql, and a Prisma.sql fragment that is no statement names no table",
            "clients under a root other than the chain roots of the pack are recognised only through an alias of this, or a name the same file declares with one of the root types of the pack, directly or through a type alias of that file, or the first parameter of a callback given to $transaction; a name that the file also binds anywhere else to anything but this or a root type is not recognised (function a(tx2: PrismaClient) beside items.map(tx2 => tx2) reads no table), and a root type that the file declares as a type of its own is that local type",
            "a root type is not read when the file imports it under another name (PrismaClient as Base), reaches it through a renamed namespace (P.TransactionClient), writes it as typeof a name, or aliases an import type of it (type Db = import('@prisma/client').PrismaClient)",
            "model types are read only from the packages the pack names: those of a client generated to a custom output path, or of a package that re-exports the client with export * from '@prisma/client', name no table",
            "an enum imported by name from @prisma/client and used as a value reads as a table of that name that no model declares; a model type that the file only imports, re-exports or writes in a type names no table",
        ],
        (ContractKind::Table, Role::Declares) => &["tables named at run time are not read"],
        (ContractKind::Queue, Role::Calls) => &[
            "the kafka pack reads a producer only when its receiver is declared with a type it lists (a parameter, a class property, a constructor parameter property, or a variable made with new): a producer held in a name with no declared type, such as const producer = kafka.producer(), is not read",
        ],
        (ContractKind::Queue, Role::Serves) => &[
            "the kafka pack reads a consumer only when its receiver is declared with a type it lists, so const consumer = kafka.consumer() is not; topics given as an array to subscribe ({ topics: [...] }) are not read",
        ],
        (ContractKind::Env, Role::Calls) => {
            &["variables read through an alias of the environment object are not seen"]
        }
        (ContractKind::Env, Role::Serves) => &[
            "variables written through an alias of the environment object are not seen",
            "variables written by Object.assign(process.env, …), jest.replaceProperty(process.env, …) or process.env = {…} are not seen",
        ],
        _ => &[],
    };
    spots.iter().map(|spot| (*spot).to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_import_rules_of_a_pack_are_listed() {
        let packs = gsttt_config::builtin_packs(crate::PACKS).unwrap();
        let prisma = provides(&packs)
            .into_iter()
            .find(|provide| provide.kind == ContractKind::Table && provide.role == Role::Calls)
            .unwrap();
        assert_eq!(prisma.via, ["prisma", "sql-drivers"]);
        let only_imports = Pack {
            chains: Vec::new(),
            calls: Vec::new(),
            embeds: Vec::new(),
            ..packs
                .into_iter()
                .find(|pack| pack.name == "prisma")
                .unwrap()
        };
        assert!(
            !only_imports.imports.is_empty() && only_imports.contracts().next().is_none(),
            "the pack lists none itself"
        );
        assert_eq!(provides(&[only_imports]).len(), 1, "the provides add them");
    }
}
