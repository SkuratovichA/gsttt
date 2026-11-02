## ADDED Requirements

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
