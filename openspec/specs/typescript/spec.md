# typescript Specification

## Purpose
The TypeScript connector reads TypeScript and JavaScript into imports and contract sites. It parses with oxc, resolves specifiers the way TypeScript and Node would inside the repository, and finds HTTP routes and requests, GraphQL operations, tRPC procedures, tables and environment variables through rule packs.

## Requirements

### Requirement: Projects
The connector SHALL open one project per `package.json` that sits at the repository root, names a package, declares dependencies or lists workspaces, and one at the root when code lies under no project. A project SHALL claim the files with the extensions `ts`, `tsx`, `mts`, `cts`, `js`, `jsx`, `mjs` and `cjs` under its root, and its own `package.json` and `tsconfig*.json` files, which it tags as manifests.

#### Scenario: Every package is a project
- **WHEN** a workspace has `package.json`, `apps/web/package.json` and `packages/ui/package.json`
- **THEN** the connector opens projects rooted at the repository root, `apps/web` and `packages/ui`
- **AND** `packages/ui/src/button.tsx` belongs to `packages/ui`

#### Scenario: A marker package.json is no project
- **WHEN** `packages/app/package.json` names `app` and declares `react` and `express`, `packages/app/src/esm/package.json` holds only `{"type": "module"}`, the root manifest declares `lodash`, and `packages/app/src/esm/entry.ts` imports `react`, `lodash`, `uuid` and `left-pad`
- **THEN** the projects are the root and `packages/app`, and `entry.ts` belongs to `packages/app`
- **AND** `react` and `lodash` resolve to those packages, `uuid` to the package `uuid` through the root's `@types/uuid`, and `left-pad` to an unknown package
- **AND** the `express` pack is active in `packages/app`, so `packages/app/src/esm/server.ts` serves `http GET /health`

### Requirement: Project configs
The connector SHALL read the `tsconfig.json` or `jsconfig.json` nearest to a file and every config it extends or references; a config it cannot read SHALL be recorded as that file's parse failure, and the project SHALL go on with the options it could read. A config with `references` SHALL hand a file to the first referenced config that covers it by `files`, `include` and `exclude`.

#### Scenario: A broken tsconfig fails alone
- **WHEN** `apps/web/tsconfig.json` is not valid JSON with comments
- **THEN** that file's status is a syntax failure with its line
- **AND** the other files of `apps/web` are parsed and resolved

#### Scenario: A config extends another
- **WHEN** `apps/web/tsconfig.json` extends `../../tsconfig.base.json`
- **THEN** `apps/web/tsconfig.json` has an import that resolves to `tsconfig.base.json`

#### Scenario: A solution tsconfig hands a file to the config that lists it
- **WHEN** the root `tsconfig.json` has `"files": []` and references `tsconfig.app.json`, which maps `@/*` to `./src/*` and includes `src`, and `tsconfig.node.json`, which maps `@/*` to `./tooling/*` and includes `vite.config.ts`
- **THEN** `@/components/Button` imported from `src/main.ts` resolves to `src/components/Button.ts`
- **AND** `@/Plugin` imported from `vite.config.ts` resolves to `tooling/Plugin.ts`

#### Scenario: A jsconfig governs a directory without a tsconfig
- **WHEN** `legacy/` has a `jsconfig.json` that maps `~/*` to `./lib/*` and no tsconfig, and `legacy/main.js` imports `~/util`
- **THEN** the import resolves to `legacy/lib/util.js`

### Requirement: Imports
The connector SHALL read static imports, type-only imports, side-effect imports, re-exports, `export *` re-exports, `import()` and `require()` with a literal specifier, `import x = require()` and triple-slash path references, each with its line, its kind and the names it imports, and SHALL flag type-only imports. An `import()` or `require()` whose specifier is not a literal SHALL be recorded as an unread site computed at run time.

#### Scenario: Every import form is read
- **WHEN** a file holds an import of each form listed above, one of them naming a file of the repository
- **THEN** its facts hold each import with its kind, line, names and target, matching the reviewed snapshot

#### Scenario: Type-only imports are flagged
- **WHEN** a file has `import type { A } from './a'` and `import { type B, C } from './b'`
- **THEN** the first import is flagged type-only and the second is not

#### Scenario: A computed import is a blind spot
- **WHEN** a file has `await import(name)`
- **THEN** the file has an unread site at that line with the reason computed at run time

### Requirement: Parse status
A file with syntax errors from which oxc recovered SHALL be recorded as recovered with the number of errors and the line of the first, keeping what was read; a file oxc could not read at all SHALL be recorded as a syntax failure with the line and the message.

#### Scenario: A recoverable error keeps the imports
- **WHEN** a file imports `./a` on line 1 and has a stray `)` on line 3
- **THEN** its status is recovered with one error at line 3 and its facts still hold the import of `./a`

### Requirement: Test files
Files named `*.test.*` or `*.spec.*`, and files under a directory named `__tests__`, `test` or `tests`, SHALL be tagged as tests.

#### Scenario: Test files are tagged
- **WHEN** the project holds `src/a.test.ts`, `src/__tests__/b.ts`, `test/c.ts` and `src/d.ts`
- **THEN** the first three are tagged as tests and `src/d.ts` is not

### Requirement: Resolution
The connector SHALL resolve specifiers against the repository's file list, asking the disk only about a target it found missing. An extensionless specifier SHALL try `.ts`, `.tsx`, `.d.ts`, `.js` and `.jsx`, then a directory index, never `.mts`, `.cts`, `.mjs` or `.cjs`; `.`, `..` and a trailing `/` name directories only. A built extension SHALL find the source it may come from: `.js` a `.ts`, `.tsx`, `.d.ts` or `.jsx`, `.jsx` a `.tsx`, `.mjs` a `.mts` and `.cjs` a `.cts`.

#### Scenario: Relative specifiers find TypeScript sources
- **WHEN** `src/a.ts` imports `./util`, `./util.js` and `./components`, and the files `src/util.ts` and `src/components/index.tsx` exist
- **THEN** the first two resolve to `src/util.ts` and the third to `src/components/index.tsx`

### Requirement: Aliases and packages
A specifier SHALL follow `paths` and `baseUrl` from the config chain of the importing file, the longest matching prefix winning and the first listed among equals, and a workspace package by name through `exports`, `types`, `module` and `main`, mapping an entry under the package's `outDir` back to its source under `rootDir`.

#### Scenario: Path aliases follow the config chain
- **WHEN** `tsconfig.base.json` at the root maps `@db/*` to `packages/db/src/*`, `apps/web/tsconfig.json` extends `../../tsconfig.base.json`, and `apps/web/src/page.ts` imports `@db/client`
- **THEN** the import resolves to `packages/db/src/client.ts`, because a mapping is read relative to the config that declares it

#### Scenario: Workspace packages resolve to their sources
- **WHEN** `packages/ui/package.json` is named `@acme/ui` with `exports` mapping `./button` to `./src/button.tsx`, and `apps/web` imports `@acme/ui/button`
- **THEN** the import resolves to `packages/ui/src/button.tsx`

#### Scenario: Built entries map back to sources
- **WHEN** the workspace package `@acme/db` has `main` set to `dist/index.js`, its tsconfig has `outDir` `dist` and `rootDir` `src`, and another package imports `@acme/db`
- **THEN** the import resolves to `packages/db/src/index.ts`

### Requirement: Package exports
A workspace package's `exports` and a manifest's `imports` SHALL expose only what they list: a `null` target and a subpath that no key matches SHALL be not found. The first key in written order whose condition is active SHALL decide; `types`, `default` and the configured conditions are active, and so is `import`, or `require` for a `require()` call. A `#` import whose target is a Node builtin SHALL resolve to a builtin.

#### Scenario: An exports map hides what it does not list
- **WHEN** the workspace package `lib` has `exports` mapping `./feature/*` to `./src/feature/*.ts` and setting `./internal/*` and `./hidden` to `null`, and `apps/web` imports `lib/feature/one`, `lib/internal/secret`, `lib/hidden` and `lib/unlisted`, all of which exist as files in `packages/lib/src`
- **THEN** the first import resolves to `packages/lib/src/feature/one.ts` and the other three are not found

#### Scenario: Conditions follow the form of the import
- **WHEN** `lib` maps `.` to `{"require": "./src/cjs.ts", "import": "./src/esm.ts"}`, `typed` maps `.` to `{"types": "./src/types.d.ts", "default": "./src/runtime.ts"}`, and `apps/web/package.json` has `imports` mapping `#dep` to `{"import": "./src/dep-esm.ts", "default": "./src/dep.ts"}`, `#fs` to `node:fs` and `#utils/*` to `./src/utils/*.ts`
- **THEN** `import ... from 'lib'` resolves to `packages/lib/src/esm.ts` and `require('lib')` to `packages/lib/src/cjs.ts`
- **AND** `typed` resolves to `packages/typed/src/types.d.ts`, `#dep` to `apps/web/src/dep-esm.ts`, `#fs` to a builtin and `#utils/format` to `apps/web/src/utils/format.ts`

### Requirement: Ignored targets
A relative or aliased specifier that finds no file in the list but names a file on disk SHALL be unresolved as ignored, not as not found; a target that is absent, or a directory without an index, SHALL be not found. A file that a code generator will write cannot be told from a missing one, so it is not found.

#### Scenario: An import of an ignored file says so
- **WHEN** `generated/client.ts` exists on disk but an ignore file keeps it out of the file list, `tsconfig.json` maps `@gen/*` to `generated/*`, and `src/main.ts` imports `../generated/client`, `@gen/client`, `./absent` and `./folder`, a directory without an index
- **THEN** the first two are unresolved as ignored
- **AND** the other two are unresolved as not found

### Requirement: Packages and builtins
A specifier naming a package declared in the manifest of the project or of any package around it SHALL resolve to that package, a Node builtin to a builtin, a bare name declared nowhere to an unknown package, and a path that names no file to not found.

#### Scenario: Packages, builtins and unknown names are told apart
- **WHEN** `apps/web` declares `react` and `react-dom` and imports `react`, `react-dom/client`, `node:fs`, `path`, `left-pad` and `./nope`
- **THEN** `react` and `react-dom/client` resolve to the packages `react` and `react-dom`, `node:fs` and `path` to builtins, `left-pad` to an unknown package and `./nope` to not found

### Requirement: HTTP routes
With the `express` pack, `get`, `post`, `put`, `patch`, `delete`, `options`, `head` and `all` called with a path on a receiver made by `express()`, `express.Router()` or `Router()` SHALL serve `http <METHOD> <path>`. A call with a path alone, such as `app.get('env')`, reads a setting and SHALL serve nothing.

#### Scenario: An Express route is served
- **WHEN** a file has `const app = express(); app.get('/health', h)`
- **THEN** it serves `http GET /health` at that line

#### Scenario: A call on another receiver is no route
- **WHEN** a file has `cache.get('/health')` and `cache` is not made by Express
- **THEN** the file has no contract site at that line

#### Scenario: A setting getter is no route
- **WHEN** a file has `const app = express()`, `const env = app.get('env')` and `app.get('/ping', h)`
- **THEN** it serves `http GET /ping` and nothing else

### Requirement: Express mounts
A router mounted with `use('<prefix>', router)` SHALL have its routes served under the prefix, through any number of mounts and across files. A mount path given as an array SHALL mount the router under each literal element and record any other element as an unread site, and a cycle of mounts SHALL be cut where it closes.

#### Scenario: A router mounted in another file serves under the mount
- **WHEN** `bookings.ts` has `const router = Router(); router.get('/:id', h); export default router`, `api.ts` has `const api = Router(); api.use('/bookings', bookings)` with `bookings` imported from `./bookings`, and `app.ts` has `app.use('/api', api)` with `api` imported from `./api`
- **THEN** `bookings.ts` serves `http GET /api/bookings/{}` at the line of `router.get`

#### Scenario: A router mounted under several paths serves under each
- **WHEN** `app.ts` has `app.use('/v1', users)`, `app.use('/v2', users)`, `app.use(extras)`, `app.use(['/a', '/b'], pair)` and `app.use(['/c', dynamic], mixed)` where `dynamic` is a variable, and each router has one `get`
- **THEN** `users` serves under `/v1` and `/v2`, `extras` without a prefix, `pair` under `/a` and `/b`, and `mixed` under `/c`
- **AND** `app.ts` has an unread site at the line of the array holding `dynamic`, with the reason computed at run time

#### Scenario: A mount cycle is cut where it closes
- **WHEN** `app.ts` has `app.use('/api', a)`, `a.ts` has `a.use('/b', b)` and `a.get('/x', h)`, and `b.ts` has `b.use('/a', a)` and `b.get('/y', h)`
- **THEN** `a.ts` serves `http GET /api/x` and `b.ts` serves `http GET /api/b/y`
- **AND** no route is served under a path that goes round the cycle again

### Requirement: Routes under every mount
A route SHALL be served once under each mount of its router and under no other path. A mount with no path is a mount at the root, so it serves the route bare, and a router that nothing mounts serves its routes as written. Routes on one line SHALL each be keyed on their own.

#### Scenario: A router mounted at the root serves without a prefix
- **WHEN** `app.ts` has `const app = express(); const router = Router(); app.use(router); router.get('/x', h)`
- **THEN** `app.ts` serves `http GET /x` at the line of `router.get` and nothing else

#### Scenario: A router mounted at the root and under a prefix serves under both
- **WHEN** `app.ts` has `app.use('/api', router)`, `app.use(router)` and `router.get('/x', h)`
- **THEN** `app.ts` serves `http GET /api/x` and `http GET /x`, both at the line of `router.get`

#### Scenario: Routes on one line are keyed apart
- **WHEN** `app.ts` has `router.get('/a', h); router.post('/b', h);` on one line and `app.use('/api', router)`
- **THEN** `app.ts` serves `http GET /api/a` and `http POST /api/b` at that line
- **AND** it serves neither `http GET /a` nor `http POST /b`

### Requirement: Link and parsed facts
`link` SHALL return the routes under the prefixes their routers are mounted at, the routes a NestJS controller inherits and the tRPC procedures as new facts that name no parsed fact, so that a fact of any rule on the line of a route keeps its site. A route a NestJS controller declares SHALL be keyed again in place of its parsed fact when a global prefix reaches it, and the other facts of its line SHALL keep their sites.

#### Scenario: A fact of another rule on the line of a route is kept
- **WHEN** one line of `app.ts` has `app.get('/b', h)` on an Express app, `fetch('/c')` and a `@Get('a')` method of a `@Controller('x')` class
- **THEN** `app.ts` serves `http GET /b` and `http GET /x/a` and calls `http GET /c`, all at that line

#### Scenario: A route behind a global prefix keeps the facts of other rules on its line
- **WHEN** `main.ts` sets the global prefix `api` on an app made by `NestFactory.create(AppModule)`, `AppModule` lists `UserController` in its `controllers`, and one line of `user.controller.ts` has a `@Get(':id')` method of a `@Controller('users')` class whose body calls `fetch('/c')`
- **THEN** `user.controller.ts` serves `http GET /api/users/{}` and calls `http GET /c`, both at that line
- **AND** it serves `http GET /users/{}` nowhere

### Requirement: NestJS routes
With the `nestjs` pack, `@Get`, `@Post`, `@Put`, `@Patch`, `@Delete`, `@Options`, `@Head` and `@All` on a method of a `@Controller('<prefix>')` class SHALL serve the prefix and the path joined, also when they are called through a namespace import (`@common.Get`).

#### Scenario: A NestJS controller serves its routes
- **WHEN** a class decorated `@Controller('bookings')` has a method decorated `@Get(':id')`
- **THEN** the file serves `http GET /bookings/{}` at the line of `@Get`

#### Scenario: A NestJS controller imported as a namespace serves its routes
- **WHEN** a file has `import * as common from '@nestjs/common'` and a class decorated `@common.Controller('users')` has methods decorated `@common.Get('/:id')` and `@common.Post()`
- **THEN** the file serves `http GET /users/{}` and `http POST /users` at the lines of the decorators

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

### Requirement: GraphQL
With the `nestjs` pack, `@Query`, `@Mutation` and `@Subscription` on a method SHALL serve `graphql <Root>.<name>`, the name taken from the decorator's `name` option or else the method's name, also when they are called through a namespace import (`@graphql.Query`).

