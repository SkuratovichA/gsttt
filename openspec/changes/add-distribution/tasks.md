# Tasks

## 1. Checks

- [ ] 1.1 `ci.yml` checks on Linux, actions pinned by commit, the OpenSpec CLI pinned through `pnpm dlx`
- [x] 1.2 `cargo xtask check`: action pins, plugin manifests and the paths they name

## 2. Releases

- [ ] 2.1 `cargo xtask version`, and the version written into the release build
- [x] 2.2 `cargo xtask package` and `cargo xtask notes`
- [ ] 2.3 Release jobs: four targets, packages, checksums, the GitHub release; no release from the archive workflow's pushes
- [ ] 2.4 `install.sh` with `gh` and `curl`, checksum verification and the overrides

## 3. Plugin

- [ ] 3.1 Marketplace, plugin manifest, MCP server, session-start hook, skill, and the wrapper for a missing binary

## 4. Proof

- [ ] 4.1 A test annotated `#[scenario("distribution", …)]` for every scenario of the `distribution` spec, the installer ones against a local directory
- [ ] 4.2 `cargo xtask check` passes on this repository, and `cargo nextest run -p xtask`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass
