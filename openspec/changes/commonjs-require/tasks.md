# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The reader

- [x] 2.1 `crates/lang-ts/src/shape.rs`: `commonjs_exports` reads an assignment target into export names and values
- [x] 2.2 `crates/lang-ts/src/analyze.rs`: `commonjs_export` uses it, and a call is exported as the local of its export name
- [x] 2.3 `crates/lang-ts/src/analyze/makers.rs`: the makers learn the CommonJS targets
- [x] 2.4 `crates/lang-ts/src/project.rs`: `READER_VERSION` 16 to 17

## 3. The link

- [x] 3.1 `crates/lang-ts/src/link/deferred.rs`: a member of a name bound to a `default` the module lacks is read as an export

## 4. The tests

- [x] 4.1 Fixtures, tests and snapshots of the three scenarios, with the exports left unknown, an ES module left as it was and a module with a `default` still read as its default