#### Scenario: A resolver method serves a root field
- **WHEN** a `@Resolver()` class has `@Query(() => [Booking]) bookings()` and `@Mutation(() => Booking, { name: 'cancelBooking' }) cancel()`
- **THEN** the file serves `graphql Query.bookings` and `graphql Mutation.cancelBooking`

#### Scenario: A resolver imported as a namespace serves its root fields
- **WHEN** a file has `import * as graphql from '@nestjs/graphql'` and a `@graphql.Resolver()` class has `@graphql.Query(() => [User]) users()`, `@graphql.Mutation(() => User, { name: 'removeUser' }) remove()` and `@graphql.ResolveField(() => Org) org()`
- **THEN** the file serves `graphql Query.users` and `graphql Mutation.removeUser` and nothing for `org`

### Requirement: GraphQL documents and resolver maps
With the `graphql` pack, a template tagged `gql` or `graphql`, and the first argument of `graphql()`, SHALL be an embedded `graphql` document at its line, and each property of a `Query`, `Mutation` or `Subscription` object literal inside an object literal bound to `resolvers` or passed as the `resolvers` option SHALL serve `graphql <Root>.<property>`.

#### Scenario: A resolver map serves its fields
- **WHEN** a file has `const resolvers = { Query: { bookings: () => list() }, Mutation: { cancel } }`
- **THEN** it serves `graphql Query.bookings` and `graphql Mutation.cancel` at the lines of the properties

#### Scenario: A tagged template is an embedded document
- **WHEN** a file has `` const Q = gql`query Bookings { bookings { id } }` `` starting on line 5
- **THEN** the file embeds a `graphql` document at line 5 with that text

### Requirement: tRPC
With the `trpc` pack, a property of an object passed to a router builder (`router`, `t.router`, `createTRPCRouter`) whose value ends in `.query`, `.mutation` or `.subscription`, or names a variable that holds such a call, SHALL serve `trpc <path>`, where the path joins the property names from the router no other router includes, through routers in other files. A spread or member entry SHALL be recorded as an unread site.

#### Scenario: Composed routers serve full paths
- **WHEN** `booking.ts` exports `bookingRouter = router({ get: publicProcedure.input(s).query(f) })`, and `root.ts` has `appRouter = router({ booking: bookingRouter })`
- **THEN** `booking.ts` serves `trpc booking.get` at the line of the `get` property

#### Scenario: A procedure held in a variable is served
- **WHEN** `app.ts` has `const list = publicProcedure.query(f)`, `const remove = publicProcedure.mutation(g)` and `router({ list, remove, ...other, nested: other.router, inline: publicProcedure.query(h) })`
- **THEN** it serves `trpc list`, `trpc remove` and `trpc inline` at the lines of their properties
- **AND** the spread and `other.router` entries are unread sites at their lines

### Requirement: tRPC calls
With the `trpc` pack, member chains from `trpc` or `api` ending in a client call (`useQuery`, `useSuspenseQuery`, `useInfiniteQuery`, `useSuspenseInfiniteQuery`, `useMutation`, `useSubscription`, `query`, `mutate`, `subscribe`, `prefetch`, `prefetchInfinite`, `fetch`, `fetchInfinite`, `invalidate`, `refetch`, `cancel`, `setData`, `getData`, `ensureData`) SHALL call `trpc <path>`.

#### Scenario: A client call names its procedure
- **WHEN** a component has `api.booking.get.useQuery({ id })`
- **THEN** it calls `trpc booking.get`

### Requirement: Tables
With the `prisma` pack, a member chain `<root>.<model>` followed by a client method SHALL call `table <model>`, where the root is `prisma`, `db`, `tx`, `this.prisma`, `this.prismaService`, `this.db`, `ctx.prisma` or `ctx.db`. With the `typeorm` pack, `@Entity('<name>')` SHALL declare `table <name>` and a bare `@Entity()` the class name. With the `drizzle` pack, `pgTable`, `mysqlTable` and `sqliteTable` SHALL declare `table <first argument>`.

#### Scenario: A Prisma call names its table
- **WHEN** a file has `prisma.bookingItem.findMany()`
- **THEN** it calls `table bookingitem`

#### Scenario: ORM declarations declare tables
- **WHEN** one file has `@Entity('bookings') class Booking {}` and another `export const items = pgTable('booking_items', {…})`
- **THEN** the first declares `table bookings` and the second `table booking_items`

### Requirement: Environment variables
With the `env` pack, `process.env.<NAME>`, `process.env['<NAME>']`, `import.meta.env.<NAME>` and each name destructured from `process.env` SHALL call `env <NAME>`.

#### Scenario: Every way of reading the environment is seen
- **WHEN** a file reads `process.env.A`, `process.env['B']`, `import.meta.env.VITE_C` and has `const { D, E: renamed } = process.env`
- **THEN** it calls `env A`, `env B`, `env VITE_C`, `env D` and `env E`

### Requirement: Pack activation
A pack SHALL be active in a project when the manifest of the project or of any package around it declares one of the pack's `detect` packages in any dependency section, or always when its `detect` is empty. Each project SHALL report what it provides, with each active pack under its kind and role and the connector's blind spots in words.

#### Scenario: Packs follow the manifest
- **WHEN** `apps/api` declares `@nestjs/common` and `apps/web` declares neither `@nestjs/common` nor `express`
- **THEN** NestJS decorators in `apps/web` produce no sites and the same decorators in `apps/api` do

#### Scenario: Blind spots are reported
- **WHEN** the coverage of a TypeScript project is read
- **THEN** its provides list the active packs and name, among the blind spots, file-system routes and routers mounted from another package

### Requirement: Imports in types
The connector SHALL read `typeof import('x')` and `import('x').T` in a type as a static import of `x` flagged type-only, at the line of the specifier. `typeof import('x')` SHALL name `*`, as a namespace import does, and `import('x').A.B` and `typeof import('x').A` SHALL name `A`, the first name of the qualifier.

#### Scenario: A type query of a module is a type-only import
- **WHEN** `src/types.ts` has `export type Lib = typeof import('./lib')` on line 1 and `export type Helper = typeof import('./lib').helper` on line 2, and `src/lib.ts` exists
- **THEN** `src/types.ts` has two static imports of `./lib`, both flagged type-only, the first at line 1 naming `*` and the second at line 2 naming `helper`
- **AND** both resolve to `src/lib.ts`

#### Scenario: An import type names the first segment of its qualifier
- **WHEN** `src/types.ts` has `export type Editor = import('./lib').Editor` on line 1 and `export type Inner = import('./lib').Outer.Inner` on line 2
- **THEN** the file has static imports of `./lib` flagged type-only, naming `Editor` at line 1 and `Outer` at line 2

#### Scenario: A nested import type is read at the line of its specifier
- **WHEN** `src/types.ts` has `typeof import(` on line 1 and `'./lib'` on line 2, `import('./lib').Box<typeof import('./other')>` on line 4 and `Promise<typeof import('./lib')>` on line 5
- **THEN** the file has type-only static imports of `./lib` at lines 2 and 5 naming `*`, and at line 4 one of `./lib` naming `Box` and one of `./other` naming `*`

### Requirement: Query suffixes
A specifier with a query, such as `./terms.md?raw`, `./logo.svg?url` or `ui-kit/theme.css?inline`, SHALL resolve as the same specifier without the query does, and the import SHALL keep the specifier as written.

#### Scenario: A query does not change where an import leads
- **WHEN** `src/page.ts` imports `../docs/terms.md?raw`, `./worker?worker`, `./logo.svg?url`, `ui-kit/theme.css?inline`, `../generated/schema.json?url`, `./absent.md?raw` and `ui-kit?raw`, and each of them again without its query, where `docs/terms.md`, `src/worker.ts` and `src/logo.svg` exist, `ui-kit` is declared in `package.json`, `generated/schema.json` exists on disk but an ignore file keeps it out of the file list, and `absent.md` does not exist
- **THEN** each import with a query leads where the same specifier without the query leads
- **AND** the first three resolve to `docs/terms.md`, `src/worker.ts` and `src/logo.svg`, the fourth and the seventh to the package `ui-kit`, the fifth is unresolved as ignored and the sixth as not found
- **AND** the facts keep the specifiers with their queries

### Requirement: Aliases of this
A chain rule SHALL read a chain that starts at a name bound by `const <name> = this` or `let <name> = this`, whatever the name, as a chain from `this`. The alias SHALL hold for the whole file, and a name that the file binds to anything else anywhere in it, as a variable, a parameter, a function, a class or an import, SHALL be no alias.

#### Scenario: An alias of this reaches the Prisma client
- **WHEN** `src/tenants.ts` has a class with the field `prisma = new PrismaClient()`, a method that binds `const self = this` and calls `self.prisma.tenant.findUnique({ where: { id } })` on line 9, a method that binds `let that = this` and calls `that.prisma.booking.count()` on line 15, and a function that binds `const holder = shared` and calls `holder.prisma.invoice.findMany()`
- **THEN** the file calls `table tenant` at line 9 and `table booking` at line 15
- **AND** it calls no other table

#### Scenario: A name bound to anything else is no alias of this
- **WHEN** `src/self-alias.ts` has a class with the field `prisma = new PrismaClient()` and a method that binds `const self = this` and calls `self.prisma.tenant.findFirst()`, and a function with the parameter `self: Mock` that calls `self.prisma.tenant.findMany()`
- **AND** `src/consistent.ts` has a class with the field `prisma = new PrismaClient()`, a method that binds `const self = this` and calls `self.prisma.tenant.findFirst()` on line 8, and a method that binds `let self = this` and calls `self.prisma.booking.count()` on line 13
- **AND** `src/earlier.ts` has a function with the parameter `self` and no type that calls `self.prisma.tenant.findMany()`, before a class with the field `prisma = new PrismaClient()` and a method that binds `const self = this` and calls `self.prisma.tenant.findFirst()`
- **THEN** `src/self-alias.ts` and `src/earlier.ts` call no table
- **AND** `src/consistent.ts` calls `table tenant` at line 8 and `table booking` at line 13

### Requirement: Roots by type
A chain rule MAY list root types. A parameter, a variable or a class property declared with one of them, alone, in a union or as the first type argument of `Pick`, `Omit`, `Partial`, `Required`, `Readonly` or `NonNullable`, SHALL then start a chain of the rule, and a constructor parameter with a modifier SHALL start one as `this.<name>` too. The `prisma` pack lists `PrismaClient`, `PrismaService` and `Prisma.TransactionClient`.

#### Scenario: A parameter typed as a pick of the client is a root
- **WHEN** `src/tenants.ts` has `import type { Prisma } from '@prisma/client'`, a function with the parameter `reader: Pick<Prisma.TransactionClient, 'tenant' | 'tenantBranch'>` that calls `reader.tenant.findUnique()` on line 4 and `reader.tenantBranch.findMany()` on line 5, and a function with the parameter `options: { tenant: { findFirst(): void } }` that calls `options.tenant.findFirst()`, and `src/tenants.test.ts` has `const mockPrisma = { tenant: { findFirst: vi.fn() } }` and calls `mockPrisma.tenant.findFirst()`
- **THEN** `src/tenants.ts` calls `table tenant` at line 4 and `table tenantbranch` at line 5, and no other table
- **AND** `src/tenants.test.ts` calls no table

#### Scenario: A client typed as PrismaClient or PrismaService is a root
- **WHEN** `src/invoices.ts` has a class with the property `private readonly client: PrismaClient` and the constructor `constructor(private readonly store: PrismaService, plain: PrismaService)`, whose methods call `this.store.invoice.findMany()` on line 12, `this.client.payment.findUnique()` on line 16 and `this.plain.refund.findMany()` on line 20, and a function with the parameter `client: PrismaClient | null` that calls `client.booking.count()` on line 25
- **THEN** the file calls `table invoice` at line 12, `table payment` at line 16 and `table booking` at line 25
- **AND** the call on `this.plain` names no table, because a constructor parameter without a modifier is no property

### Requirement: A root needs every binding to agree
A name or a property that the file also declares anywhere in it with another type SHALL start no chain, because a root holds for the whole file and not for one scope. A declaration has the type it is annotated with and, when it has no annotation, the class that its initializer constructs with `new <Name>(…)`; a declaration with neither has no type, which is another type than any root type.

#### Scenario: A name declared with another type is no root
- **WHEN** `src/names.ts` has a function with the parameter `tx2: Pick<Prisma.TransactionClient, 'tenant'>` and a function with the parameter `tx2: { tenant: { findFirst(): void } }`, and both call `tx2.tenant.findFirst()`
- **AND** `src/properties.ts` has a class with the property `client: PrismaClient` and a class with the property `client: { tenant: { findFirst(): void } }`, and a method of each calls `this.client.tenant.findFirst()`
- **AND** `src/consistent.ts` has a function with the parameter `client: PrismaClient` that calls `client.tenant.findFirst()` on line 4 and a function with the parameter `client: PrismaClient | null` that calls `client.booking.count()` on line 8
- **AND** `src/earlier.ts` has a function with the parameter `tx2` and no type before a function with the parameter `tx2: PrismaClient`, and a class with the property `client = makeClient()` before a class with the property `client: PrismaClient`, and each of them calls `.tenant.findFirst()` on its name
- **THEN** `src/names.ts`, `src/properties.ts` and `src/earlier.ts` call no table
- **AND** `src/consistent.ts` calls `table tenant` at line 4 and `table booking` at line 8

#### Scenario: A binding made with new has the type it constructs
- **WHEN** `src/seed.ts` has `const client = new PrismaClient()` and a function with the parameter `client: PrismaClient` that calls `client.tenant.createMany()` on line 6
- **AND** `src/alone.ts` has `const client = new PrismaClient({ log: ['query'] })` and a function that calls `client.booking.count()` on line 6
- **AND** `src/members.ts` has a class with the property `store = new PrismaClient()` whose method calls `this.store.invoice.findFirst()` on line 7, and a function with the parameter `reader = new PrismaClient()` that calls `reader.payment.count()` on line 12
- **AND** `src/other.ts` has `const client = new Map()` and a function with the parameter `client: PrismaClient` that calls `client.tenant.findFirst()`
- **THEN** `src/seed.ts` calls `table tenant` at line 6, `src/alone.ts` calls `table booking` at line 6, and `src/members.ts` calls `table invoice` at line 7 and `table payment` at line 12
- **AND** `src/other.ts` calls no table

### Requirement: Signatures bind nothing
A parameter of a signature without a body SHALL bind no name, so that it is neither a root nor a reason for another declaration of its name to be no root. These are the method, call and construct signatures of an interface or a type literal, a function type, a constructor type, an overload, an abstract method and a `declare function`. A function type SHALL bind none wherever it is written, and so SHALL the name of a type parameter.

#### Scenario: A signature without a body binds nothing
- **WHEN** `src/signatures.ts` has a parameter typed `PrismaClient`, each with another name, in an interface method signature, a call signature, a construct signature, a function type, a `declare function`, an overload signature and an abstract method, and calls `.tenant.findFirst()` on each of those seven names, none of which the file declares anywhere else
- **THEN** the file calls no table

#### Scenario: A function type binds nothing wherever it is written
- **WHEN** `src/types.ts` has a function with the return type `(returnedClient: PrismaClient) => void`, a class with the property `handler?: (propertyClient: PrismaClient) => void` and `make() as (castClient: PrismaClient) => void`, and calls `.tenant.findFirst()` on `returnedClient`, `propertyClient` and `castClient`, none of which the file declares anywhere else
- **THEN** the file calls no table

#### Scenario: A signature is no reason for another declaration to be no root
- **WHEN** `src/shared.ts` has a parameter typed `string` in an interface method signature, a call signature, a construct signature, a function type, a `declare function`, an overload signature, an abstract method and the return type of a function, each with another name, and a type parameter with a ninth name
- **AND** a function with a body has a parameter typed `PrismaClient` with each of those nine names and calls `.tenant.findFirst()` on each of them, on lines 42 to 50
- **THEN** the file calls `table tenant` on each of the nine lines

### Requirement: Type aliases of root types
A `type` alias of the same file SHALL be read as the type it defines, through any number of aliases, when a declaration is checked for a root type. An alias that is defined by itself, directly or through other aliases, SHALL define no root type, and no alias SHALL be followed more than once for a declaration, so that a type that names itself many times is read as fast as one that names itself once.

