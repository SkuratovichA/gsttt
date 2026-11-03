# Proposal: Count what waits for a failed link as unread

## Why

An answer of gsttt says what it covered. A contract site that only the link can key lives in the extra data of its connector and not in the facts. When the link of a project fails, by an error or by a panic the engine contains, those sites become neither facts nor unread sites, and the only trace is one coverage line about the failed link. Coverage then counts as read a file that holds sites nobody read.

## What Changes

- Core: `Project::unlinked` is a new provided method: the places in one file that only the link turns into facts, each with its line and the code in question. `DynProject::unlinked` is its form over the extra data as JSON, and gives nothing for extra data that does not decode.
- Core: when `link` fails or panics, the engine returns, in place of an empty `Linked`, one with `unread` holding those places for every file of the project, each with the reason `Unsupported("link failed")`. The call is contained, so a connector that panicked once cannot take the index down by panicking again. The coverage line of the failure stays.
- TypeScript: a site left to the link, an Express route on a host, a route of a class without the decorator, and a tRPC procedure listed in a router are named. A NestJS route that a controller declares, mounts, class prefixes that name a constant and controllers that extend a base class are not, because their own fact stands or they name no site.
- Python: each route declared on a router is named.
- OpenAPI: each operation of a fragment is named.
- Rust names nothing: the routes of a group are facts of the file, which the link only keys again, and a function names no site.
- Versions: none. The unread sites are computed in every run from the extra data and are not part of any stored fact, snapshot or reader output.

## Impact

- Capabilities `indexing`, `typescript`, `python` and `contract-formats`.
- `crates/core/src/connector.rs`, `crates/core/src/engine/pipeline.rs`, the projects of `lang-ts`, `lang-python` and `contracts` (OpenAPI), and `docs/architecture.md`.
- A run whose links succeed answers as before. A run with a failed link counts more unread sites.
