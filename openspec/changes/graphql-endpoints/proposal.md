# Proposal: Cross a GraphQL contract only within an endpoint

## Why

A GraphQL operation is keyed by its type and field, and nothing in a key says which API it belongs to. In a repository with several GraphQL APIs, or several clients of one gateway, `Query.user` in one API is a different operation from `Query.user` in another, and a change of a resolver reached every client of every API that happens to name a field alike. HTTP has the same problem and `http_prefixes` fixes it with a rule on the key; for GraphQL the key cannot tell, so the config has to say which files make an endpoint.

## What Changes

- A top-level `[[graphql_endpoints]]` array in `.gsttt/config.toml`. Each entry has `clients` and `servers`, non-empty lists of globs matched against repository-relative paths. Both lists are checked as the config is read, with the glob check that `[[external]]` uses for its `files`.
- The endpoints reach the index with the rest of the setup, and `gsttt config show` prints them.
- `impact` and `path` cross a GraphQL contract only between sites that meet. A `serves` site belongs to the endpoints whose `servers` match its file, a `declares` site to those whose `clients` or `servers` match, and a `calls` site to those whose `clients` match or, when none does, whose `servers` match, because a gateway that is no client of its own endpoint calls the API it serves. Two sites meet when the one in the higher role (`calls`, then `serves`, then `declares`) belongs to no endpoint, or the two share one. A site that matches no glob therefore still meets every site as a caller, and a caller of an endpoint does not reach a server that matches no glob.
- The rule is a property of the pair of sites, so a walk towards callers and a walk towards declarers agree on it.
- A file that holds several roles of a scoped contract leaves it through each of them, since the roles can belong to different endpoints: a gateway serves one endpoint and calls another. For every other contract the file keeps leaving through its highest role only.
- Only GraphQL contracts are scoped. An index without endpoints answers as before, with no glob matched.
- The `contract` query lists every site of a key whatever the endpoints.
- The snapshot layout goes from 7 to 8, because the index holds the endpoints. `FACT_FORMAT` and the connectors' versions stay, since no fact changes. `build_index` keeps its signature, which the test support of `lang-rust` and `lang-ts` calls: the engine sets the endpoints on the index it returns, from the setup.

## Impact

- Capabilities `config`, `queries` and `cli`.
- `crates/core/src/setup.rs`, `index.rs`, `engine/refresh.rs`, `engine/snapshot.rs`, `testing.rs`, `query/scope.rs` (new), `query/cross.rs`, `query/walk.rs`, `query/walk/names.rs`; `crates/config/src/evaluate.rs` with its schema, fixture and snapshots; `crates/gsttt/src/commands/config/show.rs`; `docs/architecture.md`.
- Indexes built by an earlier version are dropped and built again once. A change to `graphql_endpoints` changes the fingerprint of the setup, so the next query indexes again.
- Cost: a query that meets no GraphQL contract pays nothing, since the scope of a file is worked out the first time a walk asks for it.