#### Scenario: A local type alias of a client type is a root
- **WHEN** `src/readers.ts` has `type Client = Prisma.TransactionClient` on line 3, `type Reader = Pick<Client, 'tenant'>` on line 4, `type Options = { tenant: { findFirst(): void } }` on line 5, and `type Loop = Again` and `type Again = Loop` on lines 6 and 7, and functions that take `reader: Reader`, `options: Options` and `loop: Loop` and call `reader.tenant.findUnique()` on line 10, `options.tenant.findFirst()` on line 14 and `loop.tenant.findMany()` on line 18
- **THEN** the file calls `table tenant` at line 10 and no other table

#### Scenario: A long chain of type aliases is followed to its end
- **WHEN** `src/chain.ts` has the nine aliases `Chain1` to `Chain9`, where `Chain1` is `PrismaClient` and each of the others is the one before it, and a function with the parameter `client: Chain9` that calls `client.tenant.findFirst()` on line 14
- **AND** `src/cycle.ts` has `type Ping = Pong` and `type Pong = Ping` and a function with the parameter `ring: Ping` that calls `ring.tenant.findMany()`, and `type Back = Forth | PrismaClient` and `type Forth = Back` and a function with the parameter `value: Forth` that calls `value.booking.count()` on line 13
- **AND** `src/fan-out.ts` has `type Wide = Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide` and a function with the parameter `value: Wide` that calls `value.tenant.findFirst()`
- **AND** `src/long.ts` has the 100,000 aliases `Chain1` to `Chain100000`, where `Chain1` is `PrismaClient` and each of the others is the one before it, and a function with the parameter `client: Chain100000` that calls `client.tenant.findFirst()` on line 100003
- **THEN** `src/chain.ts` calls `table tenant` at line 14 and `src/cycle.ts` calls `table booking` at line 13, and no other table
- **AND** `src/fan-out.ts` calls no table, and the project is indexed without waiting on `Wide`
- **AND** `src/long.ts` calls `table tenant` at line 100003, and the project is indexed within twenty seconds

### Requirement: Local types shadow root types
A name that the file declares itself, as a `type` alias, an `interface`, a `class` or an `enum`, SHALL be that local type and never the root type of a pack with the same name. A local `type` alias SHALL be read as the type it defines, which may be a root type, an `interface` or a `class` SHALL be a root type only when it extends one, and an `enum` SHALL be none. Only a name that the file does not declare SHALL be checked against the root types.

#### Scenario: A local type shadows a root type
- **WHEN** `src/alias.ts` has `type PrismaClient = { tenant: { findFirst(): void } }`, `src/interface.ts` has an `interface PrismaClient`, `src/class.ts` has a `class PrismaService`, `src/enum.ts` has an `enum PrismaClient`, and each of the four has a function with a parameter of that type that calls `.tenant.findFirst()` on it
- **AND** `src/alias-of-root.ts` has `import type { Prisma } from '@prisma/client'` and `type PrismaClient = Pick<Prisma.TransactionClient, 'tenant'>`, and a function with the parameter `client: PrismaClient` that calls `client.tenant.findFirst()` on line 6
- **THEN** `src/alias-of-root.ts` calls `table tenant` at line 6
- **AND** the other four files call no table

### Requirement: Root types not written by their name are not read
A root type imported under another name, a root type reached through a namespace import under another name, `typeof` a name, and a local `type` alias of an import type SHALL NOT be read as a root type.

#### Scenario: A client type not written by its name is no root
- **WHEN** `src/renamed.ts` has `import type { PrismaClient as Base } from '@prisma/client'` and a parameter `client: Base`, `src/namespaced.ts` has `import type { Prisma as P } from '@prisma/client'` and a parameter `reader: P.TransactionClient`, `src/typed-of.ts` has `const shared = new PrismaClient()` and a parameter `client: typeof shared`, and `src/aliased-import.ts` has `type Db = import('@prisma/client').PrismaClient` and a parameter `client: Db`, each of them calling `.tenant.findFirst()` on its parameter
- **THEN** the four files call no table

### Requirement: Intersections and heritage of root types
An intersection that has a root type as one of its members SHALL be read as a root type, as a union is. A class or an interface of the file that extends a root type, or one of the file that does, SHALL be a root type, through any number of such declarations and `type` aliases. One that extends no root type SHALL NOT be.

#### Scenario: An intersection with a client type is a root
- **WHEN** `src/intersected.ts` has `import type { PrismaClient } from '@prisma/client'` and a parameter `client: PrismaClient & { extra: true }` that calls `client.tenant.findFirst()` on line 4
- **AND** `src/unrelated.ts` has a parameter typed with an intersection of two `type` aliases that are no root types and a parameter typed with an intersection of two object types, and calls `.tenant.findFirst()` on each
- **THEN** `src/intersected.ts` calls `table tenant` at line 4
- **AND** `src/unrelated.ts` calls no table

#### Scenario: A class or an interface that extends a client type is a root
- **WHEN** `src/service.ts` has `class PrismaService extends PrismaClient` and a class with the constructor parameter `private readonly store: PrismaService` whose method calls `this.store.tenant.findFirst()` on line 13
- **AND** `src/chain.ts` has `class Audited extends PrismaClient`, `class Logged extends Audited`, `interface Narrow extends Pick<PrismaClient, 'tenant'>` and `interface Wide extends Narrow`, a function with a parameter of each of those four types that calls `.tenant.findFirst()` on it on lines 12, 16, 20 and 24, and `const made = new Logged()` with `made.tenant.count()` on line 30
- **AND** `src/unrelated.ts` has `class Cache extends Map` and `interface Shape extends Base`, where `Base` is an interface of the file, and a function with a parameter of each that calls `.tenant.findFirst()` on it
- **THEN** `src/service.ts` calls `table tenant` at line 13 and `src/chain.ts` calls `table tenant` at lines 12, 16, 20, 24 and 30
- **AND** `src/unrelated.ts` calls no table

### Requirement: Model types of other packages
A name taken from a package that `from` does not list SHALL name nothing, so the model types of a client generated to a custom `output` path, or of a package that re-exports the client with `export * from '@prisma/client'`, SHALL name no table.

#### Scenario: A model type taken from another package names nothing
- **WHEN** `src/custom-output.ts` has `import type { Tenant } from '../generated/prisma'`, a client generated to `generated/prisma`, and `src/shared.ts` has `import type { Booking } from '@acme/db'`, a package that re-exports the client with `export * from '@prisma/client'`
- **THEN** the two files call no table

### Requirement: Exports
The connector SHALL record the names a file exports itself, sorted: its exported declarations, the names of its `export { … }` lists, `default`, and the names its named re-exports and its `export * as` give, but not the names an `export *` brings. A named re-export SHALL record the name it exports each imported name as. The connector SHALL record the exports of a file as unknown when the file has `export =` or was read past syntax errors.

#### Scenario: Every export form is named
- **WHEN** a file has `export const a = 1`, `export const { b, c: d } = obj`, `export declare const g: number`, `export function f() {}`, `export class C {}`, `export enum E {}`, `export type T = string`, `export interface I {}`, `export namespace N {}`, `export default 1`, `export { x as y }`, `export { z as w } from './z'`, `export * as ns from './ns'` and `export * from './all'`
- **THEN** its exports are `C`, `E`, `I`, `N`, `T`, `a`, `b`, `d`, `default`, `f`, `g`, `ns`, `w` and `y`
- **AND** its re-export of `./z` records `z` exported as `w`, matching the reviewed snapshot

#### Scenario: CommonJS exports are unknown
- **WHEN** one file assigns `module.exports = { ...a }` and another assigns `exports[k] = 1`
- **THEN** the exports of both are unknown

#### Scenario: Exports of a file read past errors are unknown
- **WHEN** a file with `export const a = 1` is read past a syntax error
- **THEN** its exports are unknown

### Requirement: Mounts through a star
The link phase SHALL find the router a mount names behind a file that re-exports another with `export * from`, and SHALL NOT find a router imported as `default` there, because a star forwards every name but `default`. The impact walk makes the same distinction between the names a barrel exports and those it does not. A cycle of such files SHALL end, and the router is found behind it when one of them exports it.

#### Scenario: A star does not forward default to a mount
- **WHEN** `routes.ts` has only `export * from './users'`, `users.ts` has a router `users` exported by name and a router `fallback` exported as `default` with one route each, `by-name.ts` mounts `users` imported by name from `./routes` under `/users`, and `by-default.ts` mounts `fallback` imported as `default` from `./routes` under `/old`
- **THEN** `users.ts` serves the route of `users` under `/users` and the route of `fallback` without the prefix `/old`
- **AND** a change to `users.ts` reaches `routes.ts` as a re-export of `users`, `by-name.ts` as an importer of `users` and `by-default.ts` as an importer of `default`, which `routes.ts` does not export

#### Scenario: A cycle of stars ends
- **WHEN** `a.ts` has `export * from './b'` and `export * from './extra'`, `b.ts` has `export * from './a'` and a router `users` exported by name, `extra.ts` exports `extra`, `by-name.ts` mounts `users` imported from `./a` under `/users`, and `by-gone.ts` mounts `gone`, which nothing exports, imported from `./a`
- **THEN** `b.ts` serves the route of `users` under `/users` and nothing is mounted for `gone`
- **AND** a change to `extra.ts` reaches `a.ts` and `b.ts` as re-exports of `extra` and `by-gone.ts` as an importer of `gone`, and does not reach `by-name.ts`, because `a.ts` exports `users`

### Requirement: Mounts written with require
A router mounted in place with `use('<prefix>', require('<specifier>'))` SHALL be the router the file the specifier leads to exports as `default` or assigns to `module.exports`, and SHALL have its routes served under the prefix.

#### Scenario: A router required in place serves under the mount
- **WHEN** `items.js` has `const router = Router(); router.get('/:id', h); module.exports = router` and `app.js` has `app.use('/items', require('./items'))`
- **THEN** `items.js` serves `http GET /items/{}` at the line of `router.get`

### Requirement: Declarations
The connector SHALL record the declarations of a file, each with the lines it spans: every top-level function, class, variable, enum, interface, type alias and namespace, each name a destructuring declares, `default` for `export default` of an expression, every member of a class as `Class.member` and every method and property signature of an interface as `Interface.member`. Overloads and merged declarations SHALL be joined by name. A file read past syntax errors SHALL record none.

#### Scenario: Every declaration form is recorded
- **WHEN** a file has `function f() {}`, two overloads and the body of `g`, `const { a, b: c } = obj`, `class C { constructor() {} m() {} static s = 1; #p = 2; get v() { return 1 } set v(x) {} }`, `enum E {}`, `interface I {}` twice, `type T = string`, `namespace N {}` and `export default 1`
- **THEN** its declarations are `C`, `C.#p`, `C.constructor`, `C.m`, `C.s`, `C.v`, `E`, `I`, `I.first`, `I.second`, `N`, `T`, `a`, `c`, `default`, `f` and `g`, each with its lines, matching the reviewed snapshot

#### Scenario: A file read past errors records no declarations
- **WHEN** a file declares `foo`, imports `dep` and has a syntax error
- **THEN** it records no declarations
- **AND** its static imports record no local names

#### Scenario: A member of an interface is a declaration
- **WHEN** a file has `export interface Provider { name: string; list(): string[]; list(id: string): string[]; [key: string]: unknown; ['computed'](): void }`, `export default interface Named { id(): number }` and `type Shape = { size(): number }`
- **THEN** its declarations are `Named`, `Named.id`, `Provider`, `Provider.list`, `Provider.name` and `Shape`
- **AND** `Provider.list` spans both overloads

### Requirement: What a declaration uses
A declaration SHALL record, once each, the names it refers to that the file declares or binds by import: identifiers, JSX tags, type references and `typeof`, `Class.member` for `this.member` inside the class, `C.s` alone for `C.s`, and `Class.` for a type alone. An export specifier SHALL NOT count, nor a use of the declaration itself. Shadowing SHALL be ignored.

#### Scenario: References, tags, types and this are uses
- **WHEN** a file imports `useX`, `Props` and `Widget`, declares `helper`, `function View(p: Props) { const x = useX(); return <Widget value={helper(x)} /> }`, `class C { a() { this.b() } b() {} static s() {} }` and `function d() { return C.s() }`
- **THEN** `View` uses `Props.`, `Widget`, `helper` and `useX`, `C.a` uses `C.b`, and `d` uses `C.s` alone, matching the reviewed snapshot

### Requirement: Writes of top-level variables
A top-level variable SHALL record as uses the declarations that assign it, update it, assign or delete a member of it, or call on it one of `set`, `add`, `delete`, `clear`, `push`, `pop`, `shift`, `unshift`, `splice`, `sort`, `reverse` and `fill`, so that a function that fills a cache affects the functions that read it.

#### Scenario: A function that fills a cache is a use of the cache
- **WHEN** a file has `const cache = new Map()`, `function fill(k) { cache.set(k, 1) }`, `function read(k) { return cache.get(k) }`, `let count = 0` and `function bump() { count++ }`
- **THEN** `cache` uses `fill` and not `read`, and `count` uses `bump`

### Requirement: Code at load
The connector SHALL record what a file refers to as it loads: everything in a top-level statement that is not a declaration, callbacks included, and in a declaration what is under a call, `new`, a tagged template or a decorator evaluated at load, in an initializer, a static field or block, a computed key, a decorator or `extends`. Function bodies, parameter defaults and instance fields SHALL NOT count.

#### Scenario: What runs at load is recorded
- **WHEN** a file imports `f`, `g`, `h`, `k`, `n`, `mixin` and `D` from `./x` and `test` from `vitest`, and has `const a = f()`, `const b = g`, `test('x', () => h())` and `class C extends mixin(D) { static s = k(); m() { n() } }`
- **THEN** what it uses at load is `D`, `f`, `h`, `k`, `mixin` and `test`
- **AND** `b` uses `g`, and `C.m` uses `n`

### Requirement: Local names of imports and exports
A static import SHALL record the local name of each name it takes, in the same order: `React` for `import React from 'react'`, `ns` for `import * as ns`, `b` for `import { a as b }`, `a` for `import { a }`, type-only imports included. An import type, `import('x').T`, binds no name and SHALL record none. An export of a local under another name SHALL record the local: `a` for `export { a as b }`, `f` for `export default f`. A file read past syntax errors SHALL record no local names.

#### Scenario: Imports and exports record their local names
- **WHEN** a file has `import React from 'react'`, `import * as ns from './ns'`, `import { a as b, c } from './x'`, `import type { T } from './t'`, `const f = 1`, `export { f as g }` and `export default f`
- **THEN** its imports record the local names `React`, `ns`, `b` and `c`, and `T`, and its exports record `g` from `f` and `default` from `f`, matching the reviewed snapshot

### Requirement: Mocks of a module
The connector SHALL read the first argument of `vi.mock`, `vi.doMock`, `vi.unmock`, `vi.doUnmock`, `vi.importMock`, `jest.mock`, `jest.doMock`, `jest.unmock`, `jest.dontMock` and `jest.requireMock` as a mock of the module with no names when it is a string or a template with no expression. A computed first argument SHALL be neither an import nor an unread site.

#### Scenario: A mocked module is a mock
- **WHEN** `src/runners.test.ts` has `'./lib'` as the first argument of `vi.mock`, `vi.doMock`, `vi.unmock`, `vi.doUnmock`, `vi.importMock`, `jest.mock`, `jest.doMock`, `jest.unmock`, `jest.dontMock` and `jest.requireMock`, one per line, and `` vi.mock(`./lib`) `` on line 11
- **THEN** each of the eleven calls is a mock of `./lib` at its line, with no names, resolving to `src/lib.ts`

#### Scenario: A module named at run time is no import
- **WHEN** `src/computed.test.ts` has `vi.mock(path)`, `` jest.mock(`./${name}`) ``, `vi.importActual('./' + name)` and `await import(name)`
- **THEN** the file has no imports
- **AND** its only unread site is the one of `await import(name)`, with the reason computed at run time

#### Scenario: A mock call on another receiver is no import
- **WHEN** `src/others.test.ts` has `server.mock('./lib')`, `vi.fn('./lib')` and `mocker.importActual('./lib')`, and `src/lib.ts` exists
- **THEN** the file has no imports

