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

pub struct Loaded { pub index: Arc<Index>, pub freshness: Freshness }
pub struct Target { pub path: RepoPath, pub on_disk: bool }

/// Why the source could not answer, as a sentence the model can act on.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct SourceError { pub message: String }

pub async fn serve(source: impl IndexSource) -> Result<(), ServeError>;
```

- The binary's implementation opens a `Repository` on every call. A server lives as long as a session, while `init.lua`, the trust store and the branch change under it, and opening is cheap next to a refresh. A root that the CLI refuses, `$HOME` for example, fails the call, not the server.

### Handler

- `ServerHandler` is implemented by hand: `get_info`, `list_tools` and `call_tool`, without the `#[tool]` router.
  - The router answers an argument that does not deserialize with a JSON-RPC error.
  - The MCP specification of 2025-11-25 says input errors are tool execution errors, so that the model can correct the call.
- Each tool's arguments are a struct with `Deserialize` and `JsonSchema`. Its input schema is `schemars::schema_for!` of that struct.
- `call_tool` deserializes the arguments itself. A failure becomes a result with `isError: true` and text such as `impact: \`paths\` must be a list of paths, for example ["src/a.ts"]`.
- An unknown tool name stays a protocol error (`-32602`), as the specification has it.

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
