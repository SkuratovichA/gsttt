## ADDED Requirements

### Requirement: CommonJS clients
A call assigned to `module.exports`, `exports.name`, `module.exports.name` or a property of `module.exports = { … }` SHALL be a client made under that export name, apart from any variable of that name, and a variable declared with such an assignment SHALL be made by the same call. A member read from a `require` or default import of a module with no `default` SHALL be read as the export of that name. A module with a `default` SHALL be read as before.

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

#### Scenario: A call assigned to an export leaves a variable of the same name as it is
- **WHEN** `wrapped.js` has `const router = express.Router()`, `router.get('/wrapped-route', h)` on line 4 and `exports.router = wrap(router)`
- **THEN** `wrapped.js` serves `http GET /wrapped-route` at line 4
- **AND** in `collide.js` with `const client = notAClient()`, `exports.client = axios.create({ baseURL: '/collide' })` and `client.get('/own-must-not-read')` the call is read as nothing, and `user.js`, which takes `client` from it with `require`, calls `http GET /collide/from-importer`

#### Scenario: A variable declared with an assignment to an export is made by the same call
- **WHEN** `router.js` has `const router = module.exports = express.Router()` and `router.get('/chained', h)` on line 4, and `app.js` has `const app = module.exports = express()` and `app.use('/mounted', require('./router'))`
- **THEN** `router.js` serves `http GET /mounted/chained` at line 4
- **AND** `client.js` with `const client = exports.client = axios.create({ baseURL: '/both' })` calls `http GET /both/own` on `client`, and `user.js`, which takes `client` from it with `require`, calls `http GET /both/imported`
- **AND** `const made = exports.renamed = axios.create(…)` is read the same way under the name `renamed`, and `const other = module.exports = exports = express.Router()` serves under its mount
