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
`link` SHALL return the routes under the prefixes their routers are mounted at and the tRPC procedures as new facts that name no parsed fact, so that a fact of any rule on the line of a route keeps its site.

#### Scenario: A fact of another rule on the line of a route is kept
- **WHEN** one line of `app.ts` has `app.get('/b', h)` on an Express app, `fetch('/c')` and a `@Get('a')` method of a `@Controller('x')` class
- **THEN** `app.ts` serves `http GET /b` and `http GET /x/a` and calls `http * /c`, all at that line

### Requirement: NestJS routes
With the `nestjs` pack, `@Get`, `@Post`, `@Put`, `@Patch`, `@Delete`, `@Options`, `@Head` and `@All` on a method of a `@Controller('<prefix>')` class SHALL serve the prefix and the path joined.

#### Scenario: A NestJS controller serves its routes
- **WHEN** a class decorated `@Controller('bookings')` has a method decorated `@Get(':id')`
- **THEN** the file serves `http GET /bookings/{}` at the line of `@Get`

### Requirement: HTTP requests
With the `fetch` pack, `fetch` SHALL call `http <method> <url>`, the method taken from the options' `method` and `*` when absent. With the `axios` pack, `axios.get`, `axios.post` and the other verbs, and the same verbs on a receiver made by `axios.create()`, SHALL call the URL with that verb. A URL built from a template SHALL keep its literal parts with each computed part as a parameter; a URL with no literal path SHALL be an unread site.

#### Scenario: Requests become calls
- **WHEN** a file has ``fetch(`/bookings/${id}`, { method: 'DELETE' })`` and `axios.post('/bookings', body)`
- **THEN** it calls `http DELETE /bookings/{}` and `http POST /bookings`

#### Scenario: A base URL in a template is dropped
- **WHEN** a file has ``fetch(`${API_URL}/bookings`)``
- **THEN** it calls `http * /bookings`

#### Scenario: A computed URL is a blind spot
- **WHEN** a file has `fetch(url)`
- **THEN** the file has an unread site at that line with the reason computed at run time

### Requirement: GraphQL
With the `nestjs` pack, `@Query`, `@Mutation` and `@Subscription` on a method SHALL serve `graphql <Root>.<name>`, the name taken from the decorator's `name` option or else the method's name.

#### Scenario: A resolver method serves a root field
- **WHEN** a `@Resolver()` class has `@Query(() => [Booking]) bookings()` and `@Mutation(() => Booking, { name: 'cancelBooking' }) cancel()`
- **THEN** the file serves `graphql Query.bookings` and `graphql Mutation.cancelBooking`

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
