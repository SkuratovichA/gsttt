## ADDED Requirements

### Requirement: Environment writes
With the `env` pack, a literal name that a site assigns, updates, destructures into, loops into or deletes, and `vi.stubEnv("<NAME>", …)`, SHALL serve `env <NAME>` and not call it. An assignment or an update whose value is used SHALL call and serve it. `Object.assign(process.env, …)`, `jest.replaceProperty(process.env, …)` and `process.env = {…}` SHALL not be read, which the provides of the project name as a blind spot of the variables it serves.

#### Scenario: A write of an environment variable serves it
- **WHEN** a file has `process.env.A = 'x'`, `process.env['B'] ??= 'y'`, `delete process.env.C`, `vi.stubEnv('D', 'z')` and `({ E: process.env.F } = values)`, each as a statement
- **THEN** it serves `env A`, `env B`, `env C`, `env D` and `env F`, calls no `env` and leaves no unread site

#### Scenario: Every form of assignment target is a write
- **WHEN** a file has `process.env.G! = 'v'`, `(process.env.H as string) = 'v'`, `process.env.I += ':x'`, `process.env.J++`, `for (process.env.K of values) {}` and `[process.env.L] = values`
- **THEN** it serves `env G`, `env H`, `env I`, `env J`, `env K` and `env L` and calls none

#### Scenario: An assignment used as a value reads and serves
- **WHEN** a file has `export const url = (process.env.API_URL ??= 'http://localhost')`, `export const retries = () => (process.env.RETRIES ??= '3')` and `export const next = ++process.env.COUNT`
- **THEN** it calls and serves `env API_URL`, `env RETRIES` and `env COUNT`

#### Scenario: A computed name in a write is one unread site
- **WHEN** a file has `process.env[process.env.TARGET] = 'v'` and, in a function with the parameter `name`, `process.env[name] = 'v'`
- **THEN** it calls `env TARGET`, serves no `env`, and leaves one unread site for each statement

### Requirement: Writes to a Prisma client
A site that assigns to a member of a Prisma client, as a test does with `prisma.tenant.findFirst = vi.fn()`, SHALL name no table, because the assignment queries nothing.

#### Scenario: A write to a Prisma client names no table
- **WHEN** a file has `const prisma = new PrismaClient()`, `prisma.tenant.findFirst = vi.fn()` and `export const all = () => prisma.booking.findMany()`
- **THEN** it calls `table booking` and no other table
