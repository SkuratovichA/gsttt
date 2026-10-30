# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the specs

## 2. The oracle

- [x] 2.1 `bench/agents/service.mjs`: the language service of `functions.mjs`, moved so that both scripts share it
- [x] 2.2 `bench/agents/clients.mjs`: the methods that a resolver calls, with the files whose GraphQL documents select the field the resolver serves

## 3. The pins

- [x] 3.1 `graphql` among the Node tools, `users` optional, `clients` in the `agents` table of a repository, and `speed = false` for a repository that only the agents ask about
- [x] 3.2 hoppscotch pinned at a release tag and its commit, with the reason it was chosen

## 4. The questions

- [x] 4.1 The kind `clients`: the prompt, the draw of methods with 1 to 30 clients, and the place in the questions file after the other kinds
- [x] 4.2 The questions of hoppscotch drawn and appended to `bench/agents/questions.json`, the earlier questions unchanged
- [x] 4.3 `cargo xtask agents tools` and its CI step, so that the tests that run the scripts cannot be skipped in CI

## 5. Proof

- [x] 5.1 A test annotated `#[scenario(…)]` for every scenario of this change, among them the oracle on a small tree

## 6. The run

- [ ] 6.1 The questions of the kind `clients` in the full run, with 6.1 of add-agent-benchmarks
