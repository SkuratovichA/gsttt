## 1. Skeleton

- [x] 1.1 Give `crates/gsttt` a library target with `run(cli, io) -> Outcome` and keep `main.rs` to parsing, the panic hook and the exit code; define every command and global flag with clap derive, `--version` from `GSTTT_VERSION` or `<package version>-dev`, and `completions <shell>` through `clap_complete`
- [x] 1.2 Send errors to stderr through miette, logs through `tracing-subscriber` filtered by `GSTTT_LOG`, and colour through `anstream` only on a terminal with `NO_COLOR` unset

## 2. Root, paths and the repository

- [ ] 2.1 Find the root with `Workspace::locate` after `-C`, and refuse `$HOME` and `/` by their canonical paths
- [ ] 2.2 Add `RepoPath::from_argument` to `crates/core/src/path.rs`, with proptests that its result never holds a `.` or `..` segment and never leaves the root, and read every path argument through it
- [ ] 2.3 Add `Repository` with `open`, `refreshed`, `stored`, `argument` and `changed`, in the shape the MCP server's `IndexSource` needs

## 3. Commands

- [ ] 3.1 Answer `dependents`, `dependencies`, `path`, `contract` and `coverage` after a refresh, with `--json`, `--all` and `--no-refresh`, and exit 1 when the subject is not in the index
- [ ] 3.2 Answer `impact` with paths or `--changed`, `--base` and `--depth`, passing missing named paths as deleted
- [ ] 3.3 Print the `index` summary and its JSON, and answer `status` without refreshing

## 4. Registry and trust

- [ ] 4.1 Build the registry in its fixed order, evaluating the built-in packs once
- [ ] 4.2 Key, store and check trust for externals; leave untrusted externals out with a coverage problem; add `config trust` behind the terminal check and `config untrust`

## 5. Config, init, hook and schema

- [ ] 5.1 Add `DynConnector::check_options` to core and use it in `config check`; print the setup with `config show`
- [ ] 5.2 Write the starter files with `init`
- [ ] 5.3 Print the session-start line through `session_line(header, now)`
- [ ] 5.4 Print the external protocol's schema with `schema external`

## 6. Panics

- [ ] 6.1 Install the quiet panic hook, catch a panic that reaches `main`, print its one line and exit 101

## 7. Dogfooding and CI

- [ ] 7.1 Index a `git archive` copy of this repository in a test and fail on a Rust file outside `tests/fixtures/` that does not parse
- [ ] 7.2 Run the packaged binary in the CI build on the two native targets, and pin the step in `xtask/tests/dist/ci.rs`

## 8. Gates

- [ ] 8.1 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace`, `cargo xtask spec` and `openspec validate add-cli --strict --no-interactive` all pass
