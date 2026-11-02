# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The stack

- [x] 2.1 `crates/core/src/engine/pipeline.rs`: `contain_reading` runs `parse` and `parse_embedded` on a stack sized by the text
- [x] 2.2 `Cargo.toml` and `crates/core/Cargo.toml`: `stacker`

## 3. The cost of depth

- [x] 3.1 Rust: `crates/lang-rust/src/depth.rs` counts the levels of a file as its tree is built, and `project.rs` fails a file past the limit
- [x] 3.2 TypeScript: a chain is matched against roots of at most 64 segments, and the connector goes one up
- [x] 3.3 TypeScript: a sum of text pieces moves the pieces into the first

## 4. The proof

- [x] 4.1 A binary test with a deep file each for TypeScript, Python and Rust, closed and not closed, run twice
- [x] 4.2 A test in `crates/core` with a connector whose parse recurses once per byte
- [x] 4.3 A deep sum in a URL is linked whole
- [x] 4.4 Tests of the Rust depth and of the long chain

## 5. The docs

- [x] 5.1 `docs/architecture.md`: the stack of a parse under "Failure containment"
