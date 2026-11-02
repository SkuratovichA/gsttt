# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The core

- [x] 2.1 `crates/core/src/rules.rs`: `ChainRule::callback_roots`, with its test, and the pack schema snapshot

## 3. The pack

- [x] 3.1 `crates/lang-ts/packs/prisma.toml`: the `$transaction` patterns

## 4. The TypeScript connector

- [x] 4.1 `crates/lang-ts/src/analyze/roots.rs`: the first parameter of a callback given to a listed call is declared with the pattern of the call
- [x] 4.2 `crates/lang-ts/src/analyze.rs`: the chain read accepts such a name
- [x] 4.3 `crates/lang-ts/src/provides.rs` and `README.md`: the limits name the callback parameter

## 5. The tests

- [x] 5.1 The fixture tree `prisma-callback-roots` and a test for each scenario
