# Spec Delta

## Purpose

The TypeScript connector reads TypeScript and JavaScript into imports and contract sites. It parses with oxc, resolves specifiers the way TypeScript and Node would inside the repository, and finds HTTP routes and requests, GraphQL operations, tRPC procedures, tables and environment variables through rule packs.

## ADDED Requirements

### Requirement: Projects
The connector SHALL open one project per `package.json` in the file list, and one at the root when the repository has TypeScript or JavaScript files and no root `package.json`. A project SHALL claim the files with the extensions `ts`, `tsx`, `mts`, `cts`, `js`, `jsx`, `mjs` and `cjs` under its root, and its own `package.json` and `tsconfig*.json` files, which it tags as manifests.

#### Scenario: Every package is a project
- **WHEN** a workspace has `package.json`, `apps/web/package.json` and `packages/ui/package.json`
- **THEN** the connector opens projects rooted at the repository root, `apps/web` and `packages/ui`
- **AND** `packages/ui/src/button.tsx` belongs to `packages/ui`

### Requirement: Project configs
The connector SHALL read the project's `tsconfig.json` and every config it extends; a config it cannot read SHALL be recorded as that file's parse failure, and the project SHALL go on with the options it could read.

#### Scenario: A broken tsconfig fails alone
- **WHEN** `apps/web/tsconfig.json` is not valid JSON with comments
- **THEN** that file's status is a syntax failure with its line
- **AND** the other files of `apps/web` are parsed and resolved

#### Scenario: A config extends another
- **WHEN** `apps/web/tsconfig.json` extends `../../tsconfig.base.json`
- **THEN** `apps/web/tsconfig.json` has an import that resolves to `tsconfig.base.json`

### Requirement: Imports
The connector SHALL read static imports, type-only imports, side-effect imports, re-exports, `export *` re-exports, `import()` and `require()` with a literal specifier, `import x = require()` and triple-slash path references, each with its line, its kind and the names it imports, and SHALL flag type-only imports. An `import()` or `require()` whose specifier is not a literal SHALL be recorded as an unread site computed at run time.

#### Scenario: Every import form is read
- **WHEN** a file holds one import of each form listed above
- **THEN** its facts hold each import with its kind, line and names, matching the reviewed snapshot

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
The connector SHALL resolve specifiers against the repository's file list only, never the disk: relative paths with TypeScript's extension and index probing, a `.js`, `.mjs` or `.cjs` specifier finding its TypeScript source, `paths` and `baseUrl` from the project's config chain, and workspace packages by name through `exports`, `types`, `module` and `main`, mapping an entry under the package's `outDir` back to its source under `rootDir`.

#### Scenario: Relative specifiers find TypeScript sources
- **WHEN** `src/a.ts` imports `./util`, `./util.js` and `./components`, and the files `src/util.ts` and `src/components/index.tsx` exist
- **THEN** the first two resolve to `src/util.ts` and the third to `src/components/index.tsx`

#### Scenario: Path aliases follow the config chain
- **WHEN** `tsconfig.base.json` at the root maps `@db/*` to `packages/db/src/*`, `apps/web/tsconfig.json` extends `../../tsconfig.base.json`, and `apps/web/src/page.ts` imports `@db/client`
- **THEN** the import resolves to `packages/db/src/client.ts`, because a mapping is read relative to the config that declares it

#### Scenario: Workspace packages resolve to their sources
- **WHEN** `packages/ui/package.json` is named `@acme/ui` with `exports` mapping `./button` to `./src/button.tsx`, and `apps/web` imports `@acme/ui/button`
- **THEN** the import resolves to `packages/ui/src/button.tsx`

#### Scenario: Built entries map back to sources
- **WHEN** the workspace package `@acme/db` has `main` set to `dist/index.js`, its tsconfig has `outDir` `dist` and `rootDir` `src`, and another package imports `@acme/db`
- **THEN** the import resolves to `packages/db/src/index.ts`

