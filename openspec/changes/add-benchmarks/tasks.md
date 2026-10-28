# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the `benchmarks` spec

## 2. The pins

- [x] 2.1 `bench/bench.toml`: four repositories at a release tag and commit, the tools at a version, the archives of codebase-memory-mcp with their SHA-256, and `/bench/tools` in `.gitignore`

## 3. The task

- [x] 3.1 `cargo xtask bench` reads its arguments and the pins, and refuses pins that are not a commit or a version
- [x] 3.2 Setup: shallow clones at the pinned commit, the Node tools with pnpm, grimp with uv, and codebase-memory-mcp from its archive after the SHA-256 is checked
- [x] 3.3 The plan of runs: per tool the command, the step that makes a cold run cold, and the warm-up
- [x] 3.4 hyperfine runs, the description of the machine, the results file and the printed table

## 4. Proof

- [x] 4.1 A test annotated `#[scenario("benchmarks", …)]` for every scenario of the `benchmarks` spec
- [x] 4.2 The smoke run: `cargo xtask bench --runs 1` over the four repositories exits 0 and leaves nothing to commit
- [x] 4.3 `cargo fmt --all --check`, `taplo fmt --check`, `cargo clippy`, `cargo nextest run -p xtask`, `cargo xtask spec`, `openspec validate add-benchmarks --strict` and `typos` pass
