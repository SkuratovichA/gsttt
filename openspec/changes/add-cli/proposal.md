# Proposal

## Why

The index answers nothing until something asks it. People ask from a terminal and scripts ask from CI, and both need the same things: the repository found from wherever they stand, a fresh index without a separate step, an answer they can read or parse, and an exit code they can branch on. The configuration needs a way in as well: a starter `.gsttt/config.toml`, a check that says what is wrong and on which line, and a gate in front of external connectors. Those are programs that a repository's `.gsttt/config.toml` names and gsttt starts, so without the gate cloning a repository and asking a question about it would run a stranger's program.

## What Changes

- The `gsttt` binary: the commands of the architecture's Surfaces section except `mcp`, which the MCP server change adds, plus `config trust`, `config untrust` and `schema external`.
- Finding the root, with `-C`, and refusing the home directory and `/`. Path arguments are relative to the current directory and turned into paths from the root by their text.
- A refresh before every question. `--no-refresh` answers from the stored index, and `status` never refreshes.
- Text answers for people, `--json` for scripts and `--all` for uncut lists. Exit codes are 0 for an answer, 1 for a failure and 2 for a usage error. stdout carries answers only.
- The registry: the built-in connectors in a fixed order with their packs, then the trusted externals.
- A trust store for external connectors. An external starts only after a person has trusted its exact command for this repository at a terminal.
- `gsttt hook session-start`: one line from the snapshot header, and nothing when there is no snapshot.
- `gsttt init`, `gsttt config check` and `gsttt config show`.
- A quiet panic hook that leaves the panics the engine contains to coverage.
- A smoke run of the packaged binary in the CI build, and a test that indexes this repository.

## Capabilities

### New Capabilities

- `cli`: the command line, its output and exit codes, the registry, trust for external connectors, the session-start line and the configuration commands.

### Modified Capabilities

None.

## Impact

- `crates/gsttt/`.
- Core gets three small additions:
  - in `crates/core/src/path.rs`, the path-argument helper, so that the CLI and the MCP server read paths by one rule;
  - in `crates/core/src/connector.rs`, `DynConnector::check_options`;
  - in `crates/core/src/query/render.rs`, `thousands` becomes public.
- `.github/workflows/ci.yml`: the smoke step.
- `xtask/tests/dist/ci.rs`: the test that pins the smoke step.
- No new dependency beyond `sha2`, for the trust key.
