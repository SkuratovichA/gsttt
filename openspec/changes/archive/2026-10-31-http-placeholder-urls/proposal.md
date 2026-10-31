# Proposal: a request URL with no literal path segment is a blind spot

## Why

A call whose URL is computed and whose path keeps no literal segment becomes a fact such as `http GET /{}`, and `{}` meets any segment, so the call is linked to every route of one segment. Amplication's `fetch(`${NPM_DOWNLOADS_API}${period || Period.LAST_WEEK}/${packageName}`)` is a call to an external API and gsttt answered that it calls `http GET /github`, a route of another service. A request to an external API in a private repository was linked to its local `GET /health` the same way. Only a path that normalises to the root was a blind spot, and `/{}` is as empty of information as `/`.

## What Changes

- A call URL that is computed and whose normalised path has no literal segment, which includes the root, is a blind spot with the reason computed at run time. `/users/{}` stays a fact. Routes that are served and calls under a prefix are unchanged.
- The rule is shared by every language, so the TypeScript, Python and Rust requirements and the `contract-formats` requirement about a computed origin say it, each with a scenario.
- `FACT_FORMAT` goes from 6 to 7: the key of cached facts does not cover the rule code, so an unchanged file would keep its old `http GET /{}` fact.

## Impact

- Capabilities `contract-formats`, `typescript`, `python` and `rust`.
- `crates/core/src/rules.rs`, `crates/core/src/engine/keys.rs`, and the tests and fixtures of the three connectors.
- Indexes built by an earlier version are parsed again once.
