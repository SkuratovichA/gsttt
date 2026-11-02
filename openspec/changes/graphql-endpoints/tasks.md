# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The config

- [x] 2.1 `crates/core/src/setup.rs`: `Setup::graphql_endpoints` and `GraphqlEndpoint`
- [x] 2.2 `crates/config/src/evaluate.rs`: the key, the check that both lists are not empty, and the globs checked with `glob_set` and `GlobError::in_option`
- [x] 2.3 The fixture `full`, the schema test and the reviewed snapshots

## 3. The index

- [x] 3.1 `crates/core/src/index.rs`: the endpoints; `engine/refresh.rs` sets them from the setup; `engine/snapshot.rs`: `LAYOUT` 7 to 8
- [x] 3.2 `crates/core/src/testing.rs`: `Tree::endpoint`

## 4. The queries

- [x] 4.1 `crates/core/src/query/scope.rs`: the scope of a site, worked out on demand, and `meet`
- [x] 4.2 `crates/core/src/query/cross.rs`: `Crossed` and `Leavers` keyed by scope, a file leaves a scoped contract through each of its roles
- [x] 4.3 `crates/core/src/query/walk.rs` and `walk/names.rs`: the new `Leavers` and `cross_as`

## 5. The CLI

- [x] 5.1 `crates/gsttt/src/commands/config/show.rs`: the section `graphql_endpoints`

## 6. The tests

- [x] 6.1 Config: the order of the entries, the empty lists and the missing keys, an invalid glob, the fingerprint
- [x] 6.2 Core: the scenarios of `impact` and `path`, and the bookkeeping of two scopes in one layer and of a site reached by the farther of two leavers
- [x] 6.3 Binary: `config show` with and without endpoints

## 7. The docs

- [x] 7.1 `docs/architecture.md`: the endpoints, the meeting rule of GraphQL contracts, the layout number
