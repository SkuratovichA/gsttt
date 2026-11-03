## MODIFIED Requirements

### Requirement: Link and parsed facts
`link` SHALL return the routes under the prefixes their routers are mounted at, the routes a NestJS controller inherits and the tRPC procedures as new facts that name no parsed fact, so that a fact of any rule on the line of a route keeps its site. A route a NestJS controller declares SHALL be keyed again in place of its parsed fact when a global prefix reaches it, and the other facts of its line SHALL keep their sites.

#### Scenario: A fact of another rule on the line of a route is kept
- **WHEN** one line of `app.ts` has `app.get('/b', h)` on an Express app, `fetch('/c')` and a `@Get('a')` method of a `@Controller('x')` class
- **THEN** `app.ts` serves `http GET /b` and `http GET /x/a` and calls `http GET /c`, all at that line

#### Scenario: A route behind a global prefix keeps the facts of other rules on its line
- **WHEN** `main.ts` sets the global prefix `api` on an app made by `NestFactory.create(AppModule)`, `AppModule` lists `UserController` in its `controllers`, and one line of `user.controller.ts` has a `@Get(':id')` method of a `@Controller('users')` class whose body calls `fetch('/c')`
- **THEN** `user.controller.ts` serves `http GET /api/users/{}` and calls `http GET /c`, both at that line
- **AND** it serves `http GET /users/{}` nowhere

### Requirement: HTTP requests
With the `fetch` pack, `fetch` SHALL call `http <method> <url>`, the method taken from the options' `method`. With the `axios` pack, `axios.get`, `axios.post` and the other verbs, and the same verbs on a receiver made by `axios.create()`, SHALL call the URL with that verb. A URL built from a template SHALL keep its literal parts with each computed part as a parameter; a URL with no literal path, or whose path is computed in every segment, SHALL be an unread site.

#### Scenario: Requests become calls
- **WHEN** a file has ``fetch(`/bookings/${id}`, { method: 'DELETE' })`` and `axios.post('/bookings', body)`
- **THEN** it calls `http DELETE /bookings/{}` and `http POST /bookings`

#### Scenario: A base URL in a template is dropped
- **WHEN** a file has ``fetch(`${API_URL}/bookings`)``
- **THEN** it calls `http GET /bookings`

#### Scenario: A computed URL is a blind spot
- **WHEN** a file has `fetch(url)`
- **THEN** the file has an unread site at that line with the reason computed at run time

#### Scenario: A URL whose path is only computed segments is a blind spot
- **WHEN** a file has ``fetch(`${API_URL}/${id}`)`` and ``fetch(`${base}${period}/${name}`)``
- **THEN** each is an unread site at its line and the file calls nothing for them
- **AND** ``fetch(`${API_URL}/users/${id}`)`` still calls `http GET /users/{}`

#### Scenario: A client with no base URL leaves a computed URL unread
- **WHEN** a file has `const api = axios.create({ timeout: 1000 })` with ``api.get(`${base}${path}`)`` and `api.get('users')`, `` const edge = axios.create({ baseURL: `${host}` }) `` with ``edge.get(`${path}`)``, and `const v1 = axios.create({ baseURL: 'https://api.example.com/v1' })` with `v1.get('/bookings')`
- **THEN** it has an unread site at the line of ``api.get(`${base}${path}`)`` and at the line of `edge.get`
- **AND** it calls `http GET /users` and `http GET /v1/bookings`

### Requirement: URLs built with the URL class
A URL argument written `new URL(path, base)`, alone or followed by `.href`, `.pathname` or `.toString()`, SHALL be read as the URL it builds: a path that starts with `/`, starts with a computed part or holds its own origin is the URL, a relative path joins the directory of a literal base, and a base that is not a literal is a base URL that is dropped. A path that is not read SHALL leave the call an unread site.

#### Scenario: A URL built from a path and a base is called
- **WHEN** a file has `axios.post(new URL('build-runner/success', this.baseUrl).href, body)` and `fetch(new URL('/users', process.env.API_URL))`
- **THEN** it calls `http POST /build-runner/success` and `http GET /users`

#### Scenario: A URL built from a template that starts with a base URL keeps its path
- **WHEN** a file has ``fetch(new URL(`${baseUrl}/users`))`` and ``fetch(new URL(`${API}/orders`, window.location.origin))``
- **THEN** it calls `http GET /users` and `http GET /orders`

