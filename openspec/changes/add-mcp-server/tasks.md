## 1. Server

- [x] 1.1 Define `IndexSource`, `Loaded`, `Target` and `SourceError`, and a fake source for tests that counts calls and can fail
- [x] 1.2 Implement `ServerHandler` by hand: `get_info` with the instructions, `list_tools` with a schema per argument struct, and `call_tool` that deserializes arguments into tool errors
- [x] 1.3 Answer each tool with the answer's text, and `index_status` from the stored index
- [ ] 1.4 Serialise calls into the source behind a mutex on a blocking thread, and return from `serve` when stdin closes

## 2. Binary

- [ ] 2.1 After the CLI change has landed, add `gsttt mcp` and implement `IndexSource` over `Repository`, opened on every call

## 3. Gates

- [ ] 3.1 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace`, `cargo xtask spec` and `openspec validate add-mcp-server --strict --no-interactive` all pass
