# Tasks

## 1. Imports

- [x] 1.1 `visit_ts_import_type` reads `typeof import('x')` and `import('x').T` as a type-only static import at the line of the specifier, naming the first segment of the qualifier, and walks into the types nested in it
- [x] 1.2 The first argument of the mock, unmock and actual-module calls of Vitest and Jest is a dynamic import with no names through `load`, when it is a string or a template with no expression, and a computed one is neither an import nor an unread site
- [x] 1.3 `resolve` drops the query of a specifier before resolving it, and the fact keeps the specifier as written
- [x] 1.4 The connector version is 2
- [x] 1.5 A test annotated `#[scenario("typescript", …)]` for every scenario of the requirements Imports in types, Mocked modules and Query suffixes

## 2. Prisma tables

- [x] 2.1 `ChainRule` has `root_types` and `Pack` has `imports`, a list of `ImportRule` with `from`, `kind`, `role`, `except` and `case`, and the pack schema describes both
- [x] 2.2 `analyze/roots.rs` finds the aliases of `this` and the names declared with a root type before the walk, and `chain_rules` reads an alias as `this` and a typed name as a root
- [x] 2.3 `import_rules` applies the import rules to each name of an import and of a re-export by name, at the line of the declaration
- [x] 2.4 The `prisma` pack lists `PrismaClient`, `PrismaService` and `Prisma.TransactionClient` as root types, and names a table for each name imported from `@prisma/client` except `Prisma`, `PrismaClient`, `PrismaPromise` and `$Enums`
- [x] 2.5 `visit_ts_import_type` records `*` for `typeof import('x')` without a qualifier
- [x] 2.6 A test annotated `#[scenario("typescript", …)]` for every scenario of the requirements Aliases of this, Roots by type, Type aliases of root types and Imported model types
- [x] 2.7 The scenario A pack file has a schema counts six kinds of rules, and the pinned pack schema holds `imports` and `root_types`

## 3. Review fixes

- [x] 3.1 A name the file declares as a `type` alias, an `interface`, a `class` or an `enum` is that local type and never a root type of the pack, and a root type imported under another name, reached through a renamed namespace import or intersected with another type stays unread
- [x] 3.2 The walk through the type aliases of a file keeps a list of the names it has followed instead of a depth limit, so that a chain of any length is followed, a cycle ends, and a type that names itself many times is followed once
- [x] 3.3 A name is an alias of `this` or a typed root only when every binding of it in the file, and of `this.<name>` for a class property, binds `this` or a root type, and one other binding makes it neither for the whole file
- [x] 3.4 The parameters of a signature without a body bind nothing
- [x] 3.5 An import type reaches the import rules by the first name of its qualifier, at the line of its specifier
- [x] 3.6 The doc comments of the `imports` and `root_types` rule fields say that only the TypeScript connector reads them, and the pinned pack schema follows
- [x] 3.7 The provides text, the README and the architecture document describe the roots of the pack, an alias of `this` and the root types of the pack instead of listing them, say that the pack has `[[imports]]`, and name what is not read: a root type renamed on import, reached through a renamed namespace or combined with other types, the model types of a client generated to a custom `output` path or re-exported by a package, and an enum imported by name
