# Spec Delta

## ADDED Requirements

### Requirement: Constants of a function or a block
A `const` with a plain name that a function, a block, a loop, a catch clause or a class body declares SHALL be read as a constant of that scope, the way a constant of the file is, by every rule that reads text. A site SHALL read the innermost scope around it that binds the name, and a constant of a scope SHALL be read wherever in the scope it is declared, a closure above it included.

#### Scenario: A function-local constant is read
- **WHEN** `api.ts` has a function with `const path = '/a'` that calls `fetch(path)`
- **THEN** the file calls `http * /a` at the line of the call
- **AND** it has no unread site at that line

#### Scenario: A local built from another local and a constant of the file is read
- **WHEN** `api.ts` declares `const BASE = '/api'` and a function with `const base = BASE + '/x'` and `const url = base + '/y'` that calls `fetch(url)`
- **THEN** the file calls `http * /api/x/y` at the line of the call

#### Scenario: Two blocks that declare the same name read their own
- **WHEN** `api.ts` has a function whose `if` branch declares `const url = '/first'` and calls `fetch(url)`, and whose `else` branch declares `const url = '/second'` and calls `fetch(url)`
- **THEN** the file calls `http * /first` at the line of the first call and `http * /second` at the line of the second

#### Scenario: A closure reads a constant declared after it in the same scope
- **WHEN** `api.ts` has a function with `const call = () => fetch(path)` followed by `const path = '/later'`
- **THEN** the file calls `http * /later` at the line of the call

#### Scenario: An inner function reads a constant of the function around it
- **WHEN** `api.ts` has a function with `const path = '/outer'` and a function declared inside it that calls `fetch(path)`
- **THEN** the file calls `http * /outer` at the line of the call

#### Scenario: A local object constant is read by its property
- **WHEN** `api.ts` has a function with ``const paths = { users: '/users', orders: '/orders' }`` that calls `fetch(paths.users)` and `fetch(paths.orders)`
- **THEN** the file calls `http * /users` and `http * /orders` at the lines of the two calls

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
- **THEN** the file calls `http * /local` at the line of the call

#### Scenario: A let and a destructured name hide a constant of the file
- **WHEN** `api.ts` declares `const BASE = '/api'`, a function with `let BASE = '/mutable'` and a function with `const { BASE } = options`, and each calls `fetch(BASE)`
- **THEN** the file has an unread site at the line of each call
- **AND** it calls no `http` key at those lines

#### Scenario: A var in a block hides a constant of the file in the whole function
- **WHEN** `api.ts` declares `const BASE = '/api'` and a function with `if (flag) { var BASE = '/inner' }` followed by `fetch(BASE)`, and calls `fetch(BASE)` outside the function
- **THEN** the file has an unread site at the line of the call in the function
- **AND** it calls `http * /api` at the line of the call outside it

#### Scenario: A type that names a parameter hides nothing
- **WHEN** `api.ts` declares `const BASE = '/api'` and a function with `let handler: (BASE: string) => void` that calls `fetch(BASE)`, and a function with the parameter `done: (BASE: string) => void` that calls `fetch(BASE)`
- **THEN** the file calls `http * /api` at the line of each call
- **AND** it has no unread site

### Requirement: A client made in a function reads the scope it is made in
A client or a router made in a function by a call whose prefix names a constant SHALL read the name as the scope of the call binds it: a local constant gives the prefix its text, and a parameter of the same name as a constant of the file gives the client no base.

#### Scenario: A client made in a function reads the base it is given there
- **WHEN** `api.ts` declares `const BASE = '/api'`, a function with ``const base = `${BASE}/users` `` and `const api = axios.create({ baseURL: base })` that calls `api.get('/list')`, and a function with the parameter `BASE` that makes `const client = axios.create({ baseURL: BASE })` and calls `client.get('/orders')`
- **THEN** the file calls `http GET /api/users/list` at the line of the first call and `http GET /orders` at the line of the second

### Requirement: A name that a scope binds again is no imported receiver or mount
A call on a name that the file imports from a file of its project SHALL be read as a call on that client, and a `.use` on it as a mount on that app or router, only where no scope around the call binds the name again. A parameter or a variable of the same name inside a function SHALL leave the call as it is without the import, and outside that function the name SHALL stay the import.

#### Scenario: A parameter named like an imported client is no client
- **WHEN** `http.ts` exports `const api = axios.create({ baseURL: '/api' })`, and `users.ts` imports `api` from it, declares ``function list(api) { return api.get('/users') }`` and calls `api.get('/orders')`
- **THEN** the call in `list` gives no fact
- **AND** the link gives `users.ts` the call `http GET /api/orders` at the line of the outer call
- **AND** `users.ts` has no unread site

#### Scenario: A parameter named like an imported router is no mount
- **WHEN** `app.ts` exports `const app = express()`, `router.ts` exports `const router = express.Router()` and calls `router.get('/users', handler)`, and `setup.ts` imports both and declares ``function setup(app) { app.use('/api', router) }``
- **THEN** the link reads no mount, so `router.ts` serves `http GET /users` and nothing under `/api`
- **AND** `setup.ts` has no unread site

## MODIFIED Requirements

### Requirement: Constants
Where a rule reads text, a name or dotted name SHALL be read as the constant it names, at the top of the file, in the function or block around it, or imported directly, by namespace or through re-exports: a `const` string, a template or concatenation of such names, a string `enum` member, a `static` property with a text initializer, or a property at any depth of a `const` object literal. Any other name, such as a `let`, a parameter or an import that leads nowhere, SHALL leave the site unread.

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

### Requirement: Imported names that a local binds again
A name that a file imports and also binds as a parameter, a variable, a function or a class SHALL be no import for the value a rule reads in the scope that binds it, so a site there that reads it stays as it is without the import: a computed part in a template or a concatenation, and unread when the name stands alone. Outside that scope the name SHALL stay the import.

#### Scenario: A name that a local binds again is no import
- **WHEN** `config.ts` exports `const BASE = '/api/v1'`, and `users.ts` imports it, declares ``function list(BASE: string) { return fetch(`${BASE}/users`) }`` and ``function items() { return fetch(`${BASE}/items`) }``, and calls ``fetch(`${BASE}/orders`)`` and `fetch(BASE)`
- **THEN** `users.ts` calls `http * /users` at the line of the template in `list`
- **AND** the link gives `users.ts` the calls `http * /api/v1/orders`, `http * /api/v1` and `http * /api/v1/items` at the lines of the other three sites
- **AND** it has no unread site
