# Spec Delta

## ADDED Requirements

### Requirement: Imported names in a template or a concatenation
A template or a concatenation that a rule reads SHALL be read with each name in it that the file imports from a file of its project as the text of the constant that file declares, directly, by namespace or through re-exports. A name that reads as no text SHALL be a computed part, as a name the file does not import is, so the site is read as it was before.

#### Scenario: A template with an imported constant is read
- **WHEN** `config.ts` exports `const BASE = '/api/v1'`, and `users.ts` imports it and calls ``fetch(`${BASE}/users`)`` and `fetch(BASE + '/orders')`
- **THEN** `users.ts` calls `http * /api/v1/users` and `http * /api/v1/orders` at the lines of the two calls
- **AND** it has no unread site

#### Scenario: An imported name the link cannot read keeps its reading
- **WHEN** `env.ts` has `export const BASE = process.env.API_URL`, `config.ts` imports `BASE` from it and exports `` const PATH = `${BASE}/path` ``, and `users.ts` imports both and `axios`, declares `const api = axios.create({ baseURL: BASE })`, and calls ``fetch(`${BASE}/users`)``, `fetch(BASE)`, `fetch(PATH)` and `api.get('/orders')`
- **THEN** `users.ts` calls `http * /users` and `http GET /orders` at the lines of the first and the last call
- **AND** it has an unread site at the line of `fetch(BASE)` and at the line of `fetch(PATH)`

### Requirement: Constants built from imported names
A top-level constant SHALL be read with each name in it that its file imports as the text of the constant the exporting file declares, whether it is a template, a concatenation or a name that stands alone, for the file that declares it and for the files that import it. A constant SHALL be text only when every part of it reads, so a part that cannot be read, or a cycle of constants, leaves the site unread.

#### Scenario: An exported constant built from an imported one is read
- **WHEN** `config.ts` exports `const BASE = '/api/v1'`, `routes.ts` imports it and exports `` const USERS = `${BASE}/users` ``, `const ORDERS = BASE + '/orders'` and `` const PATHS = { items: `${BASE}/items` } ``, and `client.ts` imports the three and calls `fetch(USERS)`, `fetch(ORDERS)`, `fetch(PATHS.items)` and ``fetch(`${USERS}/1`)``
- **THEN** `client.ts` calls `http * /api/v1/users`, `http * /api/v1/orders`, `http * /api/v1/items` and `http * /api/v1/users/1` at the lines of the four calls
- **AND** it has no unread site

#### Scenario: A constant of the same file built from an imported one is read
- **WHEN** `config.ts` exports `const BASE = '/api/v1'`, and `users.ts` imports it, declares `` const USERS = `${BASE}/users` ``, `const ORDERS = BASE + '/orders'` and `const ALIAS = BASE`, and calls `fetch(USERS)`, `fetch(ORDERS)`, ``fetch(`${USERS}/1`)`` and `fetch(ALIAS)`
- **THEN** `users.ts` calls `http * /api/v1/users`, `http * /api/v1/orders`, `http * /api/v1/users/1` and `http * /api/v1` at the lines of the four calls
- **AND** it has no unread site

#### Scenario: A cycle of constants stays unread
- **WHEN** `a.ts` has `import { B } from './b'` and `` export const A = `${B}/x` ``, `b.ts` has `import { A } from './a'` and `` export const B = `${A}/y` ``, and `use.ts` imports `A` and calls `fetch(A)`
- **THEN** `use.ts` has an unread site at the line of the call
- **AND** it calls no `http` key

### Requirement: Clients made with an imported base URL
A call on a client that the file makes with `axios.create`, whose `baseURL` names a constant the file imports from a file of its project, SHALL be read under the text of that constant, as a call on a client another file makes is. A client whose `baseURL` reads as no text SHALL be read as it is without a base.

#### Scenario: A client made with an imported base URL is called
- **WHEN** `config.ts` exports `const API_URL = '/api/v1'`, and `users.ts` imports it and `axios`, declares `const api = axios.create({ baseURL: API_URL })` and calls `api.get('/users')` and `api.post('/orders')`
- **THEN** `users.ts` calls `http GET /api/v1/users` and `http POST /api/v1/orders` at the lines of the two calls
- **AND** it has no unread site

### Requirement: Imported names that a local binds again
A name that a file imports and also binds as a parameter, a variable, a function or a class SHALL be no import for the value a rule reads anywhere in that file, so a site that reads it stays as it is without the import: a computed part in a template or a concatenation, and unread when the name stands alone.

