# Spec Delta

## ADDED Requirements

### Requirement: Receivers made in another file
A call that a rule reads on a receiver, written on a name the file imports from a file of the same project, SHALL be read on what that file makes under the name: an `axios.create()` client calls under its `baseURL`, and an Express app or router serves its routes under its own mounts. A call on an imported name that no such call makes SHALL be read as the rules without a receiver read it, or SHALL be silent when none does.

#### Scenario: A client made in another file is called
- **WHEN** `http.ts` has `export const api = axios.create({ baseURL: '/api' })` and `users.ts` imports `api` from `./http` and calls `api.get('/users')`
- **THEN** `users.ts` calls `http GET /api/users` at that line

#### Scenario: A router made in another file serves under its mounts
- **WHEN** `router.ts` has `export const router = express.Router()`, `users.ts` imports `router` from `./router` and calls `router.get('/users', list)`, and `app.ts` has `const app = express()` and `app.use('/api', router)` with `router` imported from `./router`
- **THEN** `users.ts` serves `http GET /api/users` at the line of `router.get`

#### Scenario: A call on an imported name that no rule makes is silent
- **WHEN** `cache.ts` has `export const cache = new Map()`, and `a.ts` has `import { cache } from './cache'` and `import * as store from './cache'` and calls `cache.get('k')` and `store.cache.get('k')`
- **THEN** `a.ts` has no contract site and no unread site at those lines

#### Scenario: A call on the default import of a file that makes a client is the client's
- **WHEN** `lib/http.ts` has `export default axios.create({ baseURL: '/api' })` and `users.ts` has `import axios from './lib/http'` and `axios.get('/users')`, and `lib/axios.ts` has `import axios from 'axios'; export default axios` and `orders.ts` has `import axios from './lib/axios'` and `axios.get('/orders')`
- **THEN** `users.ts` calls `http GET /api/users` and not `http GET /users`
- **AND** `orders.ts` calls `http GET /orders`
