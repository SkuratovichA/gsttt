# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The TypeScript connector

- [x] 2.1 A top-level `exports.x = value` declares its names, with one group for a chain
- [x] 2.2 `module.exports = function run() {}` and `module.exports = class Foo {}` declare the name they carry and export `default` from it
- [x] 2.3 `module.exports = { … }` declares each key written in place, with the lines and the uses of its own property
- [x] 2.4 `exports.run()` is a use of `run`
- [x] 2.5 `READER_VERSION` 24 to 25
- [x] 2.6 `exports.a = void 0` declares nothing, and `exports.a.b = 1` exports no name

## 3. The scenarios

- [x] 3.1 TypeScript: a fixture and a test for each scenario of "CommonJS declarations"
- [x] 3.2 Queries: a test for the scenario of "Names a CommonJS file writes in place"