#### Scenario: A name that a local binds again is no import
- **WHEN** `config.ts` exports `const BASE = '/api/v1'`, and `users.ts` imports it, declares ``function list(BASE: string) { return fetch(`${BASE}/users`) }`` and calls ``fetch(`${BASE}/orders`)`` and `fetch(BASE)`
- **THEN** `users.ts` calls `http * /users` and `http * /orders` at the lines of the two templates
- **AND** it has an unread site at the line of `fetch(BASE)`

### Requirement: Calls that wait for the link
A call or a decorator SHALL wait for the link when a value its rule reads its key from, or a decorator rule reads its own prefix from, names a constant another file declares, or when the rule found a contract and a value it reads its method from does. A call that no rule reads, whose method alone is such a name, SHALL stay the unread site it was and wait for nothing.

#### Scenario: A method that is an imported name defers nothing
- **WHEN** `verbs.ts` exports `const M = { post: 'POST' }` and `call.ts` imports `M` and calls `fetch(compute(), { method: M.post })`
- **THEN** `call.ts` has an unread site at the line of the call
- **AND** the call waits for nothing from the link

#### Scenario: A method that is an imported name is read by the link
- **WHEN** `verbs.ts` exports `const M = { post: 'POST' }` and `call.ts` imports `M` and calls `fetch('/orders', { method: M.post })`
- **THEN** `call.ts` calls `http POST /orders` at the line of the call
- **AND** it has no unread site

#### Scenario: A decorator prefix that is an imported name is read by the link
- **WHEN** a pack has a decorator rule for `Route` that reads its key from `$member` and its prefix from `$arg1`, `config.ts` exports `const BASE = '/api/v1'`, and `routes.ts` imports `BASE` and declares a class whose method `list` is decorated `@Route(BASE)`
- **THEN** `routes.ts` serves `http GET /api/v1/list` at the line of the decorator
- **AND** it has no unread site

### Requirement: A chain of constants is read to its end
A chain of constants that name one another across files SHALL be read to its end whatever its length, within the limits on the text it puts together. The read keeps its own stack of the constants it is inside of, so the length of a chain does not overflow the stack of a worker of the link, and the link finishes.

#### Scenario: A chain of constants across two files is read to its end
- **WHEN** `a.ts` and `b.ts` export 4000 constants between them, each a template that names the next one with a suffix, the last of which is text, and `use.ts` calls `fetch` with the first
- **THEN** `use.ts` calls `http * /end` followed by one suffix for each constant before the last, at the line of the call
- **AND** it has no unread site and the run has no problem line

### Requirement: A constant built deeper than the limit is no constant
A constant built from more than 32 nested levels of constants SHALL be no constant: a site that reads it is an unread site. The rest of the project SHALL link as it would without it, because the data the parse keeps for the link stays within the depth a file of the cache can be read back at.

#### Scenario: A constant built deeper than the limit is no constant and the project still links
- **WHEN** `deep.ts` exports 60 templates that each name the one before and ends with a call on the 32nd and a call on the last, and `other.ts` calls a route elsewhere in the project
- **THEN** `deep.ts` calls the route of the 32nd, and has an unread site at the line of the call on the last
- **AND** `other.ts` calls its route and the run has no problem line

### Requirement: A constant that doubles past the limit is no constant
A constant whose text, put together from the constants it names, is longer than one mebibyte SHALL be no constant: a site that reads it is an unread site, and the index SHALL complete in the time and the memory of a small project.

#### Scenario: A constant that doubles past the limit is no constant and the index completes
- **WHEN** `big.ts` exports 30 templates that each name the one before twice, from a constant that is a short text, and `use.ts` calls the 2nd and the 30th
- **THEN** `use.ts` calls the route of the 2nd, and has an unread site at the line of the call on the 30th
- **AND** the index completes and the run has no problem line

## MODIFIED Requirements

### Requirement: Constants
Where a rule reads text, a name or dotted name SHALL be read as the value of the top-level constant it names, in the same file or imported directly, by namespace or through re-exports: a `const` string, a template or concatenation of such constants or imports, a string `enum` member, a `static` class property with a text initializer, or a property at any depth of a `const` object literal. Any other name, such as a `let`, a local name or an import that leads nowhere, SHALL leave the site unread.