### Requirement: Packages and builtins
A specifier naming a package declared in the project's or the root's manifest SHALL resolve to that package, a Node builtin to a builtin, a bare name declared nowhere to an unknown package, and a path that names no file to not found.

#### Scenario: Packages, builtins and unknown names are told apart
- **WHEN** `apps/web` declares `react` and `react-dom` and imports `react`, `react-dom/client`, `node:fs`, `path`, `left-pad` and `./nope`
- **THEN** `react` and `react-dom/client` resolve to the packages `react` and `react-dom`, `node:fs` and `path` to builtins, `left-pad` to an unknown package and `./nope` to not found

### Requirement: HTTP routes
With the `express` pack, `get`, `post`, `put`, `patch`, `delete`, `options`, `head` and `all` called with a path on a receiver made by `express()`, `express.Router()` or `Router()` SHALL serve `http <METHOD> <path>`. A router mounted with `use('<prefix>', router)` SHALL have its routes served under the prefix, through any number of mounts and across files.

#### Scenario: An Express route is served
- **WHEN** a file has `const app = express(); app.get('/health', h)`
- **THEN** it serves `http GET /health` at that line

#### Scenario: A call on another receiver is no route
- **WHEN** a file has `cache.get('/health')` and `cache` is not made by Express
- **THEN** the file has no contract site at that line

#### Scenario: A router mounted in another file serves under the mount
- **WHEN** `bookings.ts` has `const router = Router(); router.get('/:id', h); export default router`, `api.ts` has `const api = Router(); api.use('/bookings', bookings)` with `bookings` imported from `./bookings`, and `app.ts` has `app.use('/api', api)` with `api` imported from `./api`
- **THEN** `bookings.ts` serves `http GET /api/bookings/{}` at the line of `router.get`

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
With the `trpc` pack, a property of an object passed to a router builder (`router`, `t.router`, `createTRPCRouter`) whose value ends in `.query`, `.mutation` or `.subscription` SHALL serve `trpc <path>`, where the path joins the property names from the router no other router includes, through routers in other files.

#### Scenario: Composed routers serve full paths
- **WHEN** `booking.ts` exports `bookingRouter = router({ get: publicProcedure.input(s).query(f) })`, and `root.ts` has `appRouter = router({ booking: bookingRouter })`
- **THEN** `booking.ts` serves `trpc booking.get` at the line of the `get` property

### Requirement: tRPC calls
With the `trpc` pack, member chains from `trpc` or `api` ending in a client call (`useQuery`, `useMutation`, `useSuspenseQuery`, `useInfiniteQuery`, `query`, `mutate`, `subscribe`) SHALL call `trpc <path>`.

#### Scenario: A client call names its procedure
- **WHEN** a component has `api.booking.get.useQuery({ id })`
- **THEN** it calls `trpc booking.get`

### Requirement: Tables
With the `prisma` pack, a member chain `prisma.<model>`, `this.prisma.<model>`, `ctx.prisma.<model>` or `ctx.db.<model>` followed by a client method SHALL call `table <model>`. With the `typeorm` pack, `@Entity('<name>')` SHALL declare `table <name>` and a bare `@Entity()` the class name. With the `drizzle` pack, `pgTable`, `mysqlTable` and `sqliteTable` SHALL declare `table <first argument>`.

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
A pack SHALL be active in a project when the project's `package.json` or the root's declares one of the pack's `detect` packages in any dependency section, or always when its `detect` is empty. Each project SHALL report what it provides, with each active pack under its kind and role and the connector's blind spots in words.

#### Scenario: Packs follow the manifest
- **WHEN** `apps/api` declares `@nestjs/common` and `apps/web` declares neither `@nestjs/common` nor `express`
- **THEN** NestJS decorators in `apps/web` produce no sites and the same decorators in `apps/api` do

#### Scenario: Blind spots are reported
- **WHEN** the coverage of a TypeScript project is read
- **THEN** its provides list the active packs and name, among the blind spots, file-system routes and routers mounted from another package
