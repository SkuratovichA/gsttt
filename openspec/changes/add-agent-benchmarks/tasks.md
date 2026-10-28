# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `design.md`, `tasks.md` and the `agent-benchmarks` spec

## 2. The pins

- [x] 2.1 The `agents` table and the repository fields of `bench/bench.toml`, typescript-language-server among the Node tools, and `/bench/results/agents/` in `.gitignore`

## 3. The questions

- [x] 3.1 The Node script in `bench/agents/` that lists the exported functions of a scope with their referring files and their bodies, over the pinned TypeScript, and refuses imports that do not resolve
- [x] 3.2 The runs of the test command with and without the throw, and the checkout restored after each
- [x] 3.3 `cargo xtask agents questions`: the seeded draw and `bench/agents/questions.json`
- [ ] 3.4 The questions of vite and excalidraw drawn and checked in

## 4. The runs

- [ ] 4.1 The checkout of the runs: the check for a CLAUDE.md above it, the clone, the install and the two indexes
- [ ] 4.2 The four setups: their commands, the plugin of the language server, the MCP config of codebase-memory-mcp and the PATH
- [ ] 4.3 One run: the init check, the limits, the answer, the scores, the status of the checkout and the transcript
- [ ] 4.4 The results file, resuming, and `cargo xtask agents table`

## 5. Proof

- [ ] 5.1 A test annotated `#[scenario("agent-benchmarks", …)]` for every scenario of the `agent-benchmarks` spec
- [ ] 5.2 The smoke run: one question of each kind in the four setups, once, exits 0 and leaves nothing to commit
- [ ] 5.3 `cargo fmt --all --check`, `taplo fmt --check`, `cargo clippy`, `cargo nextest run -p xtask`, `cargo xtask spec`, `openspec validate add-agent-benchmarks --strict` and `typos` pass

## 6. The numbers

- [ ] 6.1 The full run with the release that follows names through barrels: `bench/results/agents.json`, the table in the README and the data of the site
