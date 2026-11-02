# Proposal: Meet HTTP paths that both hold literals only through a shared one

## Why

A call to an outside API, written as `${base}/${id}/messages`, met the repository's own route `POST /u/:id`. Normalisation drops the base, so the call's key is `http POST /{}/messages` and the route's is `http POST /u/{}`: the call's literal `messages` met the route's parameter and the route's literal `u` met the call's computed part, no literal agreed, and impact of the route still listed the caller.

## What Changes

- Two paths that meet segment by segment still do not meet when both hold a literal segment and no pair is the same literal on both sides. `/{}/messages` and `/u/{}` do not meet.
- A path of parameters alone keeps meeting every path of its length: `/{}` meets `/users` and `/{}/{}` meets `/a/b`, so a catch-all route keeps its callers. A call is never parameters alone, because such a call is a blind spot, so that path is always the served or declared side.
- `/bookings/{}` and `/bookings/me` meet because `bookings` is shared, and `/` meets `/`.
- With `http_prefixes` the rule applies to each pair of forms that is compared, so meeting stays symmetric and a prefix only adds matches.
- The requirement `HTTP matching` gets the rule, and two scenarios.
- `PARAMETER` names the `{}` segment once in `crates/core/src/contract.rs` and replaces the bare literal in the normaliser, `has_literal_segment` and the matcher.

## Impact

- Capability `queries`.
- `crates/core/src/query/http.rs` (`segments_meet` and the doc comment of `HttpMatcher`), `crates/core/src/contract.rs` and `crates/core/src/rules.rs` (`PARAMETER`), the tests in `crates/core/tests/queries/http.rs` and `crates/core/tests/queries/impact.rs`, and `docs/architecture.md`.
- No fact changes, so `FACT_FORMAT`, the versions of the connectors and the snapshot `LAYOUT` stay. `HttpMatcher::meets` is called only by `Crossing::meeting` in `crates/core/src/query/cross.rs`, at query time, and nothing in `crates/core/src/engine/` refers to it; the normaliser keeps writing the same keys.
- Cost: a call whose only literals fall on the parameters of a route, and whose parameters fall on the route's literals, no longer reaches the route.
