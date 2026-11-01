# Spec Delta

## ADDED Requirements

### Requirement: NestJS routes inherited from a base class
With the `nestjs` pack, a route decorator on a method of a class that has no `@Controller` decorator SHALL be served by every class decorated `@Controller('<prefix>')` that extends it by name, under that prefix, at the line of the method in the file of the base class and at the line of the `@Controller` decorator. A base class that no controller extends SHALL serve nothing.

#### Scenario: A controller serves the routes of the base class it extends
- **WHEN** `user.controller.base.ts` has a class `UserControllerBase` with no class decorator and methods decorated `@common.Get('/:id')` and `@common.Post()`, and `user.controller.ts` imports it by name and has `@common.Controller('users') class UserController extends UserControllerBase {}`
- **THEN** `user.controller.base.ts` serves `http GET /users/{}` and `http POST /users` at the lines of the two method decorators
- **AND** `user.controller.ts` serves both at the line of `@common.Controller`

#### Scenario: A base class that no controller extends serves nothing
- **WHEN** a file has a class with no class decorator and methods decorated `@Get(':id')` and `@Post()`, and no class decorated `@Controller` extends it
- **THEN** the file serves nothing and has no unread site

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
- **WHEN** the modules and `main.ts` are as in the first scenario, and `legacy.controller.ts` has `@Controller('legacy')` with `@Get(':id')` and is listed by a module that no module `AppModule` imports lists
- **THEN** `legacy.controller.ts` serves `http GET /legacy/{}`
- **AND** `user.controller.ts` still serves `http GET /api/users/{}`

#### Scenario: A computed global prefix is an unread site
- **WHEN** `main.ts` has `app.setGlobalPrefix(prefix)` where `prefix` is a variable, on an app made by `NestFactory.create(AppModule)`
- **THEN** `main.ts` has an unread site at that line with the reason computed at run time
- **AND** the controllers `AppModule` reaches serve their paths as written