#### Scenario: A relative path joins the directory of a literal base
- **WHEN** a file has `fetch(new URL('users/' + id, 'https://api.example.com/v1/'))` and `fetch(new URL('users', 'https://api.example.com/v1'))`
- **THEN** it calls `http GET /v1/users/{}` and `http GET /users`

#### Scenario: A URL built from a computed path is an unread site
- **WHEN** a file has `fetch(new URL(path, base))`
- **THEN** the file has an unread site at that line and calls nothing for it

### Requirement: Constants
Where a rule reads text, a name or dotted name SHALL be read as the constant it names, at the top of the file, in the function or block around it, or imported directly, by namespace or through re-exports: a `const` string, a template or concatenation of such names, a string `enum` member, a `static` property with a text initializer, or a property at any depth of a `const` object literal. Any other name, such as a `let`, a parameter or an import that leads nowhere, SHALL leave the site unread.

#### Scenario: A constant in the same file is read
- **WHEN** `src/api.ts` declares `const BASE = '/api'` and `` const USERS = `${BASE}/users` ``, and calls `fetch(USERS)` and `` fetch(`${BASE}/orders`) ``
- **THEN** the file calls `http GET /api/users` and `http GET /api/orders` at the lines of the two calls
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

### Requirement: Relative requests
With the `fetch` and `axios` packs, a relative URL with no base SHALL be read as the path from the root, as a browser resolves it against the page: `fetch('items')` calls `http GET /items`. A URL with a `..` segment, or whose path starts with a computed part after a dropped `./`, SHALL be an unread site, and a target that names no path (`data:…`, `?x=1`) SHALL be no request. An absolute URL SHALL be read as before.

#### Scenario: A relative request with no base calls the rooted path
- **WHEN** a file has `fetch('items')`, `axios.get('users')`, ``fetch(`items/${id}`)``, `bare.get('users/me')` on `const bare = axios.create()` and `unread.get('users')` on `axios.create({ baseURL: BASE })` with `BASE` not a literal
- **THEN** it calls `http GET /items`, `http GET /users`, `http GET /items/{}`, `http GET /users/me` and `http GET /users`

#### Scenario: A relative request that cannot be rooted is unread
- **WHEN** a file has `fetch('../items')` and ``fetch(`./${id}`)``
- **THEN** each is an unread site at its line

#### Scenario: A request target that names no path is no request
- **WHEN** a file has `fetch('data:text/plain,x')` and `fetch('?x=1')`
- **THEN** the file has no contract site and no unread site at those lines

#### Scenario: An absolute request is unchanged by relative requests
- **WHEN** a file has `fetch('https://a.test/x')`
- **THEN** it calls `http GET /x`

### Requirement: Imported names in a template or a concatenation
A template or a concatenation that a rule reads SHALL be read with each name in it that the file imports from a file of its project as the text of the constant that file declares, directly, by namespace or through re-exports. A name that reads as no text SHALL be a computed part, as a name the file does not import is, so the site is read as it was before.

#### Scenario: A template with an imported constant is read
- **WHEN** `config.ts` exports `const BASE = '/api/v1'`, and `users.ts` imports it and calls ``fetch(`${BASE}/users`)`` and `fetch(BASE + '/orders')`
- **THEN** `users.ts` calls `http GET /api/v1/users` and `http GET /api/v1/orders` at the lines of the two calls
- **AND** it has no unread site

#### Scenario: An imported name the link cannot read keeps its reading
- **WHEN** `env.ts` has `export const BASE = process.env.API_URL`, `config.ts` imports `BASE` from it and exports `` const PATH = `${BASE}/path` ``, and `users.ts` imports both and `axios`, declares `const api = axios.create({ baseURL: BASE })`, and calls ``fetch(`${BASE}/users`)``, `fetch(BASE)`, `fetch(PATH)` and `api.get('/orders')`
- **THEN** `users.ts` calls `http GET /users` and `http GET /orders` at the lines of the first and the last call
- **AND** it has an unread site at the line of `fetch(BASE)` and at the line of `fetch(PATH)`

### Requirement: Constants built from imported names
A top-level constant SHALL be read with each name in it that its file imports as the text of the constant the exporting file declares, whether it is a template, a concatenation or a name that stands alone, for the file that declares it and for the files that import it. A constant SHALL be text only when every part of it reads, so a part that cannot be read, or a cycle of constants, leaves the site unread.

