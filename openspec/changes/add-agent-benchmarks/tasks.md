# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `design.md`, `tasks.md` and the `agent-benchmarks` spec

## 2. The pins

- [x] 2.1 The `agents` table and the repository fields of `bench/bench.toml`, typescript-language-server among the Node tools, and `/bench/results/agents/` in `.gitignore`

## 3. The questions

- [x] 3.1 The Node script in `bench/agents/` that lists the exported functions of a scope with their referring files and their bodies, over the pinned TypeScript, and refuses imports that do not resolve
- [x] 3.2 The runs of the test command with and without the throw, and the checkout restored after each
- [x] 3.3 `cargo xtask agents questions`: the seeded draw and `bench/agents/questions.json`
- [x] 3.4 The questions of vite and excalidraw drawn and checked in

## 4. The runs

- [x] 4.1 The checkout of the runs: the check for a CLAUDE.md above it, the clone, the install and the two indexes
- [x] 4.2 The four setups: their commands, the plugin of the language server, the MCP config of codebase-memory-mcp and the PATH
- [x] 4.3 One run: the init check, the limits, the answer, the scores, the status of the checkout and the transcript
- [x] 4.4 The results file, resuming, and `cargo xtask agents table`
- [x] 4.5 One task at a time: a lock on a file in the temporary directory of the system, taken by `questions` and `run`, refused to a second task
- [x] 4.6 The daemon of codebase-memory-mcp stopped before its cache is thrown away or put back, and a missing copy of an index an error
- [x] 4.7 What a run can reach: the state of the client, the other sessions and the worktrees listed before every run, the index of gsttt, no network, bash as the shell, and a path that a rule would take for a pattern stopping the task
- [x] 4.8 An empty .claude directory that Claude Code makes is removed without marking the run
- [x] 4.9 `--setup <name>`, repeatable, limits `run` to the setups it names

## 5. Proof

- [x] 5.1 A test annotated `#[scenario("agent-benchmarks", …)]` for every scenario of the `agent-benchmarks` spec
- [x] 5.2 The smoke run: one question of each kind in the seven arms, once, exits 0 and leaves nothing to commit
- [x] 5.3 `cargo fmt --all --check`, `taplo fmt --check`, `cargo clippy`, `cargo nextest run -p xtask`, `cargo xtask spec`, `openspec validate add-agent-benchmarks --strict` and `typos` pass

## 6. The numbers

- [ ] 6.1 The full run with the release that follows names through barrels: `bench/results/agents.json`, the table in the README and the data of the site
