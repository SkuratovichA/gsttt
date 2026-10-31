# distribution Specification

## Purpose
Distribution is how gsttt gets from a green commit to a working binary on someone's machine and a working MCP server in Claude Code: checks on every change, a release on every green push to `main`, an installer that verifies what it downloads, and a Claude Code plugin in the same repository.

## Requirements

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
Every action in every workflow and in every composite action under `.github/actions` SHALL be pinned by a full commit id, and every Docker image by a `sha256` digest. `cargo xtask check` SHALL fail naming the file and line of any that is not, and when `.github/workflows` is missing.

#### Scenario: An unpinned action fails the check
- **WHEN** a workflow has `uses: actions/checkout@v4`
- **THEN** `cargo xtask check` exits non-zero and names the workflow file and line

#### Scenario: An unpinned action in a composite action fails the check
- **WHEN** `.github/actions/setup/action.yml` has `uses: actions/cache@v4` in a step
- **THEN** `cargo xtask check` exits non-zero and names that file and line

#### Scenario: A Docker image pinned by its digest passes the check
- **WHEN** a workflow has `uses: docker://alpine@sha256:` followed by 64 hexadecimal digits
- **THEN** `cargo xtask check` reports no problem

#### Scenario: A uses line in a script is not an action
- **WHEN** a `run: |` script of a workflow has a line `uses: actions/checkout@v4`
- **THEN** `cargo xtask check` reports no problem

#### Scenario: A repository without workflows fails the check
- **WHEN** the repository has no `.github/workflows` directory
- **THEN** `cargo xtask check` exits non-zero and names the directory

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

### Requirement: Installer downloads
`install.sh` SHALL fetch from GitHub with `curl` over HTTPS only, with TLS 1.2 or newer, a limit on the wait and retries of transient failures, and from `GSTTT_BASE_URL` without those. It SHALL stop before any download when neither `sha256sum` nor `shasum` exists, and SHALL accept as `GSTTT_VERSION` only numbers joined by single dots.

#### Scenario: A GitHub download is over HTTPS only
- **WHEN** `install.sh` downloads without a base URL
- **THEN** every `curl` call has `--proto =https`, `--tlsv1.2`, `--connect-timeout`, `--max-time` and `--retry`

#### Scenario: A base URL is fetched as given
- **WHEN** `GSTTT_BASE_URL` is a `file://` URL
- **THEN** the package is installed and no `curl` call has those options

#### Scenario: A missing checksum tool stops the install before any download
- **WHEN** neither `sha256sum` nor `shasum` is on `PATH`
- **THEN** `install.sh` exits non-zero naming both, before `curl` is called, and installs nothing

#### Scenario: A malformed version is refused
- **WHEN** `GSTTT_VERSION` is `1..2`
- **THEN** `install.sh` exits non-zero saying it cannot tell which version to install, and downloads nothing

### Requirement: Installer on Apple Silicon
On macOS, `install.sh` SHALL install the Apple Silicon package on an Apple Silicon machine even when `uname -m` says `x86_64` because the shell runs under Rosetta.

#### Scenario: A shell under Rosetta gets the Apple Silicon package
- **WHEN** `uname -s` is `Darwin`, `uname -m` is `x86_64` and `sysctl -n sysctl.proc_translated` prints `1`
- **THEN** the `aarch64-apple-darwin` package is installed

### Requirement: Installer leaves nothing behind
`install.sh` SHALL stage the binary in a uniquely named file in the install directory before renaming it into place, and SHALL remove the staged file and its download directory however it stops.

#### Scenario: An interrupted install leaves nothing behind
- **WHEN** `install.sh` is sent `TERM` after the binary is copied and before it is renamed
- **THEN** it exits non-zero, and neither the install directory nor `TMPDIR` holds anything the install made

### Requirement: Installer PATH hint
`install.sh` SHALL print the line that adds the install directory to `PATH` when it is not there, and no hint when it is, whether or not the directory or the `PATH` entry ends in a slash.

#### Scenario: A directory on PATH gets no hint
- **WHEN** the install directory is listed in `PATH`, with or without a trailing slash
- **THEN** `install.sh` prints no hint about `PATH`

#### Scenario: A directory off PATH gets the line to add
- **WHEN** the install directory is not in `PATH`
- **THEN** `install.sh` prints `export PATH="<directory>:$PATH"`

### Requirement: Claude Code plugin
The repository SHALL be a Claude Code plugin marketplace: `.claude-plugin/marketplace.json` SHALL list the plugin `gsttt` from `plugin/`, whose manifest declares the MCP server `gsttt mcp`, a session-start hook and a skill that says when to use gsttt's tools.

#### Scenario: The marketplace lists the plugin
- **WHEN** `.claude-plugin/marketplace.json` and the manifest of the plugin are parsed
- **THEN** the marketplace lists the plugin `gsttt` from `plugin/`, and the manifest declares the MCP server, the session-start hook and the skill

