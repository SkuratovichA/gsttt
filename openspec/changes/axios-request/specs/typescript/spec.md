## ADDED Requirements

### Requirement: Requests through request
With the `axios` pack, `axios.request(config)` and `request(config)` on a receiver made by `axios.create()`, in the file or imported from a file of the project, SHALL call the `url` of the config with its `method`, upper-cased, and `*` when absent. On a receiver that no `axios.create` makes it SHALL read nothing. A config that is no object literal SHALL be an unread site.

#### Scenario: A request with a config is a call
- **WHEN** a file has `axios.request({ url: '/bookings', method: 'POST' })` and `axios.request({ url: '/users' })`
- **THEN** it calls `http POST /bookings` and `http * /users`

#### Scenario: A request on a client made by axios.create is under its base
- **WHEN** a file has `const api = axios.create({ baseURL: '/api' })` and `api.request({ url: '/users', method: 'delete' })`
- **THEN** it calls `http DELETE /api/users`

#### Scenario: A request whose config is not read is an unread site
- **WHEN** a file has `axios.request(config)` with `config` a variable, and ``axios.request({ url: `/u/${id}` })``
- **THEN** the first is an unread site and the second calls `http * /u/{}`

#### Scenario: A request on a client made in another file is called
- **WHEN** `http.ts` has `export const api = axios.create({ baseURL: '/api' })` and `users.ts` imports `api` from `./http` and calls `api.request({ url: '/users', method: 'GET' })`
- **THEN** `users.ts` calls `http GET /api/users` at that line

#### Scenario: A request on a receiver that no axios.create makes is silent
- **WHEN** a file has `const cache = { request }` and calls `cache.request({ url: '/x' })`
- **THEN** the file has no contract site and no unread site
