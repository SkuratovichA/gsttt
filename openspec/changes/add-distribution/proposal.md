# Proposal

## Why

A tool that answers "what does this change touch" has to be one command away, in the terminal and inside Claude Code, and every fix has to reach users without a release ritual. Releasing every green push to `main`, with versions counted from history, makes that automatic; checking pins, licenses and checksums keeps it safe to automate.

## What Changes

- `.github/workflows/ci.yml`: the checks on pull requests and pushes, and the release jobs on green pushes to `main`.
- `xtask/src/dist.rs` with `xtask/src/dist/`: `cargo xtask version`, `package`, `notes` and `check`.
- `install.sh`, verifying checksums, working with private releases through `gh`.
- The Claude Code plugin: `.claude-plugin/marketplace.json` and `plugin/` with the MCP server, a session-start hook, a skill and the wrapper that copes with a missing binary.
- `deny.toml` and `_typos.toml` reviewed against the spec.

## Capabilities

### New Capabilities

- `distribution`: checks, versions, releases, the installer and the Claude Code plugin.

### Modified Capabilities

None.

## Impact

`.github/workflows/ci.yml`, `xtask/src/dist.rs`, `xtask/src/dist/`, `xtask/tests/dist*`, `install.sh`, `.claude-plugin/`, `plugin/`, `deny.toml`, `_typos.toml`. macOS builds run only for releases, because macOS minutes cost ten times Linux minutes on a private repository.
