# Proposal: agent questions about the clients of a method

## Why

The agent benchmarks ask about functions and the tests that break with them, which are questions about files and imports. gsttt's other half is the contracts between services: a GraphQL operation that one package sends and another serves. Nothing asks an agent that, so the benchmarks say nothing about the part of gsttt that no language server or code index covers.

## What Changes

- A third kind of question, `clients`: which files of a scope hold a GraphQL operation that selects a field whose resolver calls a given method directly. The answer is computed without gsttt, by the TypeScript language service for the call from a resolver and by the `graphql` package for the documents, in `bench/agents/clients.mjs`.
- The language service that `functions.mjs` builds moves to `bench/agents/service.mjs`, so that the two scripts cannot read a checkout differently. The output of `functions.mjs` does not change.
- `bench/bench.toml` pins `graphql` among the Node tools and hoppscotch (MIT, a NestJS server with code-first GraphQL resolvers and client packages with GraphQL documents) as a repository that asks only questions of the kind `clients`. `users` becomes optional beside `tests` and `clients`.
- A repository can say `speed = false`: the speed benchmark does not measure it. hoppscotch does, so that this change adds no repository to what the speed benchmark measures.
- `cargo xtask agents tools` installs the Node tools of the pins and nothing else. CI runs it before the tests: the tests that run the scripts end early where the tools are missing, and a scenario they prove must not pass without having run, so in CI they fail instead.
- `bench/agents/questions.json` gets the new questions after the existing ones, which stay as they were. Scoring, results and the tables treat the kind like `users`.

## Impact

- The `agent-benchmarks` capability, in the unarchived change `add-agent-benchmarks`, gets the requirements of the kind `clients`, and the `benchmarks` capability the one about `speed = false`.
- `xtask/src/agents.rs`, `xtask/src/agents/`, `xtask/src/bench.rs`, `xtask/src/bench/pins.rs` and `xtask/src/bench/pins/agents.rs`, `bench/agents/`, `bench/bench.toml`.
- The runs of the new questions are not made by this change: `cargo xtask agents run` needs a logged-in Claude Code and is not part of CI.
