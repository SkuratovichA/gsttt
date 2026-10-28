# Tasks

## 1. Checks

- [x] 1.1 `ci.yml` checks on Linux, actions pinned by commit, the OpenSpec CLI pinned through `pnpm dlx`
- [x] 1.2 `cargo xtask check`: action pins, plugin manifests and the paths they name

## 2. Releases

- [x] 2.1 `cargo xtask version`, and the version passed to the release build as `GSTTT_VERSION`
- [x] 2.2 `cargo xtask package` and `cargo xtask notes`
- [x] 2.3 Release jobs: four targets, packages, checksums, the GitHub release; no release from the archive workflow's pushes
- [x] 2.4 `install.sh` with `gh` and `curl`, checksum verification and the overrides

## 3. Plugin

- [x] 3.1 Marketplace, plugin manifest, MCP server, session-start hook, skill, and the wrapper for a missing binary

## 4. Proof

- [x] 4.1 A test annotated `#[scenario("distribution", …)]` for every scenario of the `distribution` spec, the installer ones against a local directory
- [x] 4.2 `cargo xtask check` passes on this repository, and `cargo nextest run -p xtask`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass

## 5. Review fixes

- [x] 5.1 Tasks find the repository with `git rev-parse --show-toplevel` from any directory of it, and `flate2`, `sha2` and `tar` are workspace dependencies
- [x] 5.2 A version is released from one commit only, only the highest version is the latest release, and releases run one at a time
- [x] 5.3 The release token is given to the commands that publish and to no step that compiles code
- [x] 5.4 Clippy, the tests and the release build run with `--locked`
- [x] 5.5 The spec promises only what this change builds: the release build is given the version as `GSTTT_VERSION`, and `gsttt --version` is left to the CLI