#### Scenario: A mock that asks for the actual module reaches it
- **WHEN** `src/lib.test.ts` has `vi.mock('./lib', async () => ({ ...(await vi.importActual<typeof import('./lib')>('./lib')) }))` over lines 1 to 3 and `vi.mock('./editor', async (importOriginal) => ({ ...(await importOriginal<typeof import('./editor')>()) }))` over lines 5 to 7
- **THEN** the file mocks `./lib` at line 1 and `./editor` at line 5, imports `./lib` dynamically and as a type at line 2, and imports `./editor` as a type at line 6
- **AND** the imports resolve to `src/lib.ts` and `src/editor.ts`, so a change to either module reaches the test

### Requirement: Actual modules
The connector SHALL read the first argument of `vi.importActual` and `jest.requireActual` as a dynamic import when it is a string or a template with no expression, because both load the module as it is, and SHALL mark the import as asking for the actual module. Used as it stands, such an import SHALL name nothing. A computed first argument SHALL be neither an import nor an unread site, also when a member is read from it.

#### Scenario: An actual module is a dynamic import
- **WHEN** `src/actual.test.ts` has `vi.importActual('./lib')` on line 1 and `jest.requireActual('./lib')` on line 2, and `src/lib.ts` exists
- **THEN** each of the two calls is a dynamic import of `./lib` at its line, with no names, resolving to `src/lib.ts`
- **AND** each import asks for the actual module

### Requirement: Mocks made or set for a module
The connector SHALL read the first argument of `jest.setMock`, `jest.createMockFromModule`, `jest.genMockFromModule`, `jest.unstable_mockModule` and `jest.deepUnmock` as a mock of the module with no names when it is a string or a template with no expression, and a computed one SHALL be neither an import nor an unread site.

#### Scenario: A mock made from a module or set for it is a mock
- **WHEN** `src/made.test.ts` has `jest.setMock('./lib', {})`, `jest.createMockFromModule('./lib')`, `jest.genMockFromModule('./lib')`, `jest.unstable_mockModule('./lib', () => ({}))` and `jest.deepUnmock('./lib')` on lines 1 to 5, and `src/lib.ts` exists
- **THEN** each of the five calls is a mock of `./lib` at its line, with no names, resolving to `src/lib.ts`

### Requirement: Members read from a loaded module
A member read by name from a loaded module, `await import('x')`, `await vi.importActual('x')`, `require('x')` or `jest.requireActual('x')`, SHALL be the name its import takes. A member read from a promise of one, such as `import('x')` without `await`, SHALL name nothing. A pattern that destructures a loaded module with a rest element or a computed key SHALL also take the module itself, `*` for an import and `default` for a `require`, because it may take more than it names.

#### Scenario: A member read from a loaded module is the name it takes
- **WHEN** `src/use.ts` has `(await import('./lib')).helper()` on line 1, `require('./lib').other` on line 2 and `import('./lib').then(start)` on line 3, and `src/lib.ts` exists
- **THEN** the file has a dynamic import of `./lib` naming `helper` at line 1, a `require` of it naming `other` at line 2, and a dynamic import of it with no names at line 3

#### Scenario: A destructuring that may take more than it names takes the module
- **WHEN** `src/open.ts` has `const { a, ...rest } = await import('./lib')` on line 1, `const { [key]: b } = await import('./lib')` on line 2 and `const { c } = require('./lib')` on line 3, and `src/lib.ts` exists
- **THEN** the file has a dynamic import of `./lib` naming `a` and the namespace `*` at line 1, a dynamic import of it naming only the namespace at line 2, and a `require` of it naming `c` at line 3

#### Scenario: A member read from an actual module is the name it takes
- **WHEN** `src/members.test.ts` has `(await vi.importActual('./lib')).helper()` on line 1, `jest.requireActual('./lib').other` on line 2, `const { third } = jest.requireActual('./lib')` on line 3, `vi.importActual('./lib').then(start)` on line 4 and `jest.requireActual(name).helper` on line 5, and `src/lib.ts` exists
- **THEN** the file has a dynamic import of `./lib` naming `helper` at line 1, `other` at line 2 and `third` at line 3, and one with no names at line 4
- **AND** line 5 is neither an import nor an unread site

### Requirement: Members read through a field of a declared type
Inside a class, a declaration that reads `this.<field>.<member>`, or `this.#<field>.<member>`, SHALL use `<Type>.<member>` for each name `<Type>` the field's type stands for at its head, spelled as the type writes it, when the file declares or binds `<Type>`. Any other read of the field SHALL use `<Type>` alone. A field is a constructor parameter with `private`, `protected`, `public` or `readonly`, or a property with a type annotation or a `new` initializer.

#### Scenario: A method that calls an injected service uses the member it calls
- **WHEN** `src/invoices.resolver.ts` imports `InvoiceService`, `MailQueue` and `JobQueue` from `./services` and declares `class InvoiceResolver { private readonly mail: MailQueue; private jobs = new JobQueue(); constructor(private readonly invoices: InvoiceService, plain: InvoiceService) {} issue() { this.invoices.issue(); this.mail.push(); this.jobs.add() } other() { return this.plain } }`
- **THEN** `InvoiceResolver.issue` uses `InvoiceService.issue`, `MailQueue.push` and `JobQueue.add`
- **AND** `InvoiceResolver.other` uses none of them, because a constructor parameter without a modifier is no field

#### Scenario: A field read as a value uses its class
- **WHEN** `src/invoices.resolver.ts` declares the class of the previous scenario with the method `all() { return this.invoices }`
- **THEN** `InvoiceResolver.all` uses `InvoiceService` and not a member of it

#### Scenario: A member of a field is read in its own class
- **WHEN** `src/jobs.ts` imports `InvoiceService` and `MailQueue` from `./services` and declares `class Billing { constructor(private readonly svc: InvoiceService) {} }` and `class Mailer { constructor(private readonly svc: MailQueue) {} send() { this.svc.push() } }`
- **THEN** `Mailer.send` uses `MailQueue.push` and neither `InvoiceService` nor a member of it
- **AND** `Billing` uses nothing, because no method of it reads the field

#### Scenario: A member of a private field is read through the field
- **WHEN** `src/mailer.ts` imports `JobQueue` and `MailQueue` from `./services` and declares `class Mailer { #queue: MailQueue; #jobs = new JobQueue(); queue = 0; send() { this.#queue.push() } later() { this.#jobs.add() } count() { return this.queue } }`
- **THEN** `Mailer.send` uses `MailQueue.push` and `Mailer.#queue`, and `Mailer.later` uses `JobQueue.add` and `Mailer.#jobs`
- **AND** `Mailer.count` does not use `MailQueue`, because `queue` and `#queue` are two fields and only the private one has a type

#### Scenario: A member is used as the type of the field is written
- **WHEN** `src/notifier.ts` imports `* as services` from `./services` and `JobQueue` and `MailQueue` from `./queues`, declares `class Local { run() {} stop() {} }`, and declares `class Notifier` with the fields `mail: services.MailQueue`, `either: MailQueue | JobQueue` and `local: Local`, whose methods `qualified`, `union`, `declared`, `undeclared`, `nested` and `indexed` read `this.mail.push()`, `this.either.send()`, `this.local.run()`, `this.local.start()`, `this.mail.items.push()` and `this.mail[key]`
- **THEN** `qualified` uses `services.MailQueue.push` and `union` uses `JobQueue.send` and `MailQueue.send`
- **AND** `declared` uses `Local.run` and `undeclared` uses `Local.start`, though `Local` declares no `start`
- **AND** `nested` uses `services.MailQueue.items`, because only the first member read from the field is the member, and `indexed` uses `services`, because a computed key names no member

#### Scenario: A member is used however the field is spelled
- **WHEN** `src/spellings.ts` imports `Left`, `MailQueue`, `Right` and `Special` from `./queues` and declares `class Spellings` with the private field `#queue: MailQueue` and the parameter properties `mail: MailQueue` and `both: Left & Right`, whose methods read `this.mail?.push()`, `this.mail!.push()`, `(this.mail as Special).push()`, `this['mail'].push()`, `this.mail.push.bind(null)`, `const { push } = this.mail`, `const queue = this.mail`, `this.both.send()`, `(this.#queue as Special).push()` and `this.#queue!.push()`
- **THEN** the methods that read `push` through the field use `MailQueue.push`, and the two that cast it also use `Special`
- **AND** the methods that take the field as a value use `MailQueue`, and the one that reads `send` uses `Left.send` and `Right.send`
- **AND** the two that read `#queue` also use `Spellings.#queue`

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
With the `nestjs` pack, a route decorator on a method of a class with no `@Controller` decorator SHALL be served by every class decorated `@Controller('<prefix>')` that extends it by name, under that prefix, at the line of the method in the file of the base class and at the line of `@Controller`. A base class that no controller extends, and a subclass without the decorator, SHALL serve nothing, and a path on the base class that is neither a literal nor a constant SHALL be an unread site.

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

### Requirement: NestJS global prefix
With the `nestjs` pack, `app.setGlobalPrefix(<prefix>)` on an app made by `NestFactory.create(<Root>)` in the same file, where `<prefix>` is a literal or a constant of the same file, SHALL put every controller that `<Root>` reaches through the `imports` and `controllers` of `@Module` classes behind the prefix, the routes it inherits included. A controller that no such app reaches SHALL keep its path, and a prefix that is neither a literal nor a constant of the same file SHALL be an unread site.

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

#### Scenario: A global prefix held in a constant is read
- **WHEN** `main.ts` has `const PREFIX = 'api'`, `const app = await NestFactory.create(AppModule)` and `app.setGlobalPrefix(PREFIX)`, `AppModule` lists `UsersController` in `controllers`, and `UsersController` is `@Controller('users')` with a method decorated `@Get('x')`
- **THEN** `users.controller.ts` serves `http GET /api/users/x` at the line of `@Get` and `http GET /users/x` nowhere
- **AND** `main.ts` has no unread site

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

### Requirement: Route paths that are computed
A NestJS route decorator whose path argument is not a string literal, a template or a constant SHALL be a blind spot at the line of the decorator, not a route at the controller's prefix; a route decorator with no argument SHALL still serve the prefix alone.

#### Scenario: A route whose path is computed is a blind spot
- **WHEN** a class decorated `@Controller('/')` has a method decorated `@Get(paths.login())`, where `paths` is an imported object whose `login` is a function, and another method decorated `@Get()`
- **THEN** the file has an unread site at the line of `@Get(paths.login())` and serves `http GET /` only at the line of `@Get()`

### Requirement: Controller prefixes that are not read
A class decorator whose prefix is not read, such as `@Controller(PREFIX)` with `PREFIX` imported as a call that gives no text, SHALL be an unread site at its line, and its class SHALL serve no route, neither its own nor one it inherits; `@Controller()` and `@Controller({ version: '1' })` give no prefix.

#### Scenario: A controller whose prefix is not read serves nothing
- **WHEN** `auth.controller.ts` has `@Controller(PREFIX)`, with `PREFIX` imported from a file where it is `compute()`, and methods `@Get('login')` and `@Get(LOGIN)`, with `LOGIN` imported
- **AND** `user.controller.ts` has `@Controller(prefix()) class UserController extends UserControllerBase`, where `UserControllerBase` in another file has `@Get(':id')`
- **THEN** each controller file has an unread site at the line of its `@Controller` and serves no http key
- **AND** the file of the base class serves nothing

### Requirement: Receivers made in another file
A call that a rule reads on a receiver, written on a name the file imports from a file of the same project, SHALL be read on what that file makes under the name at its top level: an `axios.create()` client calls under its `baseURL`, and an Express app or router serves its routes under its own mounts. A call on an imported name that no such call makes SHALL be read as the rules without a receiver read it, or SHALL be silent when none does.

#### Scenario: A client made in another file is called
- **WHEN** `http.ts` has `export const api = axios.create({ baseURL: '/api' })` and `users.ts` imports `api` from `./http` and calls `api.get('/users')`
- **THEN** `users.ts` calls `http GET /api/users` at that line

#### Scenario: A router made in another file serves under its mounts
- **WHEN** `router.ts` has `export const router = express.Router()`, `users.ts` imports `router` from `./router` and calls `router.get('/users', list)`, and `app.ts` has `const app = express()` and `app.use('/api', router)` with `router` imported from `./router`
- **THEN** `users.ts` serves `http GET /api/users` at the line of `router.get`

#### Scenario: A call on an imported name that no rule makes is silent
- **WHEN** `cache.ts` has `export const cache = new Map()`, `api.ts` has `export const api = axios.create({ baseURL: '/b' })`, and `a.ts` imports `cache` and `* as store` from `./cache` and `api` from `./api`, and calls `cache.get('k')`, `store.cache.get('k')` and `api.get('/users')`
- **THEN** `a.ts` calls `http GET /b/users` at the line of `api.get`
- **AND** `a.ts` has no other contract site and no unread site

#### Scenario: A call on the default import of a file that makes a client is the client's
- **WHEN** `lib/http.ts` has `export default axios.create({ baseURL: '/api' })` and `users.ts` has `import axios from './lib/http'` and `axios.get('/users')`, and `lib/axios.ts` has `import axios from 'axios'; export default axios` and `orders.ts` has `import axios from './lib/axios'` and `axios.get('/orders')`
- **THEN** `users.ts` calls `http GET /api/users` and not `http GET /users`
- **AND** `orders.ts` calls `http GET /orders`

#### Scenario: A client made inside a function does not replace the one a file exports
- **WHEN** `client.ts` has `export const api = axios.create({ baseURL: '/a' })` and a function with `const api = axios.create({ baseURL: '/b' })`, and `users.ts` has `import { api } from './client'` and calls `api.get('/users')`
- **THEN** `users.ts` calls `http GET /a/users`

#### Scenario: A call on an imported name is read by the rule of the call that made it
- **WHEN** `app.ts` has `export const app = express()`, `http.ts` has `export const api = axios.create({ baseURL: '/b' })`, and `a.ts` imports both and calls `app.get('env')`, `app.get('/k')` and `api.get('/users', { params })`
- **THEN** `a.ts` calls `http GET /b/users` at the line of `api.get`
- **AND** `a.ts` serves no route, calls nothing else and has no unread site

### Requirement: Imported model values
A pack MAY name a contract for each name a file takes from a package. With the `prisma` pack, a name the file imports from `@prisma/client` and uses as a value SHALL call the table of that name at the line of the import, as the package exports it and not as the file renames it, except `Prisma`, `PrismaClient`, `PrismaPromise` and `$Enums`. A type, a re-export, a namespace import, a default import, a side-effect import and `export *` SHALL name nothing.

#### Scenario: A model imported as a value calls its table
- **WHEN** `src/models.ts` has `import { BookingStatus } from '@prisma/client'` on line 1 and reads `BookingStatus.CONFIRMED`
- **AND** `src/renamed.ts` has `import { BookingStatus as Status } from '@prisma/client'` on line 1 and reads `Status.CONFIRMED`
- **THEN** both files call `table bookingstatus` at line 1, and `src/renamed.ts` does not call `table status`, because the table is named as the package exports the name

#### Scenario: A model used only as a type names nothing
- **WHEN** `src/declared.ts` has `import type { Tenant } from '@prisma/client'` and `src/inline.ts` has `import { type Tenant, PrismaClient } from '@prisma/client'`
- **AND** `src/annotated.ts` has `import { Tenant } from '@prisma/client'` and uses `Tenant` only as a parameter type, a return type and a type argument
- **AND** `src/types.ts` has `import('@prisma/client').Tenant` on line 1, `import('@prisma/client').TenantBranch[]`, `import('@prisma/client').Prisma.TenantWhereInput` on line 3 and `typeof import('@prisma/client')`
- **THEN** none of the files calls a table

#### Scenario: A re-exported model names nothing
- **WHEN** `src/models.ts` has `export type { Tenant, Prisma } from '@prisma/client'`, `src/renamed.ts` has `export { Tenant as T } from '@prisma/client'` and `src/listed.ts` has `import { BookingStatus } from '@prisma/client'` and `export { BookingStatus }`
- **THEN** none of the files calls a table, because a re-export is not a use

#### Scenario: A namespace import of the client names nothing
- **WHEN** `src/client.ts` has `import * as client from '@prisma/client'`, a default import of it, `import { default as other } from '@prisma/client'`, `import '@prisma/client'`, `export * from '@prisma/client'`, `export * as models from '@prisma/client'` and `export { default as Client } from '@prisma/client'`
- **THEN** the file calls no table

