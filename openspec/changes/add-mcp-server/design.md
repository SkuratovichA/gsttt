# Design

## Context

A module crate never depends on another module crate, and `crates/mcp` cannot depend on the binary. The binary has what the server needs: the root, the evaluated config, the registry and trust. The server therefore gets its index through a trait, and the binary implements that trait once the CLI change has landed. Until then the server is built and tested against a fake source.

## Decisions

### The source

```rust
pub trait IndexSource: Send + Sync + 'static {
    /// Refreshes the index and returns it; blocking, called on a blocking thread.
    fn refreshed(&self) -> Result<Loaded, SourceError>;
    /// The stored index without refreshing, or `None` before the first index.
    fn stored(&self) -> Result<Option<Loaded>, SourceError>;
    /// A path argument read from the root, and whether a file is there.
    fn path(&self, argument: &str) -> Result<Target, SourceError>;
    /// The files that differ from HEAD, or from the best common ancestor with `base`.
    fn changed(&self, base: Option<&str>) -> Result<Vec<Change>, SourceError>;
}

/// Why the source could not answer, as a sentence the model can act on.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct SourceError { pub message: String }

pub async fn serve(source: impl IndexSource) -> Result<(), ServeError>;
```

- `Loaded { index: Arc<Index>, freshness: Freshness }` and `Target { path: RepoPath, on_disk: bool }` are `gsttt_core::Loaded` and `gsttt_core::Target`, the types the CLI's `Repository` returns, so the binary's source passes them on as they are.
- The binary's implementation opens a `Repository` on every call. A server lives as long as a session, while `.gsttt/config.toml`, the trust store and the branch change under it, and opening is cheap next to a refresh. A root that the CLI refuses, `$HOME` for example, fails the call, not the server.
- `Loaded` and `Target` live in `gsttt-core` (`crates/core/src/index.rs` and `crates/core/src/path.rs`), the one crate that the binary and the server both depend on, `Loaded` beside the `Index` and the `Freshness` it holds and `Target` beside the `RepoPath` it holds. So the command line and the server read an index and a path argument with one code: `Repository` returns the index as a `Loaded` (`refreshed` and `stored`) and a path argument as a `Target` (`argument`), the commands of the command line take them as they are, and so does the source of the server. The only difference is the directory `argument` reads a path against, which its caller passes in: the directory the command ran in, or the root.
- The server reads the trust store and never writes it, and it never asks for trust. The `Repository` it opens on every call joins to the registry only the `[[external]]` connectors that the person has trusted in that root, and trusting needs a terminal, which a server on the standard streams has not. So an external that is not trusted does not run while the server answers: the refresh goes on without it, the files only it claims stay among those no connector read, and the person runs `gsttt config trust` at a terminal, after which the next call, which opens the repository again, starts it. `an_untrusted_external_is_not_started_by_the_server` looks at the marker of a fake external and at the trust store after the first answer and again after the server has ended, and `trust_given_at_a_terminal_reaches_a_running_server` is its control.
- A tool error ends in a step wherever the command line prints a help line for the failure: a `-C` that names no directory, a root gsttt refuses (the home directory and `/`), a path outside the repository, `changed` outside git, and a `base` that is no revision or shares no history with HEAD. The text is the sentence the command line prints, `; ` and the step, as the other errors of the server are worded. The step is the help line worded again for a model, which cannot follow the line as it stands: a model passes arguments (`paths`, `changed`, `base`) and cannot change the root the server was started in, so where a person would run the command again the step asks the person, and where the line names the current directory the step names the root. A config that does not read has a step too, to fix it and ask again, which the command line leaves to the person at the terminal. A failure with no help line keeps the sentence of the command line. `the_server_has_a_step_where_the_command_line_has_a_help_line` keeps the two in step: for each failure of the repository that the config did not cause, the `CliError` that `CliError::from` makes of it has a help line exactly when the message of the server differs from the sentence of the command line, so a help line the command line gains later fails the test until `next_step` has a step for it, and `next_step` has no wildcard arm, so a new failure has to be sorted there.

### Handler

- `ServerHandler` is implemented by hand: `get_info`, `list_tools` and `call_tool`, without the `#[tool]` router.
  - The router answers an argument that does not deserialize with a JSON-RPC error.
  - The MCP specification of 2025-11-25 says input errors are tool execution errors, so that the model can correct the call.
- Each tool's arguments are a struct with `Deserialize` and `JsonSchema`. Its input schema is `schemars::schema_for!` of that struct.
- `call_tool` deserializes the arguments itself. A failure becomes a result with `isError: true` and text such as `impact: \`paths\` must be a list of paths, for example ["src/a.ts"]`.
- An unknown tool name stays a protocol error (`-32602`), as the specification has it.
- `on_custom_request` answers a `tools/call` whose `arguments` is not an object with the JSON-RPC error for invalid parameters, and the server keeps serving. rmcp reads the parameters of a call before it asks the handler, so such a call never reaches `call_tool`, where every other bad argument becomes a tool error; rmcp hands it to `on_custom_request` as a request the server does not know, and its own answer to that is `method not found` with the method's name, which would tell the client that the server has no tools at all. The error for invalid parameters says what a `tools/call` takes instead, and any other method that arrives there still gets `method not found`. That is the one protocol error left for a call to a known tool, which is why the requirement `Errors` promises none only when `arguments` is an object or is left out.

### Answers

- A tool returns one text block holding `answer.text(entries)`. That is the text the CLI prints, `--all` included, so the agent and the person read the same thing.
- There is no `structuredContent`, because it would double what every answer costs in tokens.

### Refresh

- A `tokio::sync::Mutex<()>` in the server serialises calls into the source.
- The call runs in `tokio::task::spawn_blocking`, so a long first index does not stall the transport.
- A second call that arrives during a refresh waits, then refreshes again. That refresh finds nothing to do.
- A cancelled request does not stop the refresh it started. The refresh finishes, and the next call reuses it.

### Instructions

`initialize` returns this text, which the specification test pins:

```text
gsttt answers what a change affects, from an index of imports and contracts (HTTP routes, GraphQL operations, gRPC methods, tables, queues, env variables) across the repository. Call impact before changing a file that others may use, or with changed=true before finishing a task; dependents before renaming or deleting; path to learn how two files are connected; contract to find who serves and who calls an endpoint. Every answer ends with what it covered and how fresh it is; when coverage names a gap, say so instead of assuming nothing is affected.
```

### Lifecycle

- stdout carries only the protocol. Logs go to stderr through `tracing`.
- `serve` returns when stdin closes, and `gsttt mcp` then exits 0.

## Risks

- **A first index of a large repository can outlast a client's tool timeout.** The answer arrives late or not at all, but the snapshot is written either way, and the next call is fast. Progress notifications are left for later.
