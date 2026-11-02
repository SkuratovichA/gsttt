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
- **THEN** `app.ts` serves `http GET /b` and `http GET /x/a` and calls `http * /c`, all at that line

#### Scenario: A route behind a global prefix keeps the facts of other rules on its line
- **WHEN** `main.ts` sets the global prefix `api` on an app made by `NestFactory.create(AppModule)`, `AppModule` lists `UserController` in its `controllers`, and one line of `user.controller.ts` has a `@Get(':id')` method of a `@Controller('users')` class whose body calls `fetch('/c')`
- **THEN** `user.controller.ts` serves `http GET /api/users/{}` and calls `http * /c`, both at that line
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
With the `fetch` pack, `fetch` SHALL call `http <method> <url>`, the method taken from the options' `method` and `*` when absent. With the `axios` pack, `axios.get`, `axios.post` and the other verbs, and the same verbs on a receiver made by `axios.create()`, SHALL call the URL with that verb. A URL built from a template SHALL keep its literal parts with each computed part as a parameter; a URL with no literal path, or whose path is computed in every segment, SHALL be an unread site.

#### Scenario: Requests become calls
- **WHEN** a file has ``fetch(`/bookings/${id}`, { method: 'DELETE' })`` and `axios.post('/bookings', body)`
- **THEN** it calls `http DELETE /bookings/{}` and `http POST /bookings`

#### Scenario: A base URL in a template is dropped
- **WHEN** a file has ``fetch(`${API_URL}/bookings`)``
- **THEN** it calls `http * /bookings`

#### Scenario: A computed URL is a blind spot
- **WHEN** a file has `fetch(url)`
- **THEN** the file has an unread site at that line with the reason computed at run time

#### Scenario: A URL whose path is only computed segments is a blind spot
- **WHEN** a file has ``fetch(`${API_URL}/${id}`)`` and ``fetch(`${base}${period}/${name}`)``
- **THEN** each is an unread site at its line and the file calls nothing for them
- **AND** ``fetch(`${API_URL}/users/${id}`)`` still calls `http * /users/{}`

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
The connector SHALL record the names a file exports itself, sorted: its exported declarations, the names of its `export { … }` lists, `default`, and the names its named re-exports and its `export * as` give, but not the names an `export *` brings. A named re-export SHALL record the name it exports each imported name as. The connector SHALL record the exports of a file as unknown when the file assigns `module.exports` or `exports`, has `export =`, or was read past syntax errors.

#### Scenario: Every export form is named
- **WHEN** a file has `export const a = 1`, `export const { b, c: d } = obj`, `export declare const g: number`, `export function f() {}`, `export class C {}`, `export enum E {}`, `export type T = string`, `export interface I {}`, `export namespace N {}`, `export default 1`, `export { x as y }`, `export { z as w } from './z'`, `export * as ns from './ns'` and `export * from './all'`
- **THEN** its exports are `C`, `E`, `I`, `N`, `T`, `a`, `b`, `d`, `default`, `f`, `g`, `ns`, `w` and `y`
- **AND** its re-export of `./z` records `z` exported as `w`, matching the reviewed snapshot

#### Scenario: CommonJS exports are unknown
- **WHEN** one file assigns `module.exports = { a }` and another assigns `exports.b = 1`
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
The connector SHALL record the declarations of a file, each with the lines it spans: every top-level function, class, variable, enum, interface, type alias and namespace, each name a destructuring declares, `default` for `export default` of an expression, and every member of a class as `Class.member`. Overloads and merged declarations SHALL be joined by name. A file read past syntax errors SHALL record none.

#### Scenario: Every declaration form is recorded
- **WHEN** a file has `function f() {}`, two overloads and the body of `g`, `const { a, b: c } = obj`, `class C { constructor() {} m() {} static s = 1; #p = 2; get v() { return 1 } set v(x) {} }`, `enum E {}`, `interface I {}` twice, `type T = string`, `namespace N {}` and `export default 1`
- **THEN** its declarations are `C`, `C.#p`, `C.constructor`, `C.m`, `C.s`, `C.v`, `E`, `I`, `N`, `T`, `a`, `c`, `default`, `f` and `g`, each with its lines, matching the reviewed snapshot

#### Scenario: A file read past errors records no declarations
- **WHEN** a file declares `foo`, imports `dep` and has a syntax error
- **THEN** it records no declarations
- **AND** its static imports record no local names

