# Spec Delta

## ADDED Requirements

### Requirement: NestJS routes inherited from a base class
With the `nestjs` pack, a route decorator on a method of a class that has no `@Controller` decorator SHALL be served by every class decorated `@Controller('<prefix>')` that extends it by name, under that prefix, at the line of the method in the file of the base class and at the line of the `@Controller` decorator. A base class that no controller extends, and a subclass without the decorator, SHALL serve nothing, and a path on the base class that is not a literal SHALL be an unread site.

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
- **WHEN** `UserControllerBase` has a method decorated `@Get(ROUTES.USER)`, where `ROUTES` is imported, and `@Controller('users') class UserController extends UserControllerBase {}` is in another file
- **THEN** `user.controller.base.ts` has an unread site at the line of `@Get` with the reason computed at run time
- **AND** no file serves a route for it

#### Scenario: Two controllers that extend one base class serve its routes under their own prefixes
- **WHEN** `@Controller('users') class UserController` and `@Controller('admin/users') class AdminController` both extend a base class with a method decorated `@Get(':id')`
- **THEN** the file of the base class serves `http GET /users/{}` and `http GET /admin/users/{}` at the line of `@Get`
- **AND** each controller file serves its own path at the line of its `@Controller`

### Requirement: NestJS global prefix
With the `nestjs` pack, `app.setGlobalPrefix('<literal>')` on an app made by `NestFactory.create(<Root>)` in the same file SHALL put every controller that `<Root>` reaches through the `imports` and `controllers` of `@Module` classes behind the prefix, the routes it inherits included. A controller that no such app reaches SHALL keep its path, and a prefix that is not a literal SHALL be an unread site.

#### Scenario: A global prefix reaches the controllers of imported modules
- **WHEN** `main.ts` has `const app = await NestFactory.create(AppModule)` and `app.setGlobalPrefix('api')`, `app.module.ts` has `@Module({ imports: [UserModule, ConfigModule.forRoot()] }) class AppModule`, `user.module.ts` has `@Module({ controllers: [UserController] }) class UserModule`, and `user.controller.ts` has `@Controller('users')` with a method decorated `@Get(':id')`
- **THEN** `user.controller.ts` serves `http GET /api/users/{}` at the line of `@Get` and `http GET /users/{}` nowhere

#### Scenario: A global prefix reaches the routes a controller inherits
- **WHEN** the modules and `main.ts` are as above, `UserController` is `@Controller('users') class UserController extends UserControllerBase {}`, and `UserControllerBase` in another file has a method decorated `@Get('/:id')`
- **THEN** the file of the base class and `user.controller.ts` both serve `http GET /api/users/{}`, the first at the line of `@Get` and the second at the line of `@Controller`
- **AND** neither serves `http GET /users/{}`

#### Scenario: Modules that import each other through forwardRef keep the prefix
- **WHEN** `AppModule` imports `AuthModule`, `AuthModule` imports `forwardRef(() => UserModule)` and lists `AuthController`, `UserModule` imports `forwardRef(() => AuthModule)` and lists `UserController`, `AuthController` is `@Controller()` with `@Post('login')`, `UserController` is `@Controller('users')` with `@Get(':id')`, and `main.ts` sets the global prefix `api`
- **THEN** the controllers serve `http POST /api/login` and `http GET /api/users/{}`
- **AND** no route is served under a path that goes round the cycle

#### Scenario: A controller that no app reaches keeps its path
- **WHEN** the modules and `main.ts` are as in the first scenario, and `legacy.controller.ts` has `@Controller('legacy')` with `@Get(':id')` and is listed by a `LegacyModule` that neither `AppModule` nor any module it reaches imports
- **THEN** `legacy.controller.ts` serves `http GET /legacy/{}`
- **AND** `user.controller.ts` still serves `http GET /api/users/{}`

