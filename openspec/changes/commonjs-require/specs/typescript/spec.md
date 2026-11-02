## ADDED Requirements

### Requirement: CommonJS clients
A call assigned to `module.exports`, `exports.name`, `module.exports.name` or a property of `module.exports = { … }` SHALL be a client made under that export name, and a member read from a `require` or default import of a module with no `default` of its own SHALL be read as the export of that name. A module with a `default` SHALL be read as before.

#### Scenario: A client assigned to module.exports is called by a file that requires it
- **WHEN** `default-client.js` has `module.exports = axios.create({ baseURL: '/whole' })` and `whole.js` has `const client = require('./default-client')` and `client.get('/whole-module')` on line 3
- **THEN** `whole.js` calls `http GET /whole/whole-module` at line 3
- **AND** `default-client.js` lists no exports, and an ES module with `export default axios.create(…)` is read as before

#### Scenario: A client assigned to a named CommonJS export is called by a file that requires it
- **WHEN** `exports-client.js` has `exports.client = axios.create({ baseURL: '/named' })` and `users.js` takes `client` from it with `require` and calls `client.get('/from-exports')`
- **THEN** `users.js` calls `http GET /named/from-exports`
- **AND** `module.exports.client = axios.create(…)` and `module.exports = { client: axios.create(…) }` are read the same way

#### Scenario: A member of a required CommonJS module is read as its export
- **WHEN** `http-client.js` has `module.exports = { httpClient }` for `axios.create({ baseURL: '/api' })`, and `namespace.js` has `const clients = require('./http-client')` and `clients.httpClient.get('/namespace')`
- **THEN** `namespace.js` calls `http GET /api/namespace`
- **AND** `import clients from './http-client'` in a TypeScript file is read the same way
- **AND** a module that has a `default` export is still read as its default
