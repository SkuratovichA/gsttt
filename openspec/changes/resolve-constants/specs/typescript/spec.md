# Spec Delta

## ADDED Requirements

### Requirement: Constants
Where a rule reads text, a name or dotted name SHALL be read as the value of the top-level constant it names, in the same file or imported directly, by namespace or through re-exports: a `const` string, or a template or concatenation of such constants, a string `enum` member, a `static` class property with a text initializer, or a property at any depth of a `const` object literal. Any other name, such as a `let`, a function-local name or an import that leads nowhere, SHALL leave the site unread.

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

### Requirement: Route paths that are computed
A NestJS route decorator whose path argument is not a string literal, a template or a constant SHALL be a blind spot at the line of the decorator, not a route at the controller's prefix; a route decorator with no argument SHALL still serve the prefix alone.

#### Scenario: A route whose path is computed is a blind spot
- **WHEN** a class decorated `@Controller('/')` has a method decorated `@Get(paths.login())`, where `paths` is an imported object whose `login` is a function, and another method decorated `@Get()`
- **THEN** the file has an unread site at the line of `@Get(paths.login())` and serves `http GET /` only at the line of `@Get()`

## MODIFIED Requirements

### Requirement: NestJS configuration
With the `nestjs-config` pack, `get` and `getOrThrow` on a `configService` receiver SHALL call `env <NAME>` when the first argument is a string that names a variable; a first argument that is a path into a loaded configuration, such as `database.host`, SHALL name no environment variable, and a computed first argument SHALL be a blind spot.

#### Scenario: A ConfigService read names an environment variable
- **WHEN** a package declares `@nestjs/config` and a file calls `this.configService.get<string>('API_URL', 'http://localhost')` and `configService.getOrThrow('DB_URL')`
- **THEN** the file calls `env API_URL` and `env DB_URL`

#### Scenario: A configuration path is no environment variable
- **WHEN** a file calls `this.configService.get('database.host')`
- **THEN** it calls no `env` key at that line

#### Scenario: A computed configuration name is a blind spot
- **WHEN** a file calls `this.configService.get(key)`, where `key` is a parameter of the method
- **THEN** the file has an unread site at that line

### Requirement: NestJS routes inherited from a base class
With the `nestjs` pack, a route decorator on a method of a class that has no `@Controller` decorator SHALL be served by every class decorated `@Controller('<prefix>')` that extends it by name, under that prefix, at the line of the method in the file of the base class and at the line of the `@Controller` decorator. A base class that no controller extends, and a subclass without the decorator, SHALL serve nothing, and a path on the base class that is neither a literal nor a constant SHALL be an unread site.

#### Scenario: A controller serves the routes of the base class it extends
- **WHEN** `user.controller.base.ts` has a class `UserControllerBase` with no class decorator and methods decorated `@common.Get('/:id')` and `@common.Post()`, and `user.controller.ts` imports it by name and has `@common.Controller('users') class UserController extends UserControllerBase {}`
- **THEN** `user.controller.base.ts` serves `http GET /users/{}` and `http POST /users` at the lines of the two method decorators
- **AND** `user.controller.ts` serves both at the line of `@common.Controller`

#### Scenario: A base class that no controller extends serves nothing
- **WHEN** `user.controller.base.ts` has a class with no class decorator and methods decorated `@Get(':id')` and `@Post()` that no class extends, and `orders.controller.ts` has `@Controller('orders') class OrdersController extends OrdersControllerBase`, where `OrdersControllerBase` in another file has a method decorated `@Get()`
- **THEN** `user.controller.base.ts` serves nothing and has no unread site
- **AND** `orders.controller.base.ts` and `orders.controller.ts` both serve `http GET /orders`, the first at the line of `@Get` and the second at the line of `@Controller`

#### Scenario: A subclass without the controller decorator serves no route of its base
- **WHEN** `user.controller.ts` has `@Controller('users') class UserController extends UserControllerBase {}` and `class FakeUserController extends UserControllerBase {}` with no decorator, and `UserControllerBase` has a method decorated `@Get(':id')`
- **THEN** `user.controller.base.ts` and `user.controller.ts` serve `http GET /users/{}`, the first at the line of `@Get` and the second at the line of `@Controller`
- **AND** nothing is served for `FakeUserController`

#### Scenario: A computed path on a base class is an unread site
- **WHEN** `UserControllerBase` has a method decorated `@Get(route('user'))`, where `route` is an imported function, and `@Controller('users') class UserController extends UserControllerBase {}` is in another file
- **THEN** `user.controller.base.ts` has an unread site at the line of `@Get` with the reason computed at run time
- **AND** no file serves a route for it

#### Scenario: Two controllers that extend one base class serve its routes under their own prefixes
- **WHEN** `@Controller('users') class UserController` and `@Controller('admin/users') class AdminController` both extend a base class with a method decorated `@Get(':id')`
- **THEN** the file of the base class serves `http GET /users/{}` and `http GET /admin/users/{}` at the line of `@Get`
- **AND** each controller file serves its own path at the line of its `@Controller`

## REMOVED Requirements

### Requirement: Computed route paths
**Reason**: A constant is read now, so the scenario that named a constant as a blind spot changes its meaning, and a modified requirement may not drop a scenario of the block it replaces. The requirement moves to `Route paths that are computed`, with the scenario named for what is still a blind spot.
**Migration**: A route whose path is a constant is served at that path. A route whose path is computed by a call, or is any name that is no constant, is an unread site, as before.
