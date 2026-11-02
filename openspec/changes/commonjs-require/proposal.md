# Proposal: Read a client that a CommonJS file exports and another requires

## Why

A call on an HTTP client that another CommonJS file makes was read for some shapes of the export and not for others, while the ES module form of each shape is read. `module.exports = axios.create(…)`, `exports.client = axios.create(…)` and `const clients = require('./http-client'); clients.httpClient.get(…)` gave no call, so an older Node service had calls missing from the graph and the question "who calls this route" got a wrong answer.

## What Changes

- A call assigned to `module.exports`, to `exports.name`, to `module.exports.name` or to a property of `module.exports = { name: call }` is a client made under the export name, as `export default <call>` is one made under `default`. The export is a local of that name, so the files that require it read their calls on it.
- The reading of the assignment target (`module.exports`, `module.exports.x`, `exports.x`) is one function, `commonjs_exports`, which the export list and the makers of the file both use.
- A member read from a name bound to `default`, as a `require` or a default import binds it, is the export of that name when the module has no `default` of its own: `clients.httpClient` reads the export `httpClient`, as `esModuleInterop` reads it. A module with a `default` is read as before.
- A new requirement, `CommonJS clients`, with a scenario for each of the three shapes.
- The version of the reader goes from 16 to 17, because the parse hands the engine different facts for the same text. `FACT_FORMAT` stays: core applies its rules as before.

## Impact

- Capability `typescript`.
- `crates/lang-ts/` (`shape.rs`, `analyze.rs`, `analyze/makers.rs`, `link/deferred.rs`, `project.rs`, fixtures, tests and snapshots).
- A file that assigns a call to its exports lists them as unknown, as before, and now also stores that call as the local of the export name.
- A mount of a name bound by `require` and followed by a member (`app.use('/x', routes.users)`) resolves through `define`, which reads a name as a whole, and is not changed here.
- Indexes built by an earlier version are parsed again once.
