## MODIFIED Requirements

### Requirement: A require binds its names
A `require` at the top level of a file, whose result a declarator binds to a plain identifier or to an object pattern of plain names without a rest, a default or a computed key, SHALL bind locals as an import does: it records the names it takes and the locals it binds them to, declares no name, and a use of `local.x` is a use of `x` of that local. Any other `require`, but those of the next requirement, SHALL record no locals.

#### Scenario: A require at the top level binds locals
- **WHEN** a file has `const lib = require('./a')` and `const { b, c: d } = require('./a')` at its top level, and `run` uses `lib.x`
- **THEN** the first import takes `default` as `lib` and the second takes `b` and `c` as `b` and `d`
- **AND** the file declares neither `lib`, `b` nor `d`, and `run` uses `lib.x`

#### Scenario: A require that cannot bind stays as it was
- **WHEN** a file has `const { a, ...rest } = require('./a')`, `const { a: first = 1 } = require('./a')`, `const { a: { b } } = require('./a')`, and `const lib = require('./a')` inside a function
- **THEN** none of the four records a local

## ADDED Requirements

### Requirement: A require binds in more forms
A `require` in either branch of a conditional that a declarator binds SHALL bind its names as at the top level. A `require` assigned with `=` to a local of the file, at any depth, SHALL take the name it reads, or `default` when it reads none. A `require` read by a computed member SHALL bind the module as a namespace, unless the file uses its local other than to read a member from it, which keeps the module whole.

#### Scenario: A require in a conditional binds its names
- **WHEN** a file has `const { A, B } = typeof window === 'undefined' ? require('./a') : {}` on line 1 and `const lib = flag ? require('./b') : require('./c')` on line 2
- **THEN** the first import takes `A` and `B` as `A` and `B`, and the other two take `default` as `lib`

#### Scenario: A require assigned to a local of the file takes its name
- **WHEN** a file has `let Linter = null` and `let ns = null` at its top level, and a function with `Linter = require('./d').Linter` on line 6 and `ns = require('./g')` on line 7
- **THEN** the import on line 6 takes `Linter` and the import on line 7 takes `default`

#### Scenario: A require read by a computed member binds the module
- **WHEN** a file has `const pick = require('./e')[key]` on line 9 and `const other = require('./f')[Symbol.iterator]` on line 10
- **THEN** the first import takes `default` as `pick` and the second takes `default` as `other`

### Requirement: A member read on a builtin is no read
A member read on a global builtin the file never binds, such as `Object` or `Buffer`, on a local bound to a Node module, and on a local every value of which is a global builtin or made by a Node module SHALL NOT be recorded as a read. A name the file binds in any scope, and a local also given another value, SHALL be unknown, and its reads SHALL stay.

#### Scenario: A member read on a global builtin records no name
- **WHEN** a file has `Object.entries(source)`, `Buffer.from(text)`, `Reflect.ownKeys(copy)` and `store.get(pairs)` for `const store = new WeakMap()`
- **THEN** the file records no read

#### Scenario: A member read on a proxy or on globalThis records the name
- **WHEN** a file has `wrapped.proxied()` for `const wrapped = new Proxy(lib, {})` and `globalThis.held.hung()`
- **THEN** the file records the reads `held`, `hung` and `proxied`

#### Scenario: A member read on a global name the file binds records the name
- **WHEN** a file has `function show(Object) { Object.entries(1) }`, and `const Buffer = make()` and `Buffer.from(2)`
- **THEN** the file records the reads `entries` and `from`

#### Scenario: A member read on a Node module or on a value made by one records no name
- **WHEN** a file has `const path = require('node:path')`, `const server = http.createServer()` for `http` of `node:http`, `createServer()` for `createServer` of `node:http`, and `late = new Map()` for `let late`, and calls a member of each
- **THEN** the file records none of those reads
- **AND** it records the reads of a local given `new Map()` and then `make()`, of a local given `make()`, of a local bound to `./path` and of a parameter