#### Scenario: A controller in a ring of modules that no app reaches keeps its path
- **WHEN** `AModule` imports `forwardRef(() => BModule)` and lists `AController`, `BModule` imports `forwardRef(() => AModule)` and lists `BController`, `AController` is `@Controller('a')` with `@Get(':id')`, `BController` is `@Controller('b') class BController extends BControllerBase {}` where `BControllerBase` has a method decorated `@Get()`, no app or module reaches either module, and `main.ts` sets the global prefix `api` on an app made of `AppModule`
- **THEN** the controllers serve `http GET /a/{}` and `http GET /b`, `BControllerBase` serves `http GET /b`, and no route is served under `/api`

#### Scenario: A controller that a prefixed app and an unprefixed module both reach is served under both
- **WHEN** `main.ts` makes an app of `AppModule` and sets the global prefix `api`, `worker.ts` makes an app of `WorkerModule` and sets no prefix, both modules import `UserModule`, and `UserModule` lists `UserController`, which is `@Controller('users')` with a method decorated `@Get(':id')`
- **THEN** `user.controller.ts` serves `http GET /api/users/{}` and `http GET /users/{}`, both at the line of `@Get`

#### Scenario: A computed global prefix is an unread site
- **WHEN** `main.ts` has `app.setGlobalPrefix(prefix)` where `prefix` is a variable, on an app made by `NestFactory.create(AppModule)`
- **THEN** `main.ts` has an unread site at that line with the reason computed at run time
- **AND** the controllers `AppModule` reaches serve their paths as written

### Requirement: NestJS global prefixes that are not read
With the `nestjs` pack, a `setGlobalPrefix` call on an app that `NestFactory.create(<name>)` does not make in the same file, or whose root module is not a plain name, SHALL be an unread site and apply no prefix. A call that passes options SHALL apply its prefix and also be an unread site, because the routes the options exclude are keyed behind the prefix.

#### Scenario: A global prefix on an app made elsewhere is an unread site
- **WHEN** `setup.ts` has `function configure(app: INestApplication) { app.setGlobalPrefix('api'); }`, and `main.ts` has `const app = await NestFactory.create(AppModule.forRoot())` and `app.setGlobalPrefix('api')`
- **THEN** `setup.ts` and `main.ts` each have an unread site at the line of the call
- **AND** the controllers `AppModule` reaches serve their paths as written

#### Scenario: The options of a global prefix are an unread site
- **WHEN** `main.ts` has `app.setGlobalPrefix('api', { exclude: ['health'] })` on an app made by `NestFactory.create(AppModule)`
- **THEN** `main.ts` has an unread site at that line
- **AND** the controllers `AppModule` reaches serve their paths behind `/api`

## MODIFIED Requirements

### Requirement: Link and parsed facts
`link` SHALL return the routes under the prefixes their routers are mounted at, the routes a NestJS controller inherits and the tRPC procedures as new facts that name no parsed fact, so that a fact of any rule on the line of a route keeps its site. A route a NestJS controller declares SHALL be keyed again in place of its parsed fact when a global prefix reaches it, and the other facts of its line SHALL keep their sites.

#### Scenario: A fact of another rule on the line of a route is kept
- **WHEN** one line of `app.ts` has `app.get('/b', h)` on an Express app, `fetch('/c')` and a `@Get('a')` method of a `@Controller('x')` class
- **THEN** `app.ts` serves `http GET /b` and `http GET /x/a` and calls `http * /c`, all at that line

#### Scenario: A route behind a global prefix keeps the facts of other rules on its line
- **WHEN** `main.ts` sets the global prefix `api` on an app made by `NestFactory.create(AppModule)`, `AppModule` lists `UserController` in its `controllers`, and one line of `user.controller.ts` has a `@Get(':id')` method of a `@Controller('users')` class whose body calls `fetch('/c')`
- **THEN** `user.controller.ts` serves `http GET /api/users/{}` and calls `http * /c`, both at that line
- **AND** it serves `http GET /users/{}` nowhere
