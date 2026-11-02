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

### Requirement: Spreads
A call with `...spread` among its arguments, and an object literal with `...spread` among its properties, SHALL hold what they may give: the argument at or after the spread, and a property the literal does not write. What a call holds and no connector can read as text SHALL leave the value unread, and an argument written after a spread SHALL not be read at the place it is written.

#### Scenario: A spread may carry a property
- **WHEN** a pack calls `http` for `client.request` with the key `$arg1` and the method `$arg2.method|GET`, and a file has `client.request('/plain')` on line 1, `client.request('/post', { method: 'POST' })` on line 2, `client.request('/options', { ...options })` on line 3, `client.request('/rest', ...rest)` on line 4, `client.request(...args)` on line 5 and `client.request('/own', { ...options, method: 'PUT' })` on line 6
- **THEN** the file calls `http GET /plain`, `http POST /post`, `http * /options`, `http * /rest` and `http PUT /own`
- **AND** the call on line 5 is an unread site