#### Scenario: A constant in the same file is read
- **WHEN** `src/api.ts` declares `const BASE = '/api'` and `` const USERS = `${BASE}/users` ``, and calls `fetch(USERS)` and `` fetch(`${BASE}/orders`) ``
- **THEN** the file calls `http * /api/users` and `http * /api/orders` at the lines of the two calls
- **AND** it has no unread site

#### Scenario: An imported enum member names an environment variable
- **WHEN** a package declares `@nestjs/config`, `env.ts` exports `enum Env { API_URL = 'API_URL' }`, and `app.service.ts` imports `Env` and calls `this.configService.get(Env.API_URL)`
- **THEN** `app.service.ts` calls `env API_URL` at the line of the call
- **AND** it has no unread site

#### Scenario: A static property and a nested object property are read
- **WHEN** `routes.ts` exports `class Routes { static readonly LOGIN = 'login' }` and `` export const ROUTES = { users: { byId: ':id' } } as const ``, and a class decorated `@Controller('users')` imports both and has methods decorated `@Get(Routes.LOGIN)` and `@Get(ROUTES.users.byId)`
- **THEN** the file serves `http GET /users/login` and `http GET /users/{}` at the lines of the two decorators
- **AND** it has no unread site

#### Scenario: A constant imported through re-exports or a namespace is read
- **WHEN** `paths/auth.ts` exports `const LOGIN_PATH = 'login'`, `paths/index.ts` has `export * from './auth'`, one class decorated `@Controller('/')` imports `LOGIN_PATH` from `./paths` and has a method decorated `@Get(LOGIN_PATH)`, and another imports `* as paths from './paths/auth'` and has a method decorated `@Get(paths.LOGIN_PATH)`
- **THEN** both files serve `http GET /login` at the line of their `@Get`
- **AND** neither has an unread site

#### Scenario: An imported constant path of an Express router is served under its mount
- **WHEN** `paths.ts` exports `const PATHS = { USERS: '/users' }`, `users.ts` imports it and calls `router.get(PATHS.USERS, handler)` on a `Router()` it exports by default, and `app.ts` calls `app.use('/v1', router)` with that router
- **THEN** `users.ts` serves `http GET /v1/users` at the line of the call
- **AND** it has no unread site

#### Scenario: An imported constant path of a controller is served behind the global prefix
- **WHEN** `paths.ts` exports `const LOGIN_PATH = 'login'`, a class decorated `@Controller('auth')` has a method decorated `@Get(LOGIN_PATH)`, `AppModule` lists it in `controllers`, and `main.ts` has `const app = await NestFactory.create(AppModule)` and `app.setGlobalPrefix('api')`
- **THEN** the controller's file serves `http GET /api/auth/login` at the line of `@Get`
- **AND** it serves `http GET /auth/login` nowhere and has no unread site

#### Scenario: An imported constant path on a base class is served by its controllers
- **WHEN** `UserControllerBase` has no class decorator and a method decorated `@Get(ROUTES.USER)`, where `ROUTES` is imported from a file that declares `export const ROUTES = { USER: 'user' }`, and `@Controller('users') class UserController extends UserControllerBase {}` is in another file
- **THEN** the file of the base class serves `http GET /users/user` at the line of `@Get`
- **AND** the file of the controller serves `http GET /users/user` at the line of `@Controller`
- **AND** the file of the base class has no unread site

#### Scenario: A name that is no constant leaves the site unread
- **WHEN** `api.ts` imports `LOGIN_URL` from the package `some-package`, declares `let PATH = '/users'`, and calls `fetch(PATH)` and `fetch(LOGIN_URL)`
- **THEN** the file has an unread site at the line of each call
- **AND** it calls no `http` key

#### Scenario: A constant is read wherever the parse reads text
- **WHEN** `main.ts` declares `const P = 'api'` and calls `app.setGlobalPrefix(P)` on an app made by `NestFactory.create(AppModule)`, `users.controller.ts` declares `const BASE = 'users'` and has `@Controller(BASE)` and a method decorated `@Get('x')`, `client.ts` declares `const API = 'https://api.example.com/v1'`, makes `const http = axios.create({ baseURL: API })` and calls `http.get('/x')`, and `server.ts` declares `const MOUNT = '/v2'`, calls `router.get('/y', handler)` on `express.Router()` and `app.use(MOUNT, router)`
- **THEN** `users.controller.ts` serves `http GET /api/users/x`, `client.ts` calls `http GET /v1/x`, and `server.ts` serves `http GET /v2/y`
- **AND** none of the four files has an unread site