### Requirement: Uses of an imported name as a value
A file uses an imported name as a value when its code names the local name outside every type. A name written in a type SHALL be no use of the value, and neither SHALL be a re-export, `import type`, an import type, or a field typed with the name, so `this.field` is no use of it.

#### Scenario: A name in a value position is a use
- **WHEN** a file imports `Tenant` from `@prisma/client` and has `Tenant()`, `[Tenant]`, `Tenant.ACTIVE`, `typeof Tenant` as an expression, `export default Tenant`, `class Row extends Tenant`, `enum Kind { A = Tenant.A }`, `Tenant as unknown`, `Tenant satisfies object`, `Tenant!`, `{ Tenant }`, `{ ...Tenant }`, `<Tenant />`, `(t = Tenant) => t`, a class property `x = Tenant`, `import Alias = Tenant.Inner`, or `Tenant.fields` in a method beside a parameter typed `Map<Box<string>, Tenant>`
- **THEN** each file calls `table tenant`

#### Scenario: A name outside a value position is no use
- **WHEN** a file imports `Tenant` from `@prisma/client` and uses it as a parameter or return type, in `implements Tenant`, in `interface Row extends Tenant`, as the type argument `g<Tenant>()`, in `typeof Tenant` written in a type, in `Tenant[]`, in the types of `satisfies` and `as`, in an ambient `declare function` signature, as the type of a constructor parameter property read as `this.tenant`, in `export { Tenant }` or `export { Tenant as T }`
- **AND** a file has `import type { Tenant }`, `import { type Tenant }`, `export type Row = import('@prisma/client').Tenant`, `export { Tenant as T } from '@prisma/client'` or `export type { Tenant } from '@prisma/client'`
- **THEN** none of the files calls a table

### Requirement: Receivers whose prefix is not text
A call that a rule serves routes with, on a receiver whose maker holds the `prefix` of the rule and no connector can read it as text, SHALL be an unread site and serve nothing, whether the receiver is made in the same file or in the file that exports it. The base of a client is no such prefix: a client whose base is not text SHALL be called at the path after it.

#### Scenario: A route on a router made in the same file with a prefix that is not text is unread
- **WHEN** a pack serves `get` on a router made by `makeRouter`, behind the `prefix` property of its argument, and `router.ts` has `dynamic = makeRouter({ prefix: process.env.PREFIX })`, `fixed = makeRouter({ prefix: '/v1' })` and `bare = makeRouter({})`, each with `get('/own', list)` called on it
- **THEN** `router.ts` serves `http GET /v1/own` and `http GET /own`
- **AND** the call on `dynamic` is an unread site and serves nothing

#### Scenario: A route on a router made in another file with a prefix that is not text is unread
- **WHEN** the same pack is on, `router.ts` exports the three routers, and `users.ts` imports them and calls `dynamic.get('/users', list)`, `fixed.get('/orders', list)` and `bare.get('/items', list)`
- **THEN** `users.ts` serves `http GET /v1/orders` and `http GET /items`
- **AND** the call on `dynamic` is an unread site and serves nothing

#### Scenario: A client whose base is not text is called at the path after it
- **WHEN** a file has `const api = axios.create({ baseURL: process.env.API })` and `api.get('/users')`
- **THEN** it calls `http GET /users` and has no unread site

### Requirement: Spreads
A call with `...spread` among its arguments, and an object literal with `...spread` among its properties, SHALL hold what they may give: the argument at or after the spread, and a property the literal does not write. What a call holds and no connector can read as text SHALL leave the value unread, and an argument written after a spread SHALL not be read at the place it is written.

#### Scenario: A spread may carry a property
- **WHEN** a pack calls `http` for `client.request` with the key `$arg1` and the method `$arg2.method|GET`, and a file has `client.request('/plain')` on line 1, `client.request('/post', { method: 'POST' })` on line 2, `client.request('/options', { ...options })` on line 3, `client.request('/rest', ...rest)` on line 4, `client.request(...args)` on line 5 and `client.request('/own', { ...options, method: 'PUT' })` on line 6
- **THEN** the file calls `http GET /plain`, `http POST /post`, `http * /options`, `http * /rest` and `http PUT /own`
- **AND** the call on line 5 is an unread site

### Requirement: Environment writes
With the `env` pack, a literal name that a site assigns, updates, destructures into, loops into or deletes, and `vi.stubEnv("<NAME>", …)`, SHALL serve `env <NAME>` and not call it. A compound assignment or an update whose value is used SHALL call and serve it. `Object.assign(process.env, …)`, `jest.replaceProperty(process.env, …)` and `process.env = {…}` SHALL not be read, which the provides of the project name as a blind spot of the variables it serves.

#### Scenario: A write of an environment variable serves it
- **WHEN** a file has `process.env.A = 'x'`, `process.env['B'] ??= 'y'`, `delete process.env.C`, `vi.stubEnv('D', 'z')` and `({ E: process.env.F } = values)`, each as a statement
- **THEN** it serves `env A`, `env B`, `env C`, `env D` and `env F`, calls no `env` and leaves no unread site

#### Scenario: Every form of assignment target is a write
- **WHEN** a file has `process.env.G! = 'v'`, `(process.env.H as string) = 'v'`, `process.env.I += ':x'`, `process.env.J++`, `for (process.env.K of values) {}` and `[process.env.L] = values`
- **THEN** it serves `env G`, `env H`, `env I`, `env J`, `env K` and `env L` and calls none

#### Scenario: A compound assignment used as a value reads and serves
- **WHEN** a file has `export const url = (process.env.API_URL ??= 'http://localhost')`, `export const retries = () => (process.env.RETRIES ??= '3')` and `export const next = ++process.env.COUNT`
- **THEN** it calls and serves `env API_URL`, `env RETRIES` and `env COUNT`

#### Scenario: A computed name in a write is one unread site
- **WHEN** a file has `process.env[process.env.TARGET] = 'v'` and, in a function with the parameter `name`, `process.env[name] = 'v'`
- **THEN** it calls `env TARGET`, serves no `env`, and leaves one unread site for each statement

### Requirement: A value that the code drops
A compound assignment or an update SHALL only serve the variable when the code around it drops its value: an expression statement, a `for` head or update, the operand of `void`, or a member of a sequence that is not the last or whose sequence is dropped. Where the value can be used, as in an arrow body, a `return`, a condition, an operand of `&&` or the last member of a kept sequence, it SHALL call and serve it.

#### Scenario: A compound assignment whose value is dropped only serves
- **WHEN** a file has `process.env.A += 'x', log();`, `for (;; process.env.B++) {}`, `for (process.env.C += 1;;) {}`, `void (process.env.D ??= 'x');`, `(a, (b, process.env.E += 1));`, `for (process.env.F += 1, process.env.G++;;) {}`, `(process.env.H ??= 'x');` and a function with `process.env.S ??= '3';` as a statement
- **THEN** it serves `env A` to `env H` and `env S` and calls none of them

#### Scenario: A compound assignment whose value can be used reads and serves
- **WHEN** a file has an arrow with the body `(process.env.I ??= '3')`, a function that returns `process.env.K ??= '3'`, `const l = (a, process.env.L += 1)`, `list.map(() => process.env.N ??= 'x')`, `if (process.env.O ??= 'x') {}`, `process.env.P = process.env.Q ??= 'x'` and `a && (process.env.R ??= 'y')`
- **THEN** it calls and serves `env I`, `env K`, `env L`, `env N`, `env O`, `env Q` and `env R`, and only serves `env P`

### Requirement: A write below a variable
A site that assigns to a member of the value of an environment variable, as `process.env.J.foo = 1`, SHALL call `env J` and not serve it, because it sets no variable.

#### Scenario: A write to a member of a variable reads it
- **WHEN** a file has `process.env.J.foo = 1;`
- **THEN** it calls `env J` and serves no `env`

### Requirement: Writes to a Prisma client
A site that assigns to a member of a Prisma client, as a test does with `prisma.tenant.findFirst = vi.fn()`, SHALL name no table, because the assignment queries nothing.

#### Scenario: A write to a Prisma client names no table
- **WHEN** a file has `const prisma = new PrismaClient()`, `prisma.tenant.findFirst = vi.fn()` and `export const all = () => prisma.booking.findMany()`
- **THEN** it calls `table booking` and no other table

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

### Requirement: A service a test provides itself
A call rule MAY name in `test_double` an injection token. In a test file that provides a stand-in for that token, by an object with `provide` and one of `useValue`, `useFactory`, `useClass` or `useExisting`, or by `overrideProvider`, anywhere in the file, the rule SHALL read nothing: no contract and no unread site. The `nestjs-config` pack names `ConfigService`. Any other file, and a test with no stand-in, SHALL keep its reads.

#### Scenario: A read of a ConfigService a test provides is no environment read
- **WHEN** a spec file has `{ provide: ConfigService, useValue: stub }` and calls `configService.get('API_URL')` above the provider, `configService.get(Env.BASE)` with `Env` imported from another file, and `configService.get(key)` with `key` a parameter
- **THEN** the file calls no `env` key, and has no unread site at those lines

#### Scenario: A read of an overridden ConfigService is no environment read
- **WHEN** a spec file calls `Test.createTestingModule({ imports: [AppModule] }).overrideProvider(ConfigService).useValue(stub)` and `configService.get('API_URL')`
- **THEN** the file calls no `env` key

#### Scenario: A test on the real ConfigService keeps its reads
- **WHEN** a spec file imports `ConfigModule.forRoot()`, lists `ConfigService` in `providers`, has `{ provide: BillingService, useValue: stub }` and calls `configService.get('API_URL')`
- **THEN** the file calls `env API_URL`

#### Scenario: A file that is no test keeps its reads though it provides a stand-in
- **WHEN** `src/app.module.ts` has `{ provide: ConfigService, useValue: stub }` and calls `configService.get('API_URL')`
- **THEN** the file calls `env API_URL`

#### Scenario: A project pack names the token its test replaces
- **WHEN** a project pack has a call rule on `settings.read` with `test_double = "Settings"`, one spec file has `{ provide: Settings, useClass: FakeSettings }` and calls `settings.read('PORT')`, and another spec file calls the same with no stand-in
- **THEN** only the second file calls the key

### Requirement: Mocks that replace a module
A `vi.mock` or `jest.mock` call that is a statement of its own at the top level of a file SHALL replace its module when its second argument is an arrow function or a function expression that does not use its first parameter, and the connector SHALL mark its mock as replacing. A call with no factory, a factory given by name or as an object, a factory that uses its first parameter or binds it in a pattern, a nested call, and any other mock call SHALL NOT replace it.

#### Scenario: A factory that does not use its parameter replaces the module
- **WHEN** `src/replacing.test.ts` has `vi.mock('./lib', () => ({ lib: 2 }))` on line 1, `jest.mock('./lib', function () { return { lib: 3 } })` on line 2 and `vi.mock('./lib', async (importOriginal) => ({ lib: 4 }))` on line 3, and `src/lib.ts` exists
- **THEN** each of the three calls is a mock of `./lib` at its line that replaces it, resolving to `src/lib.ts`

#### Scenario: A factory that uses its parameter does not replace the module
- **WHEN** `src/partial.test.ts` has `vi.mock('./lib', async (importOriginal) => ({ ...(await importOriginal()) }))` on line 1, `jest.mock('./lib', async (original) => { const lib = await original(); return lib })` on line 2 and `vi.mock('./lib', async ({ lib }) => ({ lib }))` on line 3, and `src/lib.ts` exists
- **THEN** each of the three calls is a mock of `./lib` at its line that does not replace it

#### Scenario: A mock without a factory does not replace the module
- **WHEN** `src/automock.test.ts` has `vi.mock('./lib')` on line 1, `vi.mock('./lib', factory)` on line 2 and `jest.mock('./lib', { spy: true })` on line 3, and `src/lib.ts` exists
- **THEN** each of the three calls is a mock of `./lib` at its line that does not replace it

#### Scenario: A nested mock call does not replace the module
- **WHEN** `src/nested.test.ts` has `vi.mock('./lib', () => ({}))` in the body of a `describe` callback on line 2 and `jest.mock('./lib', () => ({}))` in the body of a `beforeEach` callback on line 5, and `src/lib.ts` exists
- **THEN** each of the two calls is a mock of `./lib` at its line that does not replace it

#### Scenario: A mock that is not hoisted does not replace the module
- **WHEN** `src/late.test.ts` has `vi.doMock('./lib', () => ({}))` on line 1, `jest.doMock('./lib', () => ({}))` on line 2 and `jest.unstable_mockModule('./lib', () => ({}))` on line 3, and `src/lib.ts` exists
- **THEN** each of the three calls is a mock of `./lib` at its line that does not replace it

### Requirement: Loads named at run time
A file that takes a module by a name it does not spell out, with `import(name)`, `require(name)`, `vi.importActual(name)`, `jest.requireActual(name)` or another call of `vi` or `jest` that names a module, SHALL have no mock that replaces its module, because the load may be of that module as it is.

#### Scenario: A load named at run time leaves no mock replacing its module
- **WHEN** `src/import-name.test.ts` has `vi.mock('./lib', () => ({}))` on line 3 and `await import(name)` on line 5, `src/require-name.test.ts` has `jest.mock('./lib', () => ({}))` on line 3 and `require(name)` on line 5, `src/import-actual-name.test.ts` has `vi.mock('./lib', () => ({}))` on line 3 and `await vi.importActual(name)` on line 5, `src/require-actual-name.test.ts` has `jest.mock('./lib', () => ({}))` on line 3 and `jest.requireActual(name)` on line 5, and `src/lib.ts` exists
- **THEN** the mock of `./lib` on line 3 of each of the four files does not replace it
- **AND** the mock of `./lib` in a file with no such load, `src/replaced.test.ts`, does

### Requirement: Packs that find documents in the provides
The provides of a project SHALL name every pack whose `[[embeds]]` rules find a document, in `via` of the calls of the contract that document holds.

#### Scenario: The graphql pack is credited with the documents it finds
- **WHEN** a project that depends on `graphql-tag` has a file with a `gql` tagged template
- **THEN** the provides name `graphql` in `via` of the `graphql` calls

#### Scenario: The prisma pack is credited with the SQL it finds
- **WHEN** a project that depends on `@prisma/client` has a file with a `$queryRaw` tagged template
- **THEN** the provides name `prisma` in `via` of the `table` calls

### Requirement: A call rule that lists receiver types
A call rule MAY list `receiver_types`. It SHALL read a call only when the receiver is a name declared in the file with one of them, by the same rules as the root types of a chain rule, or `this.<name>` for a constructor parameter property. A rule that lists none SHALL read any receiver.

#### Scenario: A call rule reads only a receiver of a listed type
- **WHEN** a project pack has a call rule on `*.*.emit` with `receiver_types = ["EventEmitter2"]`, and one file calls `this.emitter.emit('change', x)` on a constructor parameter property of that type and another calls `this.kafka.emit('user.created', x)` on a `ClientKafka`
- **THEN** only the first calls `queue change`

### Requirement: Kafka consumers
With the `kafka` pack, active when the project depends on `kafkajs`, `@EventPattern(topic)` and `@MessagePattern(topic)` SHALL serve `queue <topic>`, and `subscribe({ topic })` on a name declared with `Consumer` SHALL serve it.

#### Scenario: A message pattern serves its topic
- **WHEN** a class has `@EventPattern('user.created')` and `@MessagePattern('user.get')`
- **THEN** the file serves `queue user.created` and `queue user.get`

#### Scenario: A consumer subscribed to a topic serves it
- **WHEN** a parameter `consumer: Consumer` calls `consumer.subscribe({ topic: 't' })`
- **THEN** the file serves `queue t`

#### Scenario: A project without kafkajs has no Kafka rules
- **WHEN** a project does not depend on `kafkajs` and a class has `@EventPattern('user.created')`
- **THEN** the file serves no contract and has no unread site

### Requirement: Kafka producers
With the `kafka` pack, `emit`, `send` and `emitMessage` on a name declared with `ClientKafka`, `ClientProxy`, `KafkaProducerService` or `Producer` SHALL call `queue <topic>`, the topic being the first argument or its `topic` property. A name that the file also binds to another kind, and a name with no declared type, SHALL be no producer.

