# Spec Delta

## ADDED Requirements

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
