# Design

## Context

`cargo xtask bench` (`xtask/src/bench.rs`) measures how fast tools build an import graph. It pins public repositories in `bench/bench.toml`, clones them at a commit (`bench/setup.rs`), installs the Node tools with pnpm into `bench/tools/node`, unpacks codebase-memory-mcp into `bench/tools/cbm`, describes the machine (`bench/machine.rs`) and writes a results file through a temporary file that refuses local paths (`bench/results.rs`). The agent benchmark reuses all of it and adds what speed does not need: questions with answers that no tool under test computed, Claude Code runs that see only the tools of their setup, and scores.

## The questions

- Two kinds, both about one exported function: `users`, the other files that refer to it, which an agent needs before it renames or changes a signature; and `tests`, the test files that break when it breaks, which an agent needs to know what to run.
- The answers come from tools that are not under test. `users` uses the TypeScript language service (`findReferences`) of the pinned TypeScript, run by a Node script in `bench/agents/`. `tests` makes the function throw and runs the repository's own tests with the Vitest JSON reporter. Static reachability says which tests could be affected; a failing test is the ground truth that one is.
- A test file is in the answer when it fails in both runs with the throw and passes in both runs without it, so a flaky file is in no answer. The files that fail without the throw are kept with the question and scored as neither right nor wrong: no agent can know them.
- Only TypeScript repositories get questions, because the language server is TypeScript's. Each repository names a scope: `packages/vite` for vite, whose other packages import vite by its package name through `dist`, which the language service cannot follow in a checkout that is not built; the whole checkout for excalidraw, whose tsconfig maps its packages to their sources. Vitest does not start in vite until `packages/vite/dist` exists, because its config imports vite through `dist`, so vite also pins a `build` command that runs once after the install; the output is git-ignored, and the draw and the runs share the one prepared checkout. Only vite gets questions of the kind `tests`: excalidraw installs with yarn, and its lockfile is the only thing that pins what its tests run against.
- The candidates of a kind are the exported functions of the scope, with `export function` and `export const name = (…) =>` or `= function`, declared in a file that is not a test and not a declaration file; `tests` also needs a block body. The Node script lists them with their referring files and the offset of their body once, and the draw in Rust takes them in the order of the SHA-256 of `seed`, repository, file and name until enough of them qualify. The script refuses to answer when a relative import or a path of the tsconfig inside the scope does not resolve, since a language service that misses a file gives answers that miss its references.
- A question holds its prompt, so the file shows exactly what the agent was asked. The prompt says what counts (by name: imports, named re-exports, uses; or the unit tests run by the pinned command), forbids changing files and running the tests, and asks for the last block marked `files`.

## The runs

- Every run is `claude -p` with the pinned model, `--output-format stream-json --verbose`, `--no-session-persistence`, `--permission-mode bypassPermissions`, `--setting-sources project`, `--tools` of its setup, the limit of turns, and the prompt on standard input. The environment drops `CLAUDECODE` and `CLAUDE_CODE_ENTRYPOINT`, so a run started from inside Claude Code is not taken for a nested one, and sets `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` and `ENABLE_CLAUDEAI_MCP_SERVERS=false`.
- `plain`, `lsp` and `cbm` pass `--strict-mcp-config` with a config of their own (empty, empty and codebase-memory-mcp). `gsttt` passes `--plugin-dir` with this repository's `plugin/` and no strict config, because a strict config also drops the servers of plugins. `lsp` passes `--plugin-dir` with a plugin the task writes, whose manifest has only `lspServers`, running the pinned typescript-language-server with the pinned TypeScript.
- The PATH is built from the directories of the programs a run needs, never from the PATH of the machine, because `~/.local/bin` holds both `claude` and an installed gsttt. The `gsttt` setup prepends a directory that holds only the binary under test.
- The checkout of the runs lives under the temporary directory of the system: under `~/git` or this repository a CLAUDE.md of a parent directory would reach every run. The task checks the parents for a CLAUDE.md before it prepares anything.
- The init event of a run lists its tools, its MCP servers with their status and its plugins. The task compares them with the setup and stops at the first difference, because one leaked server makes every number after it wrong.
- The answer, the tokens and the cost come from the stream: the final message, `modelUsage` summed over the models, `total_cost_usd`, `num_turns`. The duration is the task's own clock, so a run stopped at the limit has one too. The transcripts go to `bench/results/agents/`, which is ignored: they hold the paths of the machine.

## Results

- `bench/results/agents.json` is rewritten after every run, so a stopped task loses at most the run it was in, and a started-over task goes on where it stopped. It records the model id the init event names, not the alias, and refuses to add to results of another model, Claude Code version or questions file, so one table never mixes them.
- Tokens are counted as reported, cache reads included, because they are what the context of a run costs; the table shows the cost beside them.

## Rejected

- Questions written by hand: their answers would be what the writer believed, and a writer who knows gsttt would ask what gsttt answers well.
- Answers from gsttt, codebase-memory-mcp or a grep: each would score its own tool as perfect.
- Runs in parallel: they would compete for the machine and for the rate limit of the account, which shows up as time and as errors that belong to no setup.