#### Scenario: A client held in a constructor parameter property calls its topic
- **WHEN** a constructor has `private readonly kafka: ClientKafka` and a method calls `this.kafka.emit('user.created', x)`
- **THEN** the file calls `queue user.created`

#### Scenario: A client held in a class property calls its topic
- **WHEN** a class declares `@Inject('KAFKA') private readonly kafka: ClientKafka;` and calls `this.kafka.emit('user.deleted', {})`
- **THEN** the file calls `queue user.deleted`

#### Scenario: A producer service calls its topic
- **WHEN** a class has `private readonly kafkaProducerService: KafkaProducerService` and calls `this.kafkaProducerService.emitMessage('t', m)`
- **THEN** the file calls `queue t`

#### Scenario: A producer received as a parameter calls its topic
- **WHEN** a function has the parameter `producer: Producer` and calls `producer.send({ topic: 't', messages })`
- **THEN** the file calls `queue t`

#### Scenario: An event emitter is no producer
- **WHEN** a class has `private readonly emitter: EventEmitter2` and calls `this.emitter.emit('change', x)`
- **THEN** the file calls no contract and has no unread site

#### Scenario: A property of two types in two classes of one file is no producer
- **WHEN** a file has one class with `constructor(private readonly sender: ClientKafka) {}` and another with `constructor(private readonly sender: EventEmitter2) {}`, and both call `this.sender.emit`
- **THEN** the file calls no contract

#### Scenario: A name bound to two kinds in one file is no producer
- **WHEN** a file has `client: Producer` in one function and `client: EventEmitter2` in another, and both call `client.send` or `client.emit`
- **THEN** the file calls no contract

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

### Requirement: Clients made with an imported base URL
A call on a client that the file makes with `axios.create`, whose `baseURL` names a constant the file imports from a file of its project, SHALL be read under the text of that constant, as a call on a client another file makes is. A client whose `baseURL` reads as no text SHALL be read as it is without a base.

#### Scenario: A client made with an imported base URL is called
- **WHEN** `config.ts` exports `const API_URL = '/api/v1'`, and `users.ts` imports it and `axios`, declares `const api = axios.create({ baseURL: API_URL })` and calls `api.get('/users')` and `api.post('/orders')`
- **THEN** `users.ts` calls `http GET /api/v1/users` and `http POST /api/v1/orders` at the lines of the two calls
- **AND** it has no unread site

### Requirement: Imported names that a local binds again
A name that a file imports and also binds as a parameter, a variable, a function or a class SHALL be no import for the value a rule reads in the scope that binds it, so a site there that reads it stays as it is without the import: a computed part in a template or a concatenation, and unread when the name stands alone. Outside that scope the name SHALL stay the import.

#### Scenario: A name that a local binds again is no import
- **WHEN** `config.ts` exports `const BASE = '/api/v1'`, and `users.ts` imports it, declares ``function list(BASE: string) { return fetch(`${BASE}/users`) }`` and ``function items() { return fetch(`${BASE}/items`) }``, and calls ``fetch(`${BASE}/orders`)`` and `fetch(BASE)`
- **THEN** `users.ts` calls `http GET /users` at the line of the template in `list`
- **AND** the link gives `users.ts` the calls `http GET /api/v1/orders`, `http GET /api/v1` and `http GET /api/v1/items` at the lines of the other three sites
- **AND** it has no unread site

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
- **THEN** `use.ts` calls `http GET /end` followed by one suffix for each constant before the last, at the line of the call
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

### Requirement: Requests through request
With the `axios` pack, `axios.request(config)` and `request(config)` on a receiver made by `axios.create()`, in the file or imported from a file of the project, SHALL call the `url` of the config with its `method`, upper-cased, and `*` when absent. On a receiver that no `axios.create` makes it SHALL read nothing. A config that is no object literal SHALL be an unread site.

#### Scenario: A request with a config is a call
- **WHEN** a file has `axios.request({ url: '/bookings', method: 'POST' })` and `axios.request({ url: '/users' })`
- **THEN** it calls `http POST /bookings` and `http * /users`

#### Scenario: A request on a client made by axios.create is under its base
- **WHEN** a file has `const api = axios.create({ baseURL: '/api' })` and `api.request({ url: '/users', method: 'delete' })`
- **THEN** it calls `http DELETE /api/users`

#### Scenario: A request whose config is not read is an unread site
- **WHEN** a file has `axios.request(config)` with `config` a variable, and ``axios.request({ url: `/u/${id}` })``
- **THEN** the first is an unread site and the second calls `http * /u/{}`

#### Scenario: A request on a client made in another file is called
- **WHEN** `http.ts` has `export const api = axios.create({ baseURL: '/api' })` and `users.ts` imports `api` from `./http` and calls `api.request({ url: '/users', method: 'GET' })`
- **THEN** `users.ts` calls `http GET /api/users` at that line

#### Scenario: A request on a receiver that no axios.create makes is silent
- **WHEN** a file has `const cache = { request }` and calls `cache.request({ url: '/x' })`
- **THEN** the file has no contract site and no unread site

### Requirement: Raw SQL of the prisma pack
With the `prisma` pack, a template tagged `$queryRaw` or `$executeRaw`, the literal first argument of `$queryRawUnsafe` or `$executeRawUnsafe`, each on a receiver of up to three segments, and a template tagged `Prisma.sql`, SHALL be an embedded `sql` document at its line. A computed part of a template SHALL be written as `?` in the document.

#### Scenario: A raw query template is read as SQL with its computed parts written as placeholders
- **WHEN** `src/queries.ts` of a project that depends on `@prisma/client` has ``prisma.$queryRaw`SELECT id FROM a WHERE id = ${id}` `` on line 5 and, on line 8, ``prisma.$queryRaw`SELECT id FROM b WHERE id IN (${ids[0]}, ${ids[1]}) LIMIT ${limit}` ``
- **THEN** the file embeds a `sql` document at line 5 with the text `SELECT id FROM a WHERE id = ?`
- **AND** it embeds a `sql` document at line 8 with the text `SELECT id FROM b WHERE id IN (?, ?) LIMIT ?`
- **AND** the file has no unread site

#### Scenario: A raw query template with a type argument is read as SQL
- **WHEN** `src/queries.ts` has ``prisma.$queryRaw<{ id: string }[]>`SELECT id FROM c WHERE id = ${id}::uuid` `` on line 11 and ``prisma.$executeRaw`DELETE FROM d` `` on line 13
- **THEN** the file embeds a `sql` document at line 11 with the text `SELECT id FROM c WHERE id = ?::uuid`
- **AND** it embeds a `sql` document at line 13 with the text `DELETE FROM d`

#### Scenario: A raw query on a client held by this is read as SQL
- **WHEN** `src/service.ts` has ``this.prisma.$queryRaw`SELECT id FROM reports` `` on line 7 and ``this.db.client.$queryRaw`SELECT id FROM audits` `` on line 11
- **THEN** the file embeds a `sql` document at line 7 with the text `SELECT id FROM reports`
- **AND** it embeds a `sql` document at line 11 with the text `SELECT id FROM audits`

#### Scenario: An unsafe raw query with a literal is read as SQL
- **WHEN** `src/unsafe.ts` has `prisma.$queryRawUnsafe('SELECT id FROM a')` on line 5 and ``prisma.$executeRawUnsafe(`UPDATE b SET seen = true`)`` on line 7
- **THEN** the file embeds a `sql` document at line 5 with the text `SELECT id FROM a`
- **AND** it embeds a `sql` document at line 7 with the text `UPDATE b SET seen = true`

#### Scenario: A Prisma.sql template is read as SQL whole or in part
- **WHEN** `src/fragments.ts` has ``Prisma.sql`SELECT id FROM a WHERE id = ${id}` `` on line 3 and ``Prisma.sql`AND id = ${id}` `` on line 5
- **THEN** the file embeds a `sql` document at line 3 with the text `SELECT id FROM a WHERE id = ?`
- **AND** it embeds a `sql` document at line 5 with the text `AND id = ?`

#### Scenario: A Prisma.sql template passed to a raw query is read once
- **WHEN** `src/unsafe.ts` has ``prisma.$queryRaw(Prisma.sql`SELECT id FROM c WHERE id = ${id}`)`` on lines 11 and 12
- **THEN** the file embeds one `sql` document for it, at line 12, with the text `SELECT id FROM c WHERE id = ?`

#### Scenario: An unsafe raw query with a constant text is read as SQL
- **WHEN** `src/constant.ts` has `const TEXT = 'SELECT id FROM f'` and `prisma.$queryRawUnsafe(TEXT)` on line 7
- **THEN** the file embeds a `sql` document at line 7 with the text `SELECT id FROM f`

#### Scenario: A Prisma.sql template held in a name is read where it stands
- **WHEN** `src/held.ts` has ``const query = Prisma.sql`SELECT id FROM e` `` on line 5 and `prisma.$queryRaw(query)` on line 7
- **THEN** the file embeds one `sql` document, at line 5
- **AND** the file has no unread site

#### Scenario: A raw query is read only in a project that depends on the Prisma client
- **WHEN** `src/queries.ts` has ``prisma.$queryRaw`SELECT id FROM a` `` and the project does not depend on `@prisma/client`
- **THEN** the file embeds no document

### Requirement: Raw SQL built at run time
An `Unsafe` raw query whose text is built at run time, a template with a computed part or a variable that holds no constant, SHALL be an unread site at its line and embed no document. A `Prisma.sql` fragment that is no whole statement SHALL name no table.

#### Scenario: An unsafe raw query with a computed text is unread
- **WHEN** `src/unsafe.ts` has ``prisma.$queryRawUnsafe(`SELECT id FROM ${table}`)`` on line 9 and `prisma.$executeRawUnsafe(text)`, with `text` a parameter, on line 14
- **THEN** the file has an unread site at line 9 for `prisma.$queryRawUnsafe`, whose reason is that the text is not a literal
- **AND** it has one at line 14 for `prisma.$executeRawUnsafe`, with the same reason

#### Scenario: A Prisma.sql fragment that is no statement names no table
- **WHEN** a project has `model User` in its schema and a file with ``Prisma.sql`AND id = ${id}` ``
- **THEN** that file calls no table

### Requirement: One key for a table however a query names it
A table named by a raw query, by a model call and by a Prisma model SHALL be one contract key, in lower case, whether the query quotes the name or qualifies it with a schema.

#### Scenario: A raw query, a model call and a Prisma model name one table
- **WHEN** `prisma/schema.prisma` has `model User`, `src/model.ts` calls `prisma.user.findMany()`, `src/quoted.ts` has ``prisma.$queryRaw`SELECT id FROM "User" WHERE id = ${id}` `` and `src/qualified.ts` has ``prisma.$queryRaw`SELECT id FROM public."User"` ``
- **THEN** `table user` is declared by the schema alone
- **AND** it is called by `src/model.ts` through `prisma`, and by `src/qualified.ts` and `src/quoted.ts` through `sql`

### Requirement: Roots by callback
A chain rule MAY list callback roots, patterns of calls whose callback receives a root. The first parameter of a function or an arrow function given as the first argument of a call that a pattern matches, when it is a plain identifier, SHALL start a chain of the rule. The `prisma` pack lists `$transaction` on a receiver of one to three segments.

#### Scenario: The parameter of a transaction callback is a root whatever its name
- **WHEN** `src/untyped.ts` has `prisma.$transaction(async (client) => { await client.user.findMany(); })` with the call on line 7
- **THEN** the file calls `table user` at line 7

#### Scenario: A function expression on a nested receiver gives a root
- **WHEN** `src/nested.ts` has `this.prisma.$transaction(function (trx) { return trx.booking.create({ data }); })` with the call on line 6
- **THEN** the file calls `table booking` at line 6

#### Scenario: A callback parameter that the file binds elsewhere too is no root
- **WHEN** `src/shared.ts` has `const client = makeCache()` and a `$transaction` callback with the parameter `client`, and calls `client.user.findMany()` in both
- **THEN** the file calls no table

#### Scenario: The callback of another call is no root
- **WHEN** `src/others.ts` has `items.forEach((client) => client.user.findMany())`
- **THEN** the file calls no table

#### Scenario: A destructured callback parameter names no client
- **WHEN** `src/destructured.ts` has `prisma.$transaction(async ({ user }) => user.findMany())`
- **THEN** the file calls no table and the index is built

#### Scenario: The array form of a transaction reads as before
- **WHEN** `src/array.ts` has `prisma.$transaction([prisma.user.findMany()])` on line 6
- **THEN** the file calls `table user` at line 6

### Requirement: Mounts on an imported host
A `use` call on a name imported from a file of the project SHALL mount the routers, modules and controllers it lists when that name is the app or router another file makes with a call some rule serves routes on, found through re-exports, and SHALL read nothing otherwise: no mount and no unread site. A member of the name (`server.app.use`) and a name from a package are not read.

#### Scenario: A router mounted on an imported app serves under the mount
- **WHEN** `app.ts` has `export const app = express()`, `router.ts` has `export const router = express.Router()` and `router.get('/users', list)`, and `main.ts` imports both and calls `app.use('/api', router)`
- **THEN** `router.ts` serves `http GET /api/users` at the line of `router.get`
- **AND** `main.ts` has no unread site

#### Scenario: A router mounted on an imported router composes with the mounts above it
- **WHEN** `api.ts` and `sub.ts` export routers `api` and `sub` with `sub.get('/x', list)`, `main.ts` calls `app.use('/api', api)` on an app imported from `app.ts`, and `wiring.ts` imports `api` and `sub` and calls `api.use('/sub', sub)`
- **THEN** `sub.ts` serves `http GET /api/sub/x`

#### Scenario: A mount on an app imported through a re-export serves under the mount
- **WHEN** `index.ts` has `export { app } from './app'`, and `main.ts` imports `app` from `./index` and calls `app.use('/api', router)`
- **THEN** `router.ts` serves `http GET /api/users`

#### Scenario: A use on an imported name that is no router is silent
- **WHEN** `i18n.ts` has `export const i18n = i18next.createInstance()` and `setup.ts` imports it and calls `i18n.use(initReactI18next)`
- **THEN** the index has no mount and no unread site for the call

#### Scenario: A use on an import from a package is silent
- **WHEN** `setup.ts` has `import i18n from 'i18next'` and calls `i18n.use(initReactI18next)`
- **THEN** the index has no mount and no unread site for the call

#### Scenario: A use on an imported client that serves no routes is silent
- **WHEN** `client.ts` has `export const client = axios.create({ baseURL: 'https://api.example.com' })`, `router.ts` exports a router with `router.get('/users', list)`, and `setup.ts` calls `client.use('/api', router)` and ``client.use(`${process.env.BASE}/api`, router)``
- **THEN** `router.ts` serves `http GET /users`, not `/api/users`
- **AND** `setup.ts` has no unread site

#### Scenario: A use on a member of an imported name is silent
- **WHEN** `server.ts` has `export const server = { app: express() }`, `router.ts` exports a router with `router.get('/users', list)`, and `setup.ts` calls `server.app.use('/api', router)`
- **THEN** `router.ts` serves `http GET /users`
- **AND** `setup.ts` has no unread site

### Requirement: Computed prefixes on an imported host
A `use` call on an imported app or router whose prefix is not text SHALL be an unread site of its file, as it is on an app made in the file. The same call on an imported name that is no app or router SHALL read nothing.

#### Scenario: A computed prefix on an imported app is an unread site
- **WHEN** `main.ts` calls ``app.use(`${process.env.BASE}/api`, router)`` on an app imported from `app.ts`, and `setup.ts` calls ``i18n.use(`${process.env.BASE}/plugins`, plugin)`` on an `i18n` imported from a file that makes it with `createInstance()`
- **THEN** `main.ts` has an unread site at the line of the call
- **AND** `setup.ts` has no unread site

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

### Requirement: A chain is matched against roots of at most 64 segments
A chain of members SHALL be matched against the roots of the chain rules up to 64 segments long, whatever its length, so that the cost of a chain grows with its length and not with its square. A rule keeps the contract of a chain that starts with its root and goes on for any number of members.

#### Scenario: A long chain still matches a root of a rule
- **WHEN** a file reads `process.env.PORT` followed by 300 members
- **THEN** the file calls `env PORT` at its line
- **AND** the run has no problem line

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

