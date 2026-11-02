# Spec Delta

## ADDED Requirements

### Requirement: Arrays of route paths
A route decorator of the `nestjs` pack, and an Express verb, whose path argument is an array literal SHALL serve one route for each element, at the line of the decorator or the call. An element that is not read SHALL be an unread site at that line while the others serve, an element that is an imported constant SHALL wait for the link and serve under its value, and an empty array SHALL be an unread site.

#### Scenario: Every path of a method decorator's array serves a route
- **WHEN** `hooks.controller.ts` has `@Controller('hooks')` and a method decorated `@Post(['webhook', 'webhook/:token'])` on line 5
- **THEN** the file serves `http POST /hooks/webhook` and `http POST /hooks/webhook/{}` at line 5
- **AND** the file has no unread site

#### Scenario: A computed element of an array is an unread site and the other elements serve
- **WHEN** `hooks.controller.ts` has `@Controller('hooks')` and a method decorated `@Post(['webhook', dynamic(), 'webhook/:token'])` on line 6, where `dynamic` is an imported function
- **THEN** the file serves `http POST /hooks/webhook` and `http POST /hooks/webhook/{}` at line 6
- **AND** the file has one unread site at line 6

#### Scenario: An element of an array that is an imported constant waits for the link
- **WHEN** `hooks.controller.ts` has `@Controller('hooks')` and a method decorated `@Post(['webhook', LEGACY])` on line 6, and `LEGACY` is imported from `paths.ts`, which exports `const LEGACY = 'legacy/webhook'`
- **THEN** the file serves `http POST /hooks/webhook` at line 6 from the parse, and the link adds `http POST /hooks/legacy/webhook` at line 6
- **AND** the file has no unread site

#### Scenario: An Express verb with an array of paths serves every path
- **WHEN** `app.ts` has `const app = express()` and `app.get(['/a', '/b'], handler)` on line 4
- **THEN** the file serves `http GET /a` and `http GET /b` at line 4
- **AND** the file has no unread site

#### Scenario: An empty array of paths is an unread site
- **WHEN** `hooks.controller.ts` has `@Controller('hooks')` and a method decorated `@Post([])` on line 5
- **THEN** the file has an unread site at line 5
- **AND** the file serves no route

### Requirement: Controller prefixes in an array
A `@Controller` whose argument is an array literal SHALL serve every route of its class, and of the base class it extends, under each element that is read, once for each. An element that is not read SHALL be an unread site at the decorator while the others serve. The routes of a base class SHALL be served in the file of the base class and in the file of the controller.

#### Scenario: A controller with an array of prefixes serves every route under each
- **WHEN** a class decorated `@Controller(['a', 'b'])` has a method decorated `@Get('x')` on line 5 and one decorated `@Get(['y', 'z'])` on line 8
- **THEN** the file serves `http GET /a/x` and `http GET /b/x` at line 5
- **AND** it serves `http GET /a/y`, `http GET /a/z`, `http GET /b/y` and `http GET /b/z` at line 8

#### Scenario: A controller with a prefix that is read and one that is computed serves under the read one
- **WHEN** a class decorated `@Controller(['a', prefix()])` on line 4, where `prefix` is an imported function, has a method decorated `@Get('x')` on line 6
- **THEN** the file serves `http GET /a/x` at line 6 and no other route
- **AND** the file has an unread site at line 4

#### Scenario: A route of a base class is served under each prefix of a controller with an array
- **WHEN** `items.controller.base.ts` has a class with no class decorator and a method decorated `@Get('x')` on line 4, and `items.controller.ts` has `@Controller(['a', 'b'])` on line 4 on a class that extends it
- **THEN** `items.controller.base.ts` serves `http GET /a/x` and `http GET /b/x` at line 4
- **AND** `items.controller.ts` serves both at line 4
