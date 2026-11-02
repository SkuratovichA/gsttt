# Spec Delta

## ADDED Requirements

### Requirement: Receivers whose prefix is not text
A call that a rule serves routes with, on a receiver whose maker holds the `prefix` of the rule and no connector can read it as text, SHALL be an unread site and serve nothing, whether the receiver is made in the same file or in the file that exports it. The base of a client is no such prefix: a client whose base is not text SHALL be called at the path after it.

#### Scenario: A route on a router made in the same file with a prefix that is not text is unread
- **WHEN** a pack serves `get` on a router made by `makeRouter`, behind the `prefix` property of its argument, and `router.ts` has `dynamic = makeRouter({ prefix: process.env.PREFIX })`, `fixed = makeRouter({ prefix: '/v1' })` and `bare = makeRouter({})`, each with `get('/own', list)` called on it
- **THEN** `router.ts` serves `http GET /v1/own` and `http GET /own`
- **AND** the call on `dynamic` is an unread site and serves nothing

#### Scenario: A route on a router made in another file with a prefix that is not text is unread
- **WHEN** the same pack is on, `router.ts` exports the three routers, and `users.ts` imports them and calls `dynamic.get('/users', list)`, `fixed.get('/orders', list)` and `bare.get('/items', list)`
- **THEN** `users.ts` serves `http GET /v1/orders` and `http GET /items`
- **AND** the call on `dynamic` is an unread site and serves nothing

#### Scenario: A client whose base is not text is called at the path after it
- **WHEN** a file has `const api = axios.create({ baseURL: process.env.API })` and `api.get('/users')`
- **THEN** it calls `http GET /users` and has no unread site
