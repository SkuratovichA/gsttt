## ADDED Requirements

### Requirement: URLs built with the URL class
A URL argument written `new URL(path, base)`, alone or followed by `.href`, `.pathname` or `.toString()`, SHALL be read as the URL it builds: a path that starts with `/`, starts with a computed part or holds its own origin is the URL, a relative path joins the directory of a literal base, and a base that is not a literal is a base URL that is dropped. A path that is not read SHALL leave the call an unread site.

#### Scenario: A URL built from a path and a base is called
- **WHEN** a file has `axios.post(new URL('build-runner/success', this.baseUrl).href, body)` and `fetch(new URL('/users', process.env.API_URL))`
- **THEN** it calls `http POST /build-runner/success` and `http * /users`

#### Scenario: A URL built from a template that starts with a base URL keeps its path
- **WHEN** a file has ``fetch(new URL(`${baseUrl}/users`))`` and ``fetch(new URL(`${API}/orders`, window.location.origin))``
- **THEN** it calls `http * /users` and `http * /orders`

#### Scenario: A relative path joins the directory of a literal base
- **WHEN** a file has `fetch(new URL('users/' + id, 'https://api.example.com/v1/'))` and `fetch(new URL('users', 'https://api.example.com/v1'))`
- **THEN** it calls `http * /v1/users/{}` and `http * /users`

#### Scenario: A URL built from a computed path is an unread site
- **WHEN** a file has `fetch(new URL(path, base))`
- **THEN** the file has an unread site at that line and calls nothing for it
