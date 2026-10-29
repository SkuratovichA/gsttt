# Proposal: agent benchmarks

## Why

gsttt is there to make an agent that changes code spend less and miss less: fewer tokens to find what a change affects, and fewer affected files left out. The README measures how fast gsttt builds a graph, and nothing measures what an agent spends and how right it is with gsttt, against the same agent alone, with a language server or with codebase-memory-mcp. So neither the claim nor a regression of it can be seen.

## What Changes

- `cargo xtask agents questions` draws questions about functions of the pinned TypeScript repositories and computes their answers with tools other than gsttt: the TypeScript language service for the files that refer to a function, and the repository's own tests, run with the function made to throw, for the test files that exercise it. The questions and their answers are checked in as `bench/agents/questions.json`.
- `cargo xtask agents run` asks every question of Claude Code in seven arms: four setups (alone, with a TypeScript language server, with codebase-memory-mcp and with the gsttt plugin), and the three that add tools also asked as told to use them. Each question is asked three times per arm, which is seven runs per question and repeat, with Claude Code shut off from the settings, memory, plugins and servers of the machine. Each answer is scored by precision and recall, with the tokens, cost, turns and time it took.
- The runs go to `bench/results/agents.json`, and `cargo xtask agents table` prints the table for the README.
- `bench/bench.toml` pins the agent runs: the model, the repeats, the seed, the limit of a run, how many questions of each kind each repository gets, the commands that install and test a repository, and typescript-language-server.

## Impact

- New capability `agent-benchmarks`.
- `xtask/src/agents.rs` and `xtask/src/agents/`, which reuse the pins, the checkout, the tool setup, the machine description and the results writer of `xtask/src/bench/`.
- `bench/agents/` for the questions and the Node scripts behind the answers, `bench/bench.toml` and `.gitignore`.
- The runs need a logged-in Claude Code and Node, and they are not part of CI.