### Requirement: What a declaration uses
A declaration SHALL record, once each, the names it refers to that the file declares or binds by import: identifiers, JSX tags, type references and `typeof`, `Class.member` for `this.member` inside the class, and both `C.s` and `C` for `C.s`. An export specifier SHALL NOT count, nor a use of the declaration itself. Shadowing SHALL be ignored.

#### Scenario: References, tags, types and this are uses
- **WHEN** a file imports `useX`, `Props` and `Widget`, declares `helper`, `function View(p: Props) { const x = useX(); return <Widget value={helper(x)} /> }`, `class C { a() { this.b() } b() {} static s() {} }` and `function d() { return C.s() }`
- **THEN** `View` uses `Props`, `Widget`, `helper` and `useX`, `C.a` uses `C.b`, and `d` uses `C` and `C.s`, matching the reviewed snapshot

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
The connector SHALL read the first argument of `vi.importActual` and `jest.requireActual` as a dynamic import when it is a string or a template with no expression, because both load the module as it is. Used as it stands, such an import SHALL name nothing. A computed first argument SHALL be neither an import nor an unread site, also when a member is read from it.

#### Scenario: An actual module is a dynamic import
- **WHEN** `src/actual.test.ts` has `vi.importActual('./lib')` on line 1 and `jest.requireActual('./lib')` on line 2, and `src/lib.ts` exists
- **THEN** each of the two calls is a dynamic import of `./lib` at its line, with no names, resolving to `src/lib.ts`

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
- **THEN** it calls `http POST /build-runner/success` and `http * /users`

#### Scenario: A URL built from a template that starts with a base URL keeps its path
- **WHEN** a file has ``fetch(new URL(`${baseUrl}/users`))`` and ``fetch(new URL(`${API}/orders`, window.location.origin))``
- **THEN** it calls `http * /users` and `http * /orders`

#### Scenario: A relative path joins the directory of a literal base
- **WHEN** a file has `fetch(new URL('users/' + id, 'https://api.example.com/v1/'))` and `fetch(new URL('users', 'https://api.example.com/v1'))`
- **THEN** it calls `http * /v1/users/{}` and `http * /users`

#### Scenario: A URL built from a computed path is an unread site
- **WHEN** a file has `fetch(new URL(path, base))`
- **THEN** the file has an unread site at that line and calls nothing for it

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
A class decorator whose prefix is not read, such as `@Controller(PREFIX)` with `PREFIX` imported, SHALL be an unread site at its line, and its class SHALL serve no route, neither its own nor one it inherits; `@Controller()` and `@Controller({ version: '1' })` give no prefix.

#### Scenario: A controller whose prefix is not read serves nothing
- **WHEN** `auth.controller.ts` has `@Controller(PREFIX)`, with `PREFIX` imported, and methods `@Get('login')` and `@Get(LOGIN)`, with `LOGIN` imported
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
With the `fetch` and `axios` packs, a relative URL with no base SHALL be read as the path from the root, as a browser resolves it against the page: `fetch('items')` calls `http * /items`. A URL with a `..` segment, or whose path starts with a computed part after a dropped `./`, SHALL be an unread site, and a target that names no path (`data:…`, `?x=1`) SHALL be no request. An absolute URL SHALL be read as before.

#### Scenario: A relative request with no base calls the rooted path
- **WHEN** a file has `fetch('items')`, `axios.get('users')`, ``fetch(`items/${id}`)``, `bare.get('users/me')` on `const bare = axios.create()` and `unread.get('users')` on `axios.create({ baseURL: BASE })` with `BASE` not a literal
- **THEN** it calls `http * /items`, `http GET /users`, `http * /items/{}`, `http GET /users/me` and `http GET /users`

#### Scenario: A relative request that cannot be rooted is unread
- **WHEN** a file has `fetch('../items')` and ``fetch(`./${id}`)``
- **THEN** each is an unread site at its line

#### Scenario: A request target that names no path is no request
- **WHEN** a file has `fetch('data:text/plain,x')` and `fetch('?x=1')`
- **THEN** the file has no contract site and no unread site at those lines

#### Scenario: An absolute request is unchanged by relative requests
- **WHEN** a file has `fetch('https://a.test/x')`
- **THEN** it calls `http * /x`
