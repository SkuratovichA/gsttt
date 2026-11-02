# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The config

- [x] 2.1 `crates/core/src/setup.rs`: `Setup::graphql_endpoints` and `GraphqlEndpoint`
- [x] 2.2 `crates/config/src/evaluate.rs`: the key, the check that both lists are not empty, and the globs checked with `glob_set` and `GlobError::in_option`
- [x] 2.3 The fixture `full`, the schema test and the reviewed snapshots

## 3. The index

- [x] 3.1 `crates/core/src/index.rs`: the endpoints; `engine/build.rs` takes them as a parameter of `build_index`; `engine/snapshot.rs`: `LAYOUT` 7 to 8
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

## 8. The review

- [x] 8.1 `crates/core/src/engine/build.rs`: `build_index` takes the endpoints, `engine/refresh.rs` no longer patches the index, and the test support of `lang-rust` and `lang-ts` passes none
- [x] 8.2 `crates/core/src/engine/build.rs`: a glob of an endpoint that matches no file is one line of the coverage problems, worked out once
- [x] 8.3 `crates/core/src/query/cross.rs`: `cross_as` without the flag that stopped an unscoped contract after its first role
- [x] 8.4 `crates/core/src/query/scope.rs`: the globs are compiled on the first ask, and a comment on the memo
- [x] 8.5 `crates/core/src/setup.rs`: `minItems` 1 on `clients` and `servers`, and the reviewed schema snapshot
- [x] 8.6 `crates/config/src/external.rs` and `evaluate.rs`: core's `GlobError` is the only glob error, and a bad glob of an external speaks through `in_option`
- [x] 8.7 Tests: the config reaches the index and its snapshot, in the engine and through the binary; the walk by name with endpoints; the glob that matches no file; the `contract` query lists every site