#### Scenario: An exported constant built from an imported one is read
- **WHEN** `config.ts` exports `const BASE = '/api/v1'`, `routes.ts` imports it and exports `` const USERS = `${BASE}/users` ``, `const ORDERS = BASE + '/orders'` and `` const PATHS = { items: `${BASE}/items` } ``, and `client.ts` imports the three and calls `fetch(USERS)`, `fetch(ORDERS)`, `fetch(PATHS.items)` and ``fetch(`${USERS}/1`)``
- **THEN** `client.ts` calls `http GET /api/v1/users`, `http GET /api/v1/orders`, `http GET /api/v1/items` and `http GET /api/v1/users/1` at the lines of the four calls
- **AND** it has no unread site

#### Scenario: A constant of the same file built from an imported one is read
- **WHEN** `config.ts` exports `const BASE = '/api/v1'`, and `users.ts` imports it, declares `` const USERS = `${BASE}/users` ``, `const ORDERS = BASE + '/orders'` and `const ALIAS = BASE`, and calls `fetch(USERS)`, `fetch(ORDERS)`, ``fetch(`${USERS}/1`)`` and `fetch(ALIAS)`
- **THEN** `users.ts` calls `http GET /api/v1/users`, `http GET /api/v1/orders`, `http GET /api/v1/users/1` and `http GET /api/v1` at the lines of the four calls
- **AND** it has no unread site

#### Scenario: A cycle of constants stays unread
- **WHEN** `a.ts` has `import { B } from './b'` and `` export const A = `${B}/x` ``, `b.ts` has `import { A } from './a'` and `` export const B = `${A}/y` ``, and `use.ts` imports `A` and calls `fetch(A)`
- **THEN** `use.ts` has an unread site at the line of the call
- **AND** it calls no `http` key

### Requirement: Imported names that a local binds again
A name that a file imports and also binds as a parameter, a variable, a function or a class SHALL be no import for the value a rule reads in the scope that binds it, so a site there that reads it stays as it is without the import: a computed part in a template or a concatenation, and unread when the name stands alone. Outside that scope the name SHALL stay the import.

#### Scenario: A name that a local binds again is no import
- **WHEN** `config.ts` exports `const BASE = '/api/v1'`, and `users.ts` imports it, declares ``function list(BASE: string) { return fetch(`${BASE}/users`) }`` and ``function items() { return fetch(`${BASE}/items`) }``, and calls ``fetch(`${BASE}/orders`)`` and `fetch(BASE)`
- **THEN** `users.ts` calls `http GET /users` at the line of the template in `list`
- **AND** the link gives `users.ts` the calls `http GET /api/v1/orders`, `http GET /api/v1` and `http GET /api/v1/items` at the lines of the other three sites
- **AND** it has no unread site

### Requirement: A chain of constants is read to its end
A chain of constants that name one another across files SHALL be read to its end whatever its length, within the limits on the text it puts together. The read keeps its own stack of the constants it is inside of, so the length of a chain does not overflow the stack of a worker of the link, and the link finishes.

#### Scenario: A chain of constants across two files is read to its end
- **WHEN** `a.ts` and `b.ts` export 4000 constants between them, each a template that names the next one with a suffix, the last of which is text, and `use.ts` calls `fetch` with the first
- **THEN** `use.ts` calls `http GET /end` followed by one suffix for each constant before the last, at the line of the call
- **AND** it has no unread site and the run has no problem line

### Requirement: Constants of a function or a block
A `const` with a plain name that a function, a block, a loop, a catch clause or a class body declares SHALL be read as a constant of that scope, the way a constant of the file is, by every rule that reads text. A site SHALL read the innermost scope around it that binds the name, and a constant of a scope SHALL be read wherever in the scope it is declared, a closure above it included.

#### Scenario: A function-local constant is read
- **WHEN** `api.ts` has a function with `const path = '/a'` that calls `fetch(path)`
- **THEN** the file calls `http GET /a` at the line of the call
- **AND** it has no unread site at that line

#### Scenario: A local built from another local and a constant of the file is read
- **WHEN** `api.ts` declares `const BASE = '/api'` and a function with `const base = BASE + '/x'` and `const url = base + '/y'` that calls `fetch(url)`
- **THEN** the file calls `http GET /api/x/y` at the line of the call

