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
- [x] 3.4 `crates/lang-ts/src/project.rs`: `READER_VERSION` 7 to 8
- [x] 3.5 Fixtures, tests and snapshots of the four scenarios of `Environment writes` and of `Writes to a Prisma client`

## 4. Python

- [x] 4.1 `crates/lang-python/src/parse.rs`: the access of a subscript by its context, and one unread site for a computed name that two rules match
- [x] 4.2 `crates/lang-python/packs/env.toml`: `written = "serves"`, the serving call rule, and `pop` as a read
- [x] 4.3 `crates/lang-python/src/provides.rs`: made from `Pack::contracts`, with the blind spots of the variables that are written
- [x] 4.4 `crates/lang-python/src/lib.rs`: `version` 1 to 2
- [x] 4.5 Tests of the three scenarios of `Environment writes`

## 5. Rust

- [x] 5.1 `crates/lang-rust/src/sites/chain.rs`: the left side of an assignment is a write
- [x] 5.2 `crates/lang-rust/packs/env.toml`: the serving call rule for `set_var` and `remove_var`
- [x] 5.3 `crates/lang-rust/src/provides.rs`: made from `Pack::contracts`, with the blind spot of names built at run time
- [x] 5.4 `crates/lang-rust/src/lib.rs`: `version` 2 to 3
- [x] 5.5 Fixtures, tests and snapshots of the two scenarios of `Environment writes`

## 6. The docs

- [x] 6.1 `docs/architecture.md` says what a chain rule names for a read, a write and a read-write
