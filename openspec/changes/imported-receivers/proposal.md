# Proposal: Read the calls on a client or a router that another file makes

## Why

A client or a router is usually made once and imported where it is used: `export const httpClient = axios.create({ baseURL })` in `http-client.ts`, and `httpClient.post(…)` in the file that logs a build; `export const router = express.Router()` in one file, `router.get('/users', list)` in another and `app.use('/api', router)` in a third. A rule that takes its prefix from the receiver looked only at variables made in the file of the call, so none of these calls was read: a call to a service was missing from the index, a route was missing from the service that serves it, and a change to the handler looked as if nothing reached it.

## What Changes

- A call that a rule reads on a receiver, written on a name the file imports from a file of the same project, is read on what that file makes under the name. An `axios.create()` client calls under its `baseURL`, read against the constants of the file that makes it, and an Express app or router serves its routes under every prefix its own mounts give it, while the site stays in the file that registers the route.
- The parse keeps such a call for the link, and the file that makes a client or a router keeps, beside its exports, what it was made by. A name imported from Node, or from a package that an active pack is for, is read in the parse as before, so `import axios from 'axios'; axios.get('/x')` stays a fact of the parse.
- A call on an imported name that no such call makes (`export const cache = new Map()`) is silent, and so is a member of an imported client (`api.v1.get`).
- A rule without a receiver reads the same call too (`axios.get`), so for an imported name it stays a fallback that the link applies only when the name is not a client or a router that some rule reads. `import axios from './lib/http'` with `export default axios.create({ baseURL: '/api' })` calls `/api/users`, not `/users`, while `import axios from './lib/axios'` that re-exports the package reads as the package does.
- `export default express.Router()` is a router the file exports by the name `default`, so a mount of it in another file reaches it.
- The coverage text of the http contracts says what is read and what is not: a mount on an app or router imported from another file is still not read.
- The version of the reader goes from 5 to 6, because the data kept for the link phase changes.

## Impact

- Capability `typescript`.
- `crates/lang-ts/` (the analyzer, `TsExtra`, the link phase, the coverage texts, fixtures and tests), `crates/gsttt/tests/cli`, and the lines of `README.md` and `docs/architecture.md` that say a client or a router must be made in the file that uses it.
- Indexes built by an earlier version are parsed again once. A tree with no call on a name imported from a file of its project gives the same index as before, except for the version and the coverage text.
