# Spec Delta

## Purpose

Distribution is how gsttt gets from a green commit to a working binary on someone's machine and a working MCP server in Claude Code: checks on every change, a release on every green push to `main`, an installer that verifies what it downloads, and a Claude Code plugin in the same repository.

## ADDED Requirements

### Requirement: Checks
The `ci` workflow SHALL run on every pull request and every push to `main`, on Linux with the toolchain of `rust-toolchain.toml`: `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo nextest run --locked --workspace`, `cargo xtask spec`, `cargo xtask check`, `openspec validate --all --strict` with a pinned OpenSpec CLI run through `pnpm dlx`, `cargo deny check` and `typos`.

#### Scenario: Every check runs in CI
- **WHEN** `.github/workflows/ci.yml` is parsed
- **THEN** it runs on pull requests and on pushes to `main`, on Linux, and runs each of the eight checks

### Requirement: Locked dependencies
Clippy, the tests and the release build SHALL run with `--locked`, so a `Cargo.lock` that is out of date fails the run instead of changing what is checked and shipped.

#### Scenario: Cargo.lock is used as it is
- **WHEN** `.github/workflows/ci.yml` is parsed
- **THEN** the clippy and test commands and the release build command pass `--locked`

### Requirement: Pinned actions
Every action in every workflow SHALL be pinned by a full commit id, and `cargo xtask check` SHALL fail naming the workflow and line of any action that is not.

#### Scenario: An unpinned action fails the check
- **WHEN** a workflow has `uses: actions/checkout@v4`
- **THEN** `cargo xtask check` exits non-zero and names the workflow file and line

### Requirement: Versions
The version of a release SHALL be `<major>.<minor>.<n>`, with `<major>` and `<minor>` from `[workspace.package] version` and `<n>` the number of commits in the history of the released commit, and `cargo xtask version` SHALL print it. A release build SHALL be given that version as the environment variable `GSTTT_VERSION` at compile time, for the `gsttt` binary to report.

#### Scenario: The version counts commits
- **WHEN** the workspace version is `0.1.0` and the released commit has 42 commits in its history
- **THEN** `cargo xtask version` prints `0.1.42`

#### Scenario: The release build is given its version
- **WHEN** `.github/workflows/ci.yml` is parsed
- **THEN** the step that builds with the `dist` profile has `GSTTT_VERSION` set to the output of `cargo xtask version`