### Requirement: Plugin check
`cargo xtask check` SHALL verify that the plugin's manifests parse, that every path a manifest names starts with `./` and exists, and that every program a command runs from the plugin directory is executable, because git keeps the mode and a script that lost it breaks the hook and the server without a word.

#### Scenario: A manifest naming a missing file fails the check
- **WHEN** the plugin manifest names `skills/gone/SKILL.md` and that file does not exist
- **THEN** `cargo xtask check` exits non-zero and names the manifest and the path

#### Scenario: A manifest path must start with ./
- **WHEN** the plugin manifest names the skill `skills/p`, without `./`
- **THEN** `cargo xtask check` exits non-zero and names the manifest, the field and the path

#### Scenario: A script that is not executable fails the check
- **WHEN** a hook command of the plugin runs `${CLAUDE_PLUGIN_ROOT}/scripts/run.sh` and that file has no executable bit
- **THEN** `cargo xtask check` exits non-zero and names the manifest and the script

### Requirement: Finding the binary
The plugin's commands SHALL run through a wrapper that looks for `gsttt` on `PATH` and then in the directory `install.sh` installs into, `GSTTT_INSTALL_DIR` or `~/.local/bin`, so that Claude Code started without that directory on its `PATH` still finds it.

#### Scenario: The binary in the install directory is found
- **WHEN** `gsttt` is not on `PATH` and is in the install directory
- **THEN** the wrapper runs it with the arguments it was given

#### Scenario: The wrapper finds the default install
- **WHEN** `install.sh` has run with only `HOME` set, and `PATH` lacks `~/.local/bin`
- **THEN** the wrapper runs the binary the installer put there

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

### Requirement: Container image
After a release, the `publish` job SHALL push a container image of `gsttt` for `linux/amd64` and `linux/arm64` to `ghcr.io/<owner>/gsttt`, tagged with the version of the release and with `latest` only when the release is the latest. The image SHALL hold the binaries of that release's Linux packages, unpacked after their SHA-256 matched, and a version whose image exists SHALL get no second one.

#### Scenario: The image is built from the checked packages
- **WHEN** the step that publishes the image runs for a release whose Linux packages are in `packages/`
- **THEN** it checks both archives against their `.sha256` files, builds from a context that holds only the `gsttt` of each, and pushes one image for `linux/amd64` and `linux/arm64` tagged `ghcr.io/<owner>/gsttt:<version>`, having given the registry the token on standard input only

#### Scenario: A package that does not match its checksum is not published
- **WHEN** a Linux archive does not match its `.sha256` file
- **THEN** the step fails and builds and pushes nothing

#### Scenario: Only the highest version is the latest image
- **WHEN** the step runs for a version below an existing release
- **THEN** the image is tagged with its version and not with `latest`

#### Scenario: A version with an image is not pushed again
- **WHEN** `ghcr.io/<owner>/gsttt:<version>` exists
- **THEN** the step builds and pushes nothing

#### Scenario: A run after the release was made still publishes the image
- **WHEN** the tag of the version already points at the commit and no image has that version
- **THEN** the step pushes the image

### Requirement: Image contents
The image SHALL hold the `gsttt` binary and nothing else, on a `gcr.io/distroless/static` base, with `/repo` as its working directory and `gsttt` as its entry point. The base and the BuildKit that builds the image SHALL each be pinned by the digest of its index, and the OCI labels `source`, `description`, `licenses`, `version` and `revision` SHALL be repeated as annotations of the index, which is where GitHub reads them for a multi-platform image.

#### Scenario: The image holds only the binary
- **WHEN** the `Dockerfile` is read
- **THEN** it adds the `gsttt` of the target architecture to a distroless static base, runs no command, sets `/repo` as the working directory and makes `gsttt` the entry point

#### Scenario: The index repeats the labels
- **WHEN** the `Dockerfile` and the publish step are read
- **THEN** the step annotates the index with the key of every label of the `Dockerfile`, and with the same text for each label that is fixed text

#### Scenario: The base and the builder are pinned by digest
- **WHEN** the `Dockerfile` and the publish step are read
- **THEN** the base image and the BuildKit image are each named with a tag and a `sha256` digest

### Requirement: Installer skips an installed version

`install.sh` SHALL download nothing when the `gsttt` in the install directory prints `gsttt <version>` for `--version`, `<version>` being the version it would install. It SHALL say that the version is already installed, print the `PATH` hint as after an install, and exit 0.

#### Scenario: An installed version is not downloaded again
- **WHEN** the `gsttt` in `GSTTT_INSTALL_DIR` prints `gsttt 0.1.1` and `install.sh` is to install 0.1.1
- **THEN** it says that 0.1.1 is already installed, fetches the latest version but no package, leaves the install directory as it was, and exits 0
