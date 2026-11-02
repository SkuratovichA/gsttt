# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas of `indexing`, `typescript`, `python` and `rust`

## 2. The rule

- [x] 2.1 `crates/core/src/rules.rs`: `Access`, `ChainRule::written` left out of the pack when unset, `ChainRule::apply` by access, `Pack::contracts`, with the unit tests of the three scenarios of `Writes of a chain`
- [x] 2.2 `crates/config`: the pinned pack schema follows

## 3. TypeScript

- [x] 3.1 `crates/lang-ts/src/analyze.rs`: the access of the chain a target, an update, a destructuring target, a loop head or a `delete` names, and the read-write of a value that is used
- [x] 3.2 `crates/lang-ts/packs/env.toml`: `written = "serves"` and `vi.stubEnv`
- [x] 3.3 `crates/lang-ts/src/provides.rs`: made from `Pack::contracts`, with the blind spots of the variables that are written
- [x] 3.4 `crates/lang-ts/src/project.rs`: `READER_VERSION` 8 to 9
- [x] 3.5 Fixtures, tests and snapshots of the four scenarios of `Environment writes` and of `Writes to a Prisma client`

## 4. Python

- [x] 4.1 `crates/lang-python/src/parse.rs`: the access of a subscript by its context, and one unread site for a computed name that two rules match
- [x] 4.2 `crates/lang-python/packs/env.toml`: `written = "serves"`, the serving call rule, and `pop` as a read
- [x] 4.3 `crates/lang-python/src/provides.rs`: made from `Pack::contracts`, with the blind spots of the variables that are written
- [x] 4.4 `crates/lang-python/src/lib.rs`: `version` 2 to 3
- [x] 4.5 Tests of the three scenarios of `Environment writes`

## 5. Rust

- [x] 5.1 `crates/lang-rust/src/sites/chain.rs`: the left side of an assignment is a write
- [x] 5.2 `crates/lang-rust/packs/env.toml`: the serving call rule for `set_var` and `remove_var`
- [x] 5.3 `crates/lang-rust/src/provides.rs`: made from `Pack::contracts`, with the blind spot of names built at run time
- [x] 5.4 `crates/lang-rust/src/lib.rs`: `version` 3 to 4
- [x] 5.5 Fixtures, tests and snapshots of the two scenarios of `Environment writes`

## 6. The docs

- [x] 6.1 `docs/architecture.md` says what a chain rule names for a read, a write and a read-write

## 7. The review

- [x] 7.1 `crates/lang-ts/src/analyze.rs`: the values a statement, a `for` head or update, a `void` and a sequence drop are marked while their parent is walked; the fixture lines and the two scenarios of `A value that the code drops`
- [x] 7.2 `crates/core/src/rules.rs`: a chain cut by the `segments` limit of a rule with a `written` role is read; the unit tests, the scenario of `A write below a variable` in `typescript` and in `python`
- [x] 7.3 `crates/core/tests/queries/impact.rs`: the scenario of `A file that sets and reads a contract`, with no change to a query
- [x] 7.4 The provides of Python and Rust name `patch.dict(os.environ, …)`, `monkeypatch.setitem(os.environ, …)` and `temp_env::with_var` as variables that are not read
- [x] 7.5 `ChainRule::apply` gives one fact for a role that is read and written, and `Pack::contracts` takes its roles from the same method
- [x] 7.6 `Pack::contracts` leaves the `[[imports]]` rules out and `crates/lang-ts/src/provides.rs` adds them, with a test
- [x] 7.7 The requirement and the scenario of the compound assignment used as a value say what they mean, and the cache keys are `READER_VERSION` 9, Python 3 and Rust 4
- [x] 7.8 The proposal states the cost of the forms that are still not read