### Requirement: Releases
When the checks pass on a push to `main`, a release SHALL be built for `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `x86_64-apple-darwin` and `aarch64-apple-darwin`. A push made by the archive workflow SHALL start no release. Pull requests SHALL NOT build for macOS.

#### Scenario: Releases follow green pushes to main
- **WHEN** `.github/workflows/ci.yml` is parsed
- **THEN** its release jobs need the checks, run only on pushes to `main` and build the four targets, and no job that runs on a pull request runs on macOS

### Requirement: One release per version
Releases SHALL be made one at a time. A version SHALL be released from one commit only: a run for a version whose tag points at another commit SHALL fail and upload nothing, and a run for the commit the tag points at SHALL change nothing. Only the highest version SHALL be marked as the latest release.

#### Scenario: A version is released from one commit only
- **WHEN** the release step runs for a version whose tag points at another commit
- **THEN** it fails naming that commit and runs no `gh` command

#### Scenario: Only the highest version is the latest release
- **WHEN** the release step runs for a version below an existing release
- **THEN** it creates the release with `--latest=false`

### Requirement: Release token
The token that can write releases SHALL be given to the commands that publish and to no step that compiles code.

#### Scenario: The release token reaches no compiler
- **WHEN** `.github/workflows/ci.yml` is parsed
- **THEN** no step that runs `cargo` is given a token, and the step that publishes is

### Requirement: Packages
`cargo xtask package --target <target>` SHALL pack the release build for that target as `gsttt-<version>-<target>.tar.gz` holding `gsttt`, `LICENSE` and `README.md`, beside a `.sha256` file.

#### Scenario: A package holds the binary and its checksum
- **WHEN** `cargo xtask package --target <host target>` runs after a release build
- **THEN** the archive holds `gsttt`, `LICENSE` and `README.md`, and the `.sha256` file matches the archive

### Requirement: Release notes
The GitHub release `v<version>` SHALL carry the packages and notes listing the subjects of the commits since the previous release, which `cargo xtask notes` prints.

#### Scenario: Notes list the commits since the last release
- **WHEN** a history has the tag `v0.1.3` and three commits after it
- **THEN** `cargo xtask notes` prints the subjects of those three commits, newest first, and nothing from before the tag

### Requirement: Installer
`install.sh` SHALL pick the package for the machine's operating system and architecture, download it from the latest release or the one `GSTTT_VERSION` names, through `gh` when it is installed and signed in and with `curl` otherwise, check its SHA-256, and install `gsttt` into `GSTTT_INSTALL_DIR` or else `~/.local/bin`. `GSTTT_BASE_URL` SHALL replace the release URL. A checksum that does not match SHALL install nothing and exit non-zero.

#### Scenario: A verified package is installed
- **WHEN** `install.sh` runs with `GSTTT_BASE_URL` pointing at a directory that holds a package and its correct checksum
- **THEN** `gsttt` is in `GSTTT_INSTALL_DIR` and runs

#### Scenario: A bad checksum installs nothing
- **WHEN** the checksum beside the package does not match it
- **THEN** `install.sh` exits non-zero and `GSTTT_INSTALL_DIR` is unchanged

### Requirement: Claude Code plugin
The repository SHALL be a Claude Code plugin marketplace: `.claude-plugin/marketplace.json` SHALL list the plugin `gsttt` from `plugin/`, whose manifest declares the MCP server `gsttt mcp`, a session-start hook and a skill that says when to use gsttt's tools. `cargo xtask check` SHALL verify that the plugin's manifests parse and that every path they name exists.

#### Scenario: A manifest naming a missing file fails the check
- **WHEN** the plugin manifest names `skills/gone/SKILL.md` and that file does not exist
- **THEN** `cargo xtask check` exits non-zero and names the manifest and the path

### Requirement: Finding the binary
The plugin's commands SHALL run through a wrapper that looks for `gsttt` on `PATH` and then in the directory `install.sh` installs into, `GSTTT_INSTALL_DIR` or `~/.local/bin`, so that Claude Code started without that directory on its `PATH` still finds it.

#### Scenario: The binary in the install directory is found
- **WHEN** `gsttt` is not on `PATH` and is in the install directory
- **THEN** the wrapper runs it with the arguments it was given

### Requirement: Missing binary
When the wrapper finds no `gsttt`, a session start SHALL print one line that names the install command and is written as a note for the user, every other hook event SHALL do nothing and exit zero, and the MCP server SHALL print one line on stderr and exit with status 1.

#### Scenario: A missing binary is one line
- **WHEN** the session-start hook runs with no `gsttt` on `PATH` or in the install directory
- **THEN** it prints one line that names the install command, as a note for the user, and exits zero

#### Scenario: A missing binary stops the server with one line
- **WHEN** the server starts with no `gsttt` on `PATH` or in the install directory
- **THEN** it prints one line naming the install command on stderr and exits with status 1

### Requirement: Safe hooks
A hook of the plugin SHALL NOT fail or hang a session. The wrapper SHALL end every hook with exit status zero and nothing on stderr, whatever `gsttt` does, and the manifest SHALL give every hook command a timeout of 5 seconds.

#### Scenario: A failing hook is silent
- **WHEN** the session-start hook runs with a `gsttt` that exits with status 2 and complains on stderr
- **THEN** the hook exits zero and prints nothing

#### Scenario: A hook is cut off after five seconds
- **WHEN** the plugin manifest is parsed
- **THEN** its session-start hook has a timeout of 5 seconds

### Requirement: Supply chain
`deny.toml` SHALL allow only licenses that can be distributed with an Apache-2.0 binary, deny crates with security advisories, and allow crates from crates.io only.

#### Scenario: A copyleft dependency fails
- **WHEN** a dependency is licensed `GPL-3.0-only`
- **THEN** `cargo deny check licenses` fails and names the crate
