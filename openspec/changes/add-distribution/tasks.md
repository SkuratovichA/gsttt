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

- [x] 5.1 Tasks find the repository with `git rev-parse --show-toplevel` from any directory of it, and the three crates that write a package in-process are workspace dependencies
- [x] 5.2 A version is released from one commit only, only the highest version is the latest release, and releases run one at a time
- [x] 5.3 The release token is given to the commands that publish and to no step that compiles code
- [x] 5.4 Clippy, the tests and the release build run with `--locked`
- [x] 5.5 The spec promises only what this change builds: the release build is given the version as `GSTTT_VERSION`, and `gsttt --version` is left to the CLI
- [x] 5.6 The wrapper ends every hook with status 0 and nothing on stderr, the session-start hook is cut off after 5 seconds, and a server without `gsttt` stops with one line on stderr and status 1
- [x] 5.7 The wrapper also looks for `gsttt` in the install directory
- [x] 5.8 The session-start line is a note for the user, not an instruction to the model
- [x] 5.9 The plugin README says that the install line needs a public repository, gives the `gh api` line for a private one, says that adding the marketplace needs git access, and says what the checksum guards against
- [x] 5.10 The form of `skills` and of the inline hooks was settled with `claude plugin validate` (2.1.312), which accepts both as they are
- [x] 5.11 `cargo xtask check` fails for a program that a plugin command runs without an executable bit, and for a manifest path that does not start with `./`
- [x] 5.12 `cargo xtask check` also reads composite actions, accepts a Docker image pinned by digest, finds a `uses` in a flow mapping or behind a quoted key, ignores the text of a `run` script, and fails for a repository without workflows or a workflow that is not YAML
- [x] 5.13 `install.sh` looks for a SHA-256 tool before it downloads, fetches GitHub over HTTPS with a bounded wait and retries, refuses a version that is not numbers joined by single dots, picks the Apple Silicon package under Rosetta, stages the binary in a `mktemp` file that the exit cleanup removes, and says how to put the install directory on `PATH`; a test pins its default directory against the wrapper's
- [x] 5.14 A test pins the fields that make a package the same on every machine (the times, owners and format of the tar headers, and the time and system byte of the gzip header), the code says what else the bytes depend on, and AGENTS.md says that the tests need `cargo-deny`
- [x] 5.15 `docs/architecture.md` says that the manifest declares the server and the hook inline and that the wrapper runs both, and where the wrapper looks for the binary
- [x] 5.16 `_typos.toml` lets the two deliberate test strings (a misspelled option name and a Latin-1 byte string) pass only where they stand, by patterns that name the text around them, and the same words anywhere else are still reported

## 6. Integration fixes

- [x] 6.1 `typos` exits 0 on the whole stack: `_typos.toml` accepts the engine's `excluder` and the config's `unparseable` as the words they are, and lets the test data of the lower layers and the name of a field of gix pass only where they stand
