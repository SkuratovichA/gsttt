# Spec Delta

## MODIFIED Requirements

### Requirement: Controller prefixes that are not read
A class decorator whose prefix is not read, such as `@Controller(PREFIX)` with `PREFIX` imported as a call that gives no text, SHALL be an unread site at its line, and its class SHALL serve no route, neither its own nor one it inherits; `@Controller()` and `@Controller({ version: '1' })` give no prefix.

#### Scenario: A controller whose prefix is not read serves nothing
- **WHEN** `auth.controller.ts` has `@Controller(PREFIX)`, with `PREFIX` imported from a file where it is `compute()`, and methods `@Get('login')` and `@Get(LOGIN)`, with `LOGIN` imported
- **AND** `user.controller.ts` has `@Controller(prefix()) class UserController extends UserControllerBase`, where `UserControllerBase` in another file has `@Get(':id')`
- **THEN** each controller file has an unread site at the line of its `@Controller` and serves no http key
- **AND** the file of the base class serves nothing

## ADDED Requirements

### Requirement: Controller prefixes from constants of other files
A class decorator whose prefix names a constant of another file SHALL be read as a method path is: a name, a property or enum member, `{ path: NAME }`, a template of imported names, or an element of an array. The class SHALL serve its routes, and those it inherits, under each text read, once each, and a prefix that stays unread SHALL be one unread site.

#### Scenario: An imported const prefix serves its routes under it
- **WHEN** `users.controller.ts` has `@Controller(USERS)`, with `USERS` imported where it is `'users'`, and methods `@Get('list')` on line 6 and `@Get(':id')` on line 9
- **THEN** the file serves `http GET /users/list` at line 6 and `http GET /users/{}` at line 9
- **AND** the file has no unread site

#### Scenario: A property, a member, an object path and a template of imported names give the prefix
- **WHEN** four controllers have `@Controller(ROUTES.orders)`, `@Controller(Paths.ITEMS)`, `@Controller({ path: USERS })` and a template of the imported `BASE` and `/v1`, each with one `@Get`
- **THEN** each serves its route under the text of its prefix
- **AND** the file has no unread site

#### Scenario: An array with an imported element serves each path once
- **WHEN** a controller has `@Controller([USERS, 'literal'])` with `USERS` imported, and a method `@Get('x')` on line 6
- **THEN** the file serves `http GET /users/x` and `http GET /literal/x` at line 6, once each
- **AND** the file has no unread site

#### Scenario: An imported method path under an imported prefix
- **WHEN** a controller has `@Controller(USERS)` and a method `@Get(LOGIN)`, with both names imported
- **THEN** the file serves `http GET /users/login` at the line of the method

#### Scenario: A base class in another file is served under an imported prefix
- **WHEN** `user.controller.base.ts` has a class with no class decorator and a method `@Get(':id')` on line 4, and `user.controller.ts` has `@Controller(USERS)`, with `USERS` imported, on line 5 on a class that extends it
- **THEN** the base file serves `http GET /users/{}` at line 4 and the controller file serves it at line 5
- **AND** the base file serves no route of its own

#### Scenario: An array with one unreadable and one readable element serves the readable one
- **WHEN** a controller has `@Controller([USERS, OTHER])` on line 4, where `USERS` is imported as `'users'` and `OTHER` as `compute()`, and a method `@Get('x')`
- **THEN** the file serves `http GET /users/x` and no other route
- **AND** the file has one unread site at line 4

#### Scenario: An imported prefix composes with the global prefix
- **WHEN** a controller has `@Controller(USERS)` and a method `@Get('x')`, and the module that lists it is created by an app on which `setGlobalPrefix('api')` is called
- **THEN** the controller file serves `http GET /api/users/x`

#### Scenario: A method path that stays unread under two prefixes is one unread site
- **WHEN** a controller has `@Controller([USERS, ADMIN])` with both names imported, a method `@Get(compute())` on line 6 and a method `@Get('x')`
- **THEN** the file serves `http GET /users/x` and `http GET /admin/x`
- **AND** the file has one unread site at line 6

#### Scenario: A changed constant moves the routes of a controller that did not change
- **WHEN** `routes.ts` changes `USERS` from `'users'` to `'people'` and `users.controller.ts` with `@Controller(USERS)` and `@Get('x')` is not touched
- **THEN** the index serves `http GET /people/x` in place of `http GET /users/x`
