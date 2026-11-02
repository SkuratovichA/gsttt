# Spec Delta

## ADDED Requirements

### Requirement: Relative requests
With the `fetch` and `axios` packs, a relative URL with no base SHALL be read as the path from the root, as a browser resolves it against the page: `fetch('items')` calls `http * /items`. A URL with a `..` segment, or whose path starts with a computed part after a dropped `./`, SHALL be an unread site, and a target that names no path (`data:…`, `?x=1`) SHALL be no request. An absolute URL SHALL be read as before.

#### Scenario: A relative request with no base calls the rooted path
- **WHEN** a file has `fetch('items')`, `axios.get('users')`, ``fetch(`items/${id}`)``, `bare.get('users/me')` on `const bare = axios.create()` and `unread.get('users')` on `axios.create({ baseURL: BASE })` with `BASE` not a literal
- **THEN** it calls `http * /items`, `http GET /users`, `http * /items/{}`, `http GET /users/me` and `http GET /users`

#### Scenario: A relative request that cannot be rooted is unread
- **WHEN** a file has `fetch('../items')` and ``fetch(`./${id}`)``
- **THEN** each is an unread site at its line

#### Scenario: A request target that names no path is no request
- **WHEN** a file has `fetch('data:text/plain,x')` and `fetch('?x=1')`
- **THEN** the file has no contract site and no unread site at those lines

#### Scenario: An absolute request is unchanged by relative requests
- **WHEN** a file has `fetch('https://a.test/x')`
- **THEN** it calls `http * /x`