### Requirement: Members read through a value of a known class
A name bound below the top level of a file by `const` to `new` of a class, or declared by a `const` or a parameter with a plain class type, SHALL hold that class: a member read from it SHALL be a use of `Class.member` only, the name read as a value a use of `Class`, and its type a use of `Class.`. `new` SHALL use `Class.constructor` when its value goes no further. `Class.member` on an imported name or on a class of the file with that member SHALL be a use of that member only.

#### Scenario: A member read from a new instance is a use of that member
- **WHEN** `refund` has `new OrderService().cancel(id)`
- **THEN** `refund` uses `OrderService.cancel` and `OrderService.constructor` and not `OrderService`

#### Scenario: A member read from a constant made by new is a use of that member
- **WHEN** `refund` has `const service = new OrderService(); service.cancel(id);`
- **THEN** `refund` uses `OrderService.cancel` and `OrderService.constructor` and not `OrderService`

#### Scenario: A member read from a parameter of a class type is a use of that member
- **WHEN** `refund` is `function refund(service: OrderService) { service.cancel(id); }`
- **THEN** `refund` uses `OrderService.cancel` and `OrderService.` and not `OrderService`

#### Scenario: A value of a class read as a value is a use of the class
- **WHEN** `pass` is `function pass(service: OrderService) { run(service); }` and `keep` has `const service = new OrderService(); run(service);`
- **THEN** `pass` and `keep` use `OrderService`

#### Scenario: A static member of an imported class is a use of that member
- **WHEN** `create` has `OrderService.create()`
- **THEN** `create` uses `OrderService.create` and not `OrderService`

#### Scenario: A static member of a class of the file is a use of that member
- **WHEN** the file declares `class Runner` with `static start()` and `static stop()`, and `begin` has `Runner.start()`
- **THEN** `begin` uses `Runner.start` and not `Runner`

#### Scenario: A type with arguments holds no class
- **WHEN** `load` is `function load(repo: Repository<User>) { repo.find(); }`, with `Repository` and `User` imported
- **THEN** `load` uses `Repository` and `User`

#### Scenario: A name bound twice in a scope holds no class
- **WHEN** a function has `const a = new OrderService(); const a = new OtherService(); a.cancel();`
- **THEN** it uses neither `OrderService.cancel` nor `OtherService.cancel`

### Requirement: Raw SQL of the SQL drivers
With the `sql-drivers` pack, a literal or template first argument of `query`, `execute`, `prepare` and `exec` on a receiver of up to three segments SHALL be an embedded `sql` document, a computed part as `?`. A first argument with no text SHALL record nothing. The pack SHALL apply to a project on `pg`, `mysql`, `mysql2`, `better-sqlite3`, `sqlite3`, `@neondatabase/serverless`, `mssql`, `@libsql/client`, `@planetscale/database`, `oracledb`, `sequelize`, `postgres` or `@vercel/postgres`.

#### Scenario: A driver query is read as SQL
- **WHEN** `src/users.ts` has `db.query('SELECT id FROM users WHERE id = $1', [id])` on line 4 and `this.pool.query('DELETE FROM sessions')` on line 8
- **THEN** the file embeds a `sql` document at line 4 with the text `SELECT id FROM users WHERE id = $1` and one at line 8 with the text `DELETE FROM sessions`

#### Scenario: A driver query calls the tables it names
- **WHEN** a project that depends on `pg` has `db.query('SELECT id FROM users')` in one file and `pool.query('DELETE FROM sessions')` in another
- **THEN** each file calls its table, `users` and `sessions`

#### Scenario: A query kept in a constant is read
- **WHEN** `src/constant.ts` has `const LIST = 'SELECT id FROM accounts'` and `db.query(LIST)`
- **THEN** the file embeds a `sql` document with that text at the line of the call

#### Scenario: A query template is read with its computed parts as placeholders
- **WHEN** `src/template.ts` has ``pool.query(`SELECT id FROM orders WHERE id = ${id}`)`` on line 3 and ``pool.query(`SELECT id FROM ${table}`)`` on line 5
- **THEN** the file embeds the `sql` documents `SELECT id FROM orders WHERE id = ?` at line 3 and `SELECT id FROM ?` at line 5
- **AND** it has no unread site

#### Scenario: A computed table names no table
- **WHEN** a file has ``pool.query(`SELECT id FROM ${table}`)``
- **THEN** the file calls no table and has no unread site

#### Scenario: A call with no text in its first argument records nothing
- **WHEN** `src/silent.ts` has `client.query({ query: GET_USERS })` and `pool.query(text)` with `text` a parameter
- **THEN** the file embeds no document and has no unread site

#### Scenario: A literal that is no SQL names no table
- **WHEN** a project has `cp.exec('ls -la')` beside files that call the tables `users`, `sessions` and `orders`
- **THEN** the tables of the project are those three and no more
- **AND** the file with `cp.exec` calls none and has no unread site

#### Scenario: A project without a SQL driver reads no query
- **WHEN** a project that depends on no driver has `db.query('SELECT id FROM users')`
- **THEN** the file embeds no document

#### Scenario: The sql-drivers pack is credited with the tables it finds
- **WHEN** a project that depends on `pg` has a file that calls `db.query('SELECT id FROM users')`
- **THEN** the provides name `sql-drivers` in `via` of the `table` calls

### Requirement: A qualified type name and a JSX member are used by their name

The connector SHALL record a use of `lib.Foo` for the qualified type name `lib.Foo`, for the type query `typeof lib.Foo` and for the tag `<lib.Foo />`, as it does for the value `lib.Foo`, and not a use of the bare root `lib`. A root that is no binding records nothing.

#### Scenario: A qualified type name is used by its whole name

- **WHEN** a function writes `lib.Foo`, `Array<lib.Foo>` or `typeof lib.helper` in a type position, where `lib` is a namespace import
- **THEN** the function uses `lib.Foo`, or `lib.helper`, and not `lib`
- **AND** a value typed `lib.Foo` that is returned or passed on uses `lib.Foo` and not `lib`
- **AND** a qualified name whose root is no binding is no use

#### Scenario: A JSX member is used by its whole name

- **WHEN** a function renders `<lib.Comp />`, where `lib` is a namespace import
- **THEN** the function uses `lib.Comp` and not `lib`

### Requirement: Heirs
The connector SHALL record, for a name, the class or interface whose members it takes: the head of the `extends` of a class, each head of the `extends` of an interface, the class a top-level `const` holds, and `default` for `export default new Service()`. A head SHALL be the dotted name without type arguments. `implements`, a call such as `mixin(Base)`, `let`, `var` and a value below the top level SHALL record nothing.

#### Scenario: A class records what it extends
- **WHEN** a file has `class Plain extends Base<string>`, `class Qualified extends lib.Base`, `class Mixed extends mixin(Base)` and `class Promised implements Alone`
- **THEN** it records `Plain` of `Base` and `Qualified` of `lib.Base`
- **AND** it records no heir for `Mixed` or `Promised`

#### Scenario: An interface records what it extends
- **WHEN** a file has `interface Wide extends Base, lib.Shape` and `export default interface Named extends Base`
- **THEN** it records `Wide` of `Base`, `Wide` of `lib.Shape` and `Named` of `Base`

#### Scenario: A top-level value records the class it holds
- **WHEN** a file has `export const made = new Service()`, `const typed: Service = make()`, `const generic = new Repository<string>()`, `let later = new Service()`, `var oldest = new Service()` and a `const` in a function, and another has `export default new Service()`
- **THEN** it records `made` and `typed` of `Service` and `generic` of `Repository`
- **AND** it records nothing for the `let`, the `var` and the `const` in a function
- **AND** the other file records `default` of `Service`

### Requirement: Members of the head
In a class with a head, `super.member` SHALL be a use of `Head.member` even when the class declares that member, `this.member` of a member the class does not declare a use of `Head.member`, and `super(...)` a use of `Head.constructor`. A member read from a name that has a head SHALL be used by that name. A class with no head SHALL record none of these.

#### Scenario: `super` and an inherited `this` name the member of the head
- **WHEN** `class Sub extends Base` has `place`, which calls `super.place()`, `this.place()` and `this.own()` for its own `own`, a constructor that calls `super(x)`, and `class Headless` calls `this.go()` and `super.go()`
- **THEN** `Sub.place` uses `Base.place` and `Sub.own`, and `Sub.constructor` uses `Base.constructor`
- **AND** `Headless.run` uses nothing

#### Scenario: A member read from a name with a head is used by its name
- **WHEN** a file has `export const orders = new Service()` and a function that calls `orders.place()`, and a function that calls `Sub.make()` for a class `Sub` that extends `Base`
- **THEN** the first function uses `orders.place`
- **AND** the second uses `Sub.make`

### Requirement: A fetch with no method is a GET
A `fetch` that writes no `method`, with no options or with options that hold none, SHALL call `http GET <url>`, as the Fetch standard makes it a GET. A `fetch` whose options cannot be read, such as a variable, a spread or a `method` that is not text, SHALL call `http * <url>`.

#### Scenario: A fetch that writes no method is a GET
- **WHEN** a file has `fetch('/x')` and `fetch('/x', { headers })`
- **THEN** it calls `http GET /x` at the line of each call

#### Scenario: A fetch whose options cannot be read keeps any method
- **WHEN** a file has `fetch('/x', options)`, `fetch('/x', { method: verb })` and `fetch('/x', { ...options })`
- **THEN** it calls `http * /x` at the line of each call

### Requirement: Sites that wait for the link are named
The connector SHALL name, for each file, the places that only the link turns into facts: a site left to the link, a route on an Express host, a route of a class without the decorator, and a tRPC procedure listed in a router. A route that a controller declares SHALL not be named, because its fact stands without the link.

#### Scenario: A site left to the link is named as waiting for it
- **WHEN** a file calls `fetch(ROUTES.users)` on a constant imported from another file, another file registers `app.get('/health')` on an Express app, and a NestJS controller declares `@Get('all')`
- **THEN** the first file names the call at its line, and the second names the route at its line
- **AND** the controller names nothing

### Requirement: Reads of members
A file SHALL record each member name it reads on a value of a class it does not know: a name after the first of a chain, `x['m']`, a key of an object pattern, and the string a call of `spyOn`, `stub` or `spy` takes second. `this`, `super`, a known class, an object literal key and a computed key SHALL record none. `T['m']` as a type SHALL use `T.m`.

#### Scenario: A file records the members it reads on a value of unknown class
- **WHEN** a file has `x.a`, `x?.b`, `x['c']`, `const { d } = x`, `({ e } = x)`, `function f({ g }: T)`, `jest.spyOn(x, 'h')`, `<x.i />` and `a.j.k`
- **THEN** the file records the reads `a`, `b`, `c`, `d`, `e`, `g`, `h`, `i`, `j`, `k` and `spyOn`

#### Scenario: A read of a known class or of the class itself records no name
- **WHEN** a class has `this.own()`, `super.up()`, `this.repo.save()` with a typed field, and `y.known()` with `const y = new Service()`, and the file has `{ key: 1 }` and `x[k]`
- **THEN** the file records no read

#### Scenario: An indexed type names the member
- **WHEN** a file has `type P = Service['place']`
- **THEN** `P` uses `Service.place`

#### Scenario: A member of a field of a class below the top level is read on an unknown value
- **WHEN** a class declared inside a function has the field `service` and its method calls `this.service.update()`
- **THEN** the file records the read `update`

### Requirement: Imports under another name
A call rule SHALL read a name imported from a package a pack detects under the name the package gives it: the package for a default or namespace import or a `require`, the exported name for `{ a as b }`. A Node module, a path inside a package, a package whose name is no identifier, a name from a file of the project and a name a scope binds again SHALL keep their name.

#### Scenario: A renamed default import of a package is read
- **WHEN** a file has `import http from 'axios'`, `http.get('/a')`, `const api = http.create({ baseURL: '/v1' })` and `api.get('/d')`
- **THEN** it calls `http GET /a` and `http GET /v1/d`

#### Scenario: A namespace import of a package is read
- **WHEN** a file has `import * as ax from 'axios'` and `ax.post('/b')`
- **THEN** it calls `http POST /b`

#### Scenario: A renamed require of a package is read
- **WHEN** a file has `const axiosLib = require('axios')` and `axiosLib.put('/c')`
- **THEN** it calls `http PUT /c`

#### Scenario: A renamed destructured require of a package is read
- **WHEN** a file has `const { Router: R } = require('express')`, `const router = R()` and `router.get('/f', h)`
- **THEN** it serves `http GET /f`

#### Scenario: Renamed imports of a router are read
- **WHEN** a file has `import ex, { Router as R } from 'express'`, `const app = ex()`, `app.get('/e', h)`, `const router = R()` and `router.get('/f', h)`
- **THEN** it serves the same routes as a file that spells the names `express` and `Router`

#### Scenario: A name no pack detects keeps its own name
- **WHEN** a file has `import fetch from 'node-fetch'`, `fetch('/g')`, `import http from 'http'` and `http.get('/h')`
- **THEN** it calls `http GET /g` and nothing else

#### Scenario: A renamed import a scope binds again is not read
- **WHEN** a file has `import http from 'axios'` and `function f(http) { http.get('/i'); }`
- **THEN** it calls nothing

### Requirement: CommonJS exports
The connector SHALL record the names a file exports where its CommonJS form lists them: `exports.a =`, `module.exports.a =`, the keys of `module.exports = { … }`, and for a local object its keys too; and `default` for a function, a class, a call, a local or another value. It SHALL leave them unknown for an object with a spread or a computed key, a local object that is empty or has one, `require(…)`, an indexed assignment, and `exports` handed to a call other than the `__esModule` marker.

#### Scenario: The names of a CommonJS file are listed
- **WHEN** one file has `exports.d = 1` and `module.exports.e = f`, and another has `module.exports = { a, b: x, c() {} }`
- **THEN** the exports of the first are `d` and `e`, and those of the second are `a`, `b` and `c`

#### Scenario: A CommonJS file that exports one value exports default
- **WHEN** a file has `module.exports = class {}`, `module.exports = function () {}`, `module.exports = main()` or `module.exports = new Foo()`, or `module.exports = run` for a function `run`, a class `Foo` or a constant `make` that holds an arrow function
- **THEN** its exports are `default`

#### Scenario: CommonJS exports that cannot be listed are unknown
- **WHEN** a file has `module.exports = { ...a }`, `module.exports = { [k]: 1 }`, `module.exports = lib` for a constant that holds an object with a spread or none, `module.exports = require('./a')`, `exports[k] = 1`, `module.exports[k] = 1`, `Object.assign(module.exports, x)`, `factory(exports)` or `Object.defineProperty(exports, 'x', …)`, or has `export =`
- **THEN** its exports are unknown
- **AND** a module with export declarations keeps the names of them

#### Scenario: `module.exports` of a local object lists the keys of the object
- **WHEN** a file has `const codes = { A: make(), B: make() }`, `module.exports = codes` and `module.exports.append = append`
- **THEN** its exports are `A`, `B`, `append` and `default`

#### Scenario: An empty `module.exports` lists what is added to it
- **WHEN** a file has `module.exports = {}`, `module.exports.x = 1` and `exports.y = 2`
- **THEN** its exports are `x` and `y`

#### Scenario: The compiled module marker leaves the exports known
- **WHEN** a file has `Object.defineProperty(exports, '__esModule', { value: true })` and `exports.a = 1`
- **THEN** its exports are `a`

### Requirement: A require binds its names
A `require` at the top level of a file, whose result a declarator binds to a plain identifier or to an object pattern of plain names without a rest, a default or a computed key, SHALL bind locals as an import does: it records the names it takes and the locals it binds them to, declares no name, and a use of `local.x` is a use of `x` of that local. Any other `require` SHALL record no locals.

#### Scenario: A require at the top level binds locals
- **WHEN** a file has `const lib = require('./a')` and `const { b, c: d } = require('./a')` at its top level, and `run` uses `lib.x`
- **THEN** the first import takes `default` as `lib` and the second takes `b` and `c` as `b` and `d`
- **AND** the file declares neither `lib`, `b` nor `d`, and `run` uses `lib.x`

#### Scenario: A require that cannot bind stays as it was
- **WHEN** a file has `const { a, ...rest } = require('./a')`, `const { a: first = 1 } = require('./a')`, `const { a: { b } } = require('./a')`, and `const lib = require('./a')` inside a function
- **THEN** none of the four records a local

