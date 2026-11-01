# Proposal: Leave a computed URL on a client unread

## Why

A client made by `axios.create()` with no base URL, or with a base URL that has no literal part, passes its calls through a rule that skipped the check for a target of placeholders alone. ``api.get(`${base}${path}`)`` on `axios.create({ timeout: 1000 })` became a call whose path is all placeholders, and such a path meets every served route of its length. The specs already say that a request target that keeps no literal segment is an unread site; the rule did not hold for a call on a receiver.

## What Changes

- The check runs on the request target a call makes, the base URL of the receiver and the URL of the call together, for every HTTP call. A target with no literal path segment is an unread site whether or not the call is on a receiver.
- A call on a receiver still takes its first argument as a URL whatever it looks like, so `api.get('users')` on a client stays `http GET /users`.
- The requirement "HTTP requests" of the `typescript` capability gets the scenario "A client with no base URL leaves a computed URL unread".
- `FACT_FORMAT` goes from 10 to 11: the key of cached facts does not cover the rule code, so an unchanged file would keep its old call of placeholders alone.

## Impact

- Capability `typescript`.
- `crates/core/src/rules.rs`, `FACT_FORMAT` in `crates/core/src/engine/keys.rs`, and a fixture and a test in `crates/lang-ts/tests/`.
- A tree with no such call gives the same index as before.
