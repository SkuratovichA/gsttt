# Proposal: Read a fetch that writes no method as a GET

## Why

`fetch('/x')` is indexed under the key `http * /x`, so `gsttt contract 'GET /x'` answers calls 0 although `impact` of the server finds the caller. The Fetch standard makes a call with no `method` a GET, so the key can say so.

## What Changes

- TypeScript: the `fetch` pack reads its method as `$arg2.method|GET`, a value expression that already exists. `fetch(url)`, `fetch(url, {})` and `fetch(url, { headers })` call `http GET <url>`, `fetch(url, { method: 'PUT' })` calls `http PUT <url>`, and a call whose options cannot be read (`fetch(url, options)`, `{ method: verb }`, `{ ...options }`) keeps `http * <url>`.
- The new requirement "A fetch with no method is a GET" (`typescript`) has the two scenarios of the new reading, and "Contract lookup" (`queries`) gets the scenario that a lookup of a GET finds the fetch.
- The requirement "HTTP requests" no longer spells the `*` of an absent method, and the scenarios that spelled `http * <url>` for a `fetch` with no method spell `http GET <url>`.
- Versions: none. The text of every active pack is part of the salt of the TypeScript connector (`salt_for` in `crates/lang-ts/src/project.rs`), so a changed pack reads every file again, and the outcome of the core is unchanged.

## Impact

- Capabilities `typescript` and `queries`.
- `crates/lang-ts/packs/fetch.toml`, and the tests and snapshots of the TypeScript connector that spelled `http * <url>` for a `fetch` with no method.
- An index of a tree with a `fetch` that writes no method gives `http GET` keys where it gave `http *`. A server of another method no longer meets such a call.
