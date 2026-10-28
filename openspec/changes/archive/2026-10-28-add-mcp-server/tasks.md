## 1. Server

- [x] 1.1 Define `IndexSource`, `Loaded`, `Target` and `SourceError`, and a fake source for tests that counts calls and can fail
- [x] 1.2 Implement `ServerHandler` by hand: `get_info` with the instructions, `list_tools` with a schema per argument struct, and `call_tool` that deserializes arguments into tool errors
- [x] 1.3 Answer each tool with the answer's text, and `index_status` from the stored index
- [x] 1.4 Serialise calls into the source behind a mutex on a blocking thread, and return from `serve` when stdin closes
- [x] 1.5 Move `Loaded` and `Target` into `gsttt-core`, so that the server and the binary share the types of the source
- [x] 1.6 Let the `path` tool take `all`, as the other tools that cut a list do
- [x] 1.7 Answer a question about a file the index does not hold as a tool error with the text the CLI prints, through `Answer::held` of the core
- [x] 1.8 Answer a `tools/call` whose `arguments` is not an object with the JSON-RPC error for invalid parameters, and keep serving
- [x] 1.9 Map the `all` flag to entries with `Entries::from_flag` of the core, and drop the server's own copy
- [x] 1.10 Share the test tree builder of the core queries with the server tests as `gsttt_core::testing`, behind a `testing` feature, and give it `skipped`

## 2. Binary

- [x] 2.1 After the CLI change has landed, add `gsttt mcp` and implement `IndexSource` over `Repository`, opened on every call
- [x] 2.2 Refuse `--json`, `--all` and `--no-refresh` in `gsttt mcp` with exit code 2, naming the flag
- [x] 2.3 Test that `gsttt mcp` ends when its input closes while a refresh is running
- [x] 2.4 Test `gsttt mcp` as a process over its standard streams, one test for each scenario of the binary and of the lifecycle
- [x] 2.5 Merge the CLI change at `3fddb7d` into this change, so that the source of the server opens the `Repository` as that merge leaves it
- [x] 2.6 Tell the model to fix a config that does not read and ask again, and keep an `[[external]]` connector that no one trusted from starting when the server refreshes
- [x] 2.7 List `mcp` among the commands of the `cli` capability, in a delta of the requirement `Commands`
- [x] 2.8 End the text of a tool error in a step for a model wherever the command line prints a help line for the failure, and keep the two in step with a test
- [x] 2.9 Put the step after a config that does not read after `; ` like the others, and keep one copy of its sentence for the code and its tests
- [x] 2.10 Test the whole message of each failure of the repository that the config did not cause, in the unit tests of the source
- [x] 2.11 Check the marker of the untrusted external and the trust store again after the server has ended

## 3. Gates

- [x] 3.1 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace`, `cargo xtask spec` and `openspec validate add-mcp-server --strict --no-interactive` all pass
