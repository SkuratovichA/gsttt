# Proposal

## Why

An agent that edits code asks the questions gsttt answers more often than a person does, and it asks them through MCP. The Claude Code plugin already declares `gsttt mcp` as its server and starts it in every session, so the server has to start instantly in any directory, answer with the same text the CLI prints, and never stop a session over a bad call.

## What Changes

- `crates/mcp`: an MCP server on stdio with seven tools: `impact`, `dependents`, `dependencies`, `path`, `contract`, `coverage` and `index_status`.
- An `IndexSource` trait through which the server gets an index without depending on the binary. Its tests use a fake source.
- `initialize` and `tools/list` do not touch the repository, and `initialize` carries instructions on when to call which tool.
- Every tool except `index_status` refreshes first. Refreshes run one at a time, on a blocking thread.
- Answers are the CLI's text. A bad argument or a failed refresh is a tool error in plain English, which the model can act on, and never a protocol error.
- In the binary: `gsttt mcp`, with `IndexSource` implemented over the CLI's `Repository`.

## Capabilities

### New Capabilities

- `mcp`: the MCP server, its tools, their answers and errors, and its lifecycle.

### Modified Capabilities

None.

## Impact

- `crates/mcp/`.
- In `crates/gsttt/`, after the CLI change: the `mcp` command and the `IndexSource` implementation.
- No new dependency.