### Requirement: Packages required in place
A call rule SHALL read a callee that starts at `require('pkg')` with a literal argument under the name a pack writes for the package, and a local bound to a member of such a require under that name. A package no pack reads, a path of the project, a Node module, a path inside a package and a computed member SHALL give no fact.

#### Scenario: A member of a package required in place is called
- **WHEN** a function has `return require('axios').get('/c')`
- **THEN** it calls `http GET /c`

#### Scenario: A router required in place serves its routes
- **WHEN** a file has `const router = require('express').Router()` and `router.get('/a', h)`
- **THEN** it serves `http GET /a`

#### Scenario: An application required in place serves its routes
- **WHEN** a file has `const app = require('express')()` and `app.get('/b', h)`
- **THEN** it serves `http GET /b`

#### Scenario: A member of a package bound to a name is called
- **WHEN** a file has `const get = require('axios').get` and `get('/d')`
- **THEN** it calls `http GET /d`

#### Scenario: A member of a project module bound to a name is read as the export
- **WHEN** a file has `const client = require('./named').client`, `client.get('/users')`, and `./named` exports a client made by `axios.create({ baseURL: '/named' })`
- **THEN** it calls `http GET /named/users`

#### Scenario: A require in place that no pack reads gives no fact
- **WHEN** a file has `require('left-pad').get('/lp')` and `require('./x').get('/y')`
- **THEN** it has no contract

#### Scenario: A computed member of a required package gives no fact
- **WHEN** a file has `require('axios')[name]('/x')`
- **THEN** it has no contract

### Requirement: CommonJS declarations
The connector SHALL declare the names a top-level statement writes in place on `exports`: `exports.a = value` and `module.exports.a = value` declare `a` over the statement, a class with its members. `module.exports = function f() {}` and `= class F {}` SHALL declare `f` or `F` and export `default` from it, and `= { … }` SHALL declare each key written in place with its own lines. A local, a `require`, a computed key and a spread declare nothing.

#### Scenario: A function written in place on exports is declared
- **WHEN** a file has `exports.run = function () { return dep() }` over three lines, `exports.a = exports.b = () => dep()`, `module.exports.limit = 5`, `exports.local = dep` and `exports.loaded = require('./other')`
- **THEN** it declares `run` over its three lines, `a` and `b` over their statement, and `limit`, and not `local` or `loaded`
- **AND** `run` uses `dep` and what the body of `run` uses is not read as the file loads, matching the reviewed snapshot

#### Scenario: An object assigned to module.exports declares each property written in place
- **WHEN** a file has `module.exports = { run() { return dep() }, stop: function () { return other() }, limit: 5, Bar: class { go() { return dep() } } }`
- **THEN** it declares `run`, `stop`, `limit`, `Bar` and `Bar.go`, each over the lines of its own property
- **AND** `run` uses `dep`, `stop` uses `other` and `Bar.go` uses `dep`, matching the reviewed snapshot

#### Scenario: A property that holds a local or a require declares nothing
- **WHEN** a file has `module.exports = { a, b: local, c, d: require('./d') }` for a function `a`, a `require` of `local` and a name `c` that a `require` binds
- **THEN** it declares none of `b`, `c` and `d`
- **AND** the names the properties hold are read as the file loads

#### Scenario: A computed key declares nothing
- **WHEN** a file has `module.exports = { ['computed']() {}, run() {} }`
- **THEN** it declares `run` and not `computed`

#### Scenario: A class written in place on exports is declared under its exported name
- **WHEN** a file has `exports.Foo = class Inner { run() { return dep() } }`
- **THEN** it declares `Foo` and `Foo.run`, and `Foo.run` uses `dep`

#### Scenario: A class assigned to module.exports is declared and exported as default
- **WHEN** a file has `module.exports = class Foo { go() {} }`, and another has `module.exports = function run() {}`
- **THEN** the first declares `Foo` and `Foo.go` and exports `default` from `Foo`
- **AND** the second declares `run` and exports `default` from `run`, matching the reviewed snapshot

#### Scenario: An anonymous function assigned to module.exports declares nothing
- **WHEN** a file has `module.exports = function () { return dep() }`
- **THEN** it declares no name for the function

#### Scenario: A call on exports is a use of the name the file declares
- **WHEN** a file has `exports.run = function () {}` and `exports.again = function () { return exports.run() }`
- **THEN** `again` uses `run`

#### Scenario: A void placeholder on exports is no declaration
- **WHEN** a file has `exports.a = void 0;`, then `function a() {}` over three lines, then `exports.a = a;`
- **THEN** it declares `a` over the lines of the function only

### Requirement: Destructured members of a module
A destructuring of a name with an object pattern of static keys and no rest element SHALL use `name.key` for each key, as the member chains do, and not the name whole. A rest element or a computed key SHALL keep the use of the name whole. A default value in the pattern SHALL be read. A destructuring of a local object SHALL read as before.

#### Scenario: A destructured namespace uses the members it names
- **WHEN** a function has `const { a, b: c } = lib` and `lib` is `import * as lib from './lib'`
- **THEN** it uses `lib.a` and `lib.b`, and not `lib`

#### Scenario: A destructured require uses the members it names
- **WHEN** a function has `const { a } = lib` and `lib` is `const lib = require('./lib')`
- **THEN** it uses `lib.a`

#### Scenario: A destructured default import uses the members it names
- **WHEN** a function has `const { a } = utils` and `utils` is `import utils from './utils.cjs'`
- **THEN** it uses `utils.a`

#### Scenario: A destructuring that may take any name uses the module whole
- **WHEN** a function has `const { a, ...rest } = lib`, and another has `const { [key]: a } = lib`
- **THEN** each uses `lib`

#### Scenario: A destructuring inside a function counts for that function
- **WHEN** one function has `const { a } = lib` and another does not
- **THEN** the first uses `lib.a` and the second uses nothing

#### Scenario: A default value in a destructuring is still read
- **WHEN** a function has `const { a = helper() } = lib`
- **THEN** it uses `lib.a` and `helper`

#### Scenario: A destructuring of a local object changes nothing
- **WHEN** a function has `const { a } = config` and `config` is a `const` object of the same file
- **THEN** it uses `config`

### Requirement: Overrides
The connector SHALL record, for a class, the heads a value of which may run its members: the head of its `extends` and each type it `implements`, as the source spells the name without type arguments. An interface, a top-level value and a class with neither SHALL record none.

#### Scenario: A class records what it extends and implements
- **WHEN** a file has `class Aws extends Base implements Provider, lib.Other<string>`, `interface Wide extends Base` and `class Plain {}`
- **THEN** it records `Aws` of `Base`, `Aws` of `Provider` and `Aws` of `lib.Other`
- **AND** it records no override for `Wide` or `Plain`

### Requirement: Constants of other packages in the link
The link of a TypeScript project SHALL read a name declared in a file of another package that its files import, in a URL and in the path of a mount, as it reads a name of its own project. A name that cannot be read SHALL leave the URL or the mount as unread as before.

#### Scenario: A URL reads a constant of another package
- **WHEN** a file imports `API_PREFIX` and `USERS` from another package and calls `axios.get` with a template and with a concatenation of them
- **THEN** it calls the paths the constants spell

#### Scenario: A mount reads a constant of another package
- **WHEN** an app mounts a router with `app.use(API_PREFIX, users)` and `API_PREFIX` is declared in another package
- **THEN** the router serves its routes under the constant

#### Scenario: A prefix that reads through another package leaves no unread site
- **WHEN** an app mounts a router at a template made of a constant of another package of the repository and a literal
- **THEN** the router serves under the path the template reads as, and the app has no unread site

#### Scenario: A prefix whose name comes from an outside package stays an unread site
- **WHEN** an app mounts a router at a template made of a name that a package outside the repository declares
- **THEN** the mount is one unread site of the app

### Requirement: Routers of other packages in the link
An application SHALL serve the routes of a router of another package under the path it mounts the router at, below the router's own mounts, and a router that no file mounts SHALL keep the path it is written with. The routes of a router that several applications mount SHALL be served under each mount.

#### Scenario: A router of another package mounted by an app serves under the mount
- **WHEN** an app mounts a router of another package at `/ext`, and the router has `get('/x')`
- **THEN** the router serves `http GET /ext/x` and not `http GET /x`

#### Scenario: A router of another package with a router of its own below it chains the prefixes
- **WHEN** an app mounts a router of another package at `/ext`, and that router mounts a second router at `/inner` which has `get('/x')`
- **THEN** the second router serves `http GET /ext/inner/x`

#### Scenario: Two apps mounting one router give both paths
- **WHEN** two apps mount one router of another package at `/api` and at `/admin`
- **THEN** the router serves its route under both mounts

#### Scenario: Routers imported by name under a mount each serve under it
- **WHEN** a router passes two imported routers `ra` and `rb` to its `use`, each with one route, and an app mounts that router at `/w`
- **THEN** the routes are served as `http GET /w/ra` and `http GET /w/rb`

#### Scenario: A router of another package that no app mounts keeps its path
- **WHEN** a router of another package is imported and no file mounts it
- **THEN** it serves the path it is written with

### Requirement: Controllers of other packages in the link
A NestJS application SHALL serve a controller of another package behind its global prefix when the modules it imports reach the controller. A controller no application reaches SHALL keep the path it is written with. A package that is not in the repository SHALL give no site and no unread site.

#### Scenario: A controller of another package serves behind the global prefix of the app that reaches it
- **WHEN** an app sets the global prefix `api` and imports a module of another package that holds a controller at `items`, and another controller of that package is in no module
- **THEN** the first serves `http GET /api/items/{}` and the second serves `http GET /legacy/{}`

#### Scenario: An outside package imported by name adds no site and no blind spot
- **WHEN** a file imports a router from a package that is not in the repository and mounts it
- **THEN** the file gets no new site and no new unread site

### Requirement: Raw SQL given to a driver in an object
With the `sql-drivers` pack, the `sql` or `text` property of an object literal that is the first argument of `query`, `execute`, `prepare` or `exec` SHALL be an embedded `sql` document, as a literal or template first argument is. An object with neither property, and a first argument that is no object literal, SHALL record nothing. A property that holds no text SHALL be an unread site.

#### Scenario: A query held in an object is read
- **WHEN** a file has `pool.query({ text: 'SELECT id FROM invoices', values: [1] })` on line 4, and another has `client.execute('SELECT id FROM notes')` on line 3 and `client.execute({ sql: 'SELECT id FROM tags WHERE id = ?', args: [id] })` on line 5
- **THEN** the first file embeds `SELECT id FROM invoices` at line 4
- **AND** the other embeds `SELECT id FROM notes` at line 3 and `SELECT id FROM tags WHERE id = ?` at line 5

#### Scenario: A query in an object that is not text is an unread site
- **WHEN** a file has `pool.query({ text: build() })`
- **THEN** the file has an unread site at the call and embeds no document

### Requirement: Raw SQL of a template tagged sql and of a call on a value
With the `sql-drivers` pack, a template tagged `sql`, `x.sql` or `sql.unsafe` SHALL be an embedded `sql` document, and so SHALL the first argument of `query`, `execute`, `prepare` or `exec` called on a call, a `new` or an `await`, such as `pool.request().query(…)`. A template that is no statement SHALL record no table. A regular expression `exec` SHALL record nothing.

#### Scenario: A template tagged sql is read
- **WHEN** a project that depends on `postgres` has ``sql`SELECT id FROM users WHERE id = ${id}` `` on line 3, and one that depends on `@vercel/postgres` has ``sql`SELECT id FROM users` `` on line 4 and ``db.sql`SELECT id FROM posts` `` on line 6
- **THEN** the first embeds `SELECT id FROM users WHERE id = ?` at line 3
- **AND** the second embeds `SELECT id FROM users` at line 4 and `SELECT id FROM posts` at line 6

#### Scenario: A fragment tagged sql that is no statement records nothing
- **WHEN** a file has ``sql`lower(${x})` ``
- **THEN** the file calls no table and has no unread site

#### Scenario: A call on a value no name stands for is read
- **WHEN** a file has `getDb().query('SELECT id FROM audits')` on line 3, and a project that depends on `mssql` has `pool.request().input('id', sql.Int, id).query('SELECT id FROM orders WHERE id = @id')` on line 5, `new sql.Request().query('SELECT id FROM refunds')` on line 7 and ``sql.query`SELECT id FROM carts WHERE id = ${id}` `` on line 9
- **THEN** the first file embeds `SELECT id FROM audits` at line 3
- **AND** the `mssql` file embeds `SELECT id FROM orders WHERE id = @id` at line 5, `SELECT id FROM refunds` at line 7 and `SELECT id FROM carts WHERE id = ?` at line 9

#### Scenario: A regular expression exec reads nothing
- **WHEN** a file has `/SELECT/.exec('SELECT id FROM nothing')`
- **THEN** the file embeds no document and has no unread site

#### Scenario: A call on a value and a sql tag read nothing without a driver
- **WHEN** a project that depends on no driver has `getDb().query('SELECT id FROM audits')` and ``sql`SELECT id FROM carts` ``
- **THEN** the file embeds no document

### Requirement: Raw SQL of the other SQL drivers
With the `sql-drivers` pack, the first argument of `execute` of `@planetscale/database` and `oracledb`, and of `query` of `sequelize`, SHALL be an embedded `sql` document, bind markers such as `?` and `:id` included.

#### Scenario: A query of another SQL driver is read
- **WHEN** a project that depends on `@planetscale/database` has `conn.execute('SELECT id FROM users WHERE id = ?', [id])`, one on `oracledb` has `connection.execute('SELECT id FROM employees WHERE id = :id', [id])`, and one on `sequelize` has `sequelize.query('SELECT id FROM projects', { type: QueryTypes.SELECT })`
- **THEN** each file embeds its statement at the line of the call

### Requirement: Members read through what a factory gives
A static call on a class, `Service.make()`, SHALL give a value of that class, and a call of a member on a value of a known class SHALL stay on it. A top-level function, or a `const` holding a function, SHALL be an heir of the class its return type or its `new` returns, and a top-level `const x = f()` of a name of the file an heir of `f`. A lower-case name and a namespace import SHALL give no class.

#### Scenario: A member read from a constant made by a static call is a use of that member
- **WHEN** `viaConstant` has `const s = Service.make(); s.place();`
- **THEN** it uses `Service.place` and neither `Service.other` nor `Service`

#### Scenario: A member read from the result of a static call is a use of that member
- **WHEN** a function has `Service.getInstance().place()`, `(await Service.create()).place()` or `(Service.make()).place()`
- **THEN** it uses `Service.place`

#### Scenario: A chain of calls stays on the class
- **WHEN** a function has `new FixTracker(a).retain(b).remove(c)`, `const t = new FixTracker(a); t.retain(b).remove(c)` or `Service.make().with(1).place()`
- **THEN** it uses `FixTracker.retain` and `FixTracker.remove`, or `Service.with` and `Service.place`

#### Scenario: A call on a lower-case name or a namespace import names no class
- **WHEN** a function has `logger.child().info()` or `lib.make().place()` with `lib` imported as a namespace
- **THEN** the file reads `info` and `place` on a value of an unknown class and uses no member of `Service`

#### Scenario: A function that gives a class is an heir of it
- **WHEN** a file has a function whose return type is `Service`, `Promise<Service>` or `Service | null`, a function or an arrow that returns `new Service()`, and a function that returns `new Service()` in a branch
- **THEN** each is an heir of `Service`

#### Scenario: A function that gives a class only from a nested function is no heir
- **WHEN** a function returns an arrow that returns `new Service()`, or returns `new Missing()` of a class the file does not bind
- **THEN** it is an heir of nothing

#### Scenario: A top-level value made by a call is an heir of what it calls
- **WHEN** a file has `export const made = typed()` with `typed` a function of the file, `export const waited = await promised()` and `export const fromStatic = Service.make()`
- **THEN** `made` is an heir of `typed`, `waited` of `promised` and `fromStatic` of `Service`
- **AND** a call of a name the file does not bind, and of a member of a namespace import, gives no heir

#### Scenario: A function required with CommonJS is a use of its class
- **WHEN** a file has `const { makeService } = require('./service'); makeService().place();` and `const lib = require('./service'); lib.makeService().place();`
- **THEN** the declarations use `makeService` and `lib.makeService`, and the file reads `place`