#### Scenario: Two blocks that declare the same name read their own
- **WHEN** `api.ts` has a function whose `if` branch declares `const url = '/first'` and calls `fetch(url)`, and whose `else` branch declares `const url = '/second'` and calls `fetch(url)`
- **THEN** the file calls `http GET /first` at the line of the first call and `http GET /second` at the line of the second

#### Scenario: A closure reads a constant declared after it in the same scope
- **WHEN** `api.ts` has a function with `const call = () => fetch(path)` followed by `const path = '/later'`
- **THEN** the file calls `http GET /later` at the line of the call

#### Scenario: An inner function reads a constant of the function around it
- **WHEN** `api.ts` has a function with `const path = '/outer'` and a function declared inside it that calls `fetch(path)`
- **THEN** the file calls `http GET /outer` at the line of the call

#### Scenario: A local object constant is read by its property
- **WHEN** `api.ts` has a function with ``const paths = { users: '/users', orders: '/orders' }`` that calls `fetch(paths.users)` and `fetch(paths.orders)`
- **THEN** the file calls `http GET /users` and `http GET /orders` at the lines of the two calls

#### Scenario: An element of an array of paths reads a constant of its scope
- **WHEN** `routes.ts` has a function with `const p = '/x'` that calls ``router.get(['/a', p], handler)`` on a `Router()` made in the file
- **THEN** the file serves `http GET /a` and `http GET /x` at the line of the call
- **AND** it has no unread site

### Requirement: Names that a scope binds hide a constant of the file
A parameter, a `let`, a `var`, a destructured name, a function, a class or a catch clause SHALL hide a constant of the file and an import of the same name in the scope that binds it, and a site there that reads the name SHALL stay as it is without the constant or the import. A `var` binds in its whole function and any other name in its block. A name that only a type mentions, such as a parameter of a function type, SHALL bind nothing.

#### Scenario: A parameter hides a constant of the file of the same name
- **WHEN** `api.ts` declares `const BASE = '/api'` and a function with the parameter `BASE` that calls `fetch(BASE)`
- **THEN** the file has an unread site at the line of the call
- **AND** it calls no `http` key at that line

#### Scenario: A local constant hides a constant of the file of the same name
- **WHEN** `api.ts` declares `const BASE = '/api'` and a function with `const BASE = '/local'` that calls `fetch(BASE)`
- **THEN** the file calls `http GET /local` at the line of the call

#### Scenario: A let and a destructured name hide a constant of the file
- **WHEN** `api.ts` declares `const BASE = '/api'`, a function with `let BASE = '/mutable'` and a function with `const { BASE } = options`, and each calls `fetch(BASE)`
- **THEN** the file has an unread site at the line of each call
- **AND** it calls no `http` key at those lines

#### Scenario: A var in a block hides a constant of the file in the whole function
- **WHEN** `api.ts` declares `const BASE = '/api'` and a function with `if (flag) { var BASE = '/inner' }` followed by `fetch(BASE)`, and calls `fetch(BASE)` outside the function
- **THEN** the file has an unread site at the line of the call in the function
- **AND** it calls `http GET /api` at the line of the call outside it

#### Scenario: A type that names a parameter hides nothing
- **WHEN** `api.ts` declares `const BASE = '/api'` and a function with `let handler: (BASE: string) => void` that calls `fetch(BASE)`, and a function with the parameter `done: (BASE: string) => void` that calls `fetch(BASE)`
- **THEN** the file calls `http GET /api` at the line of each call
- **AND** it has no unread site

## ADDED Requirements

### Requirement: A fetch with no method is a GET
A `fetch` that writes no `method`, with no options or with options that hold none, SHALL call `http GET <url>`, as the Fetch standard makes it a GET. A `fetch` whose options cannot be read, such as a variable, a spread or a `method` that is not text, SHALL call `http * <url>`.

#### Scenario: A fetch that writes no method is a GET
- **WHEN** a file has `fetch('/x')` and `fetch('/x', { headers })`
- **THEN** it calls `http GET /x` at the line of each call

#### Scenario: A fetch whose options cannot be read keeps any method
- **WHEN** a file has `fetch('/x', options)`, `fetch('/x', { method: verb })` and `fetch('/x', { ...options })`
- **THEN** it calls `http * /x` at the line of each call
