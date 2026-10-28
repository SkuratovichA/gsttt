# Tasks

## 1. Imports

- [x] 1.1 `visit_ts_import_type` reads `typeof import('x')` and `import('x').T` as a type-only static import at the line of the specifier, naming the first segment of the qualifier, and walks into the types nested in it
- [x] 1.2 The first argument of the mock, unmock and actual-module calls of Vitest and Jest is a dynamic import with no names through `load`, when it is a string or a template with no expression, and a computed one is neither an import nor an unread site
- [x] 1.3 `resolve` drops the query of a specifier before resolving it, and the fact keeps the specifier as written
- [x] 1.4 The connector version is 2
- [x] 1.5 A test annotated `#[scenario("typescript", …)]` for every scenario of the requirements Imports in types, Mocked modules and Query suffixes

## 2. Prisma tables

- [ ] 2.1 `ChainRule` has `root_types` and `Pack` has `imports`, a list of `ImportRule` with `from`, `kind`, `role`, `except` and `case`, and the pack schema describes both
- [ ] 2.2 `analyze/roots.rs` finds the aliases of `this` and the names declared with a root type before the walk, and `chain_rules` reads an alias as `this` and a typed name as a root
- [ ] 2.3 `import_rules` applies the import rules to each name of an import and of a re-export by name, at the line of the declaration
- [ ] 2.4 The `prisma` pack lists `PrismaClient`, `PrismaService` and `Prisma.TransactionClient` as root types, and names a table for each name imported from `@prisma/client` except `Prisma`, `PrismaClient`, `PrismaPromise` and `$Enums`
- [x] 2.5 `visit_ts_import_type` records `*` for `typeof import('x')` without a qualifier
- [ ] 2.6 A test annotated `#[scenario("typescript", …)]` for every scenario of the requirements Aliases of this, Roots by type, Type aliases of root types and Imported model types
- [ ] 2.7 The scenario A pack file has a schema counts six kinds of rules, and the pinned pack schema holds `imports` and `root_types`
