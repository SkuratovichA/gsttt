# gsttt — working in this repository

gsttt ("ghost", glass sphere telling the truth) answers one question deterministically: what does a change affect, across files, packages, languages and the contracts between services. It builds a graph of files (imports) and contracts (HTTP routes, GraphQL operations, gRPC methods, tRPC procedures, tables, queues, env variables), and every answer says what it covered and how fresh it is. No LLM anywhere in the pipeline.

Read `docs/architecture.md` before changing anything in `crates/core`, and the spec of the capability you touch in `openspec/specs/`.

## Layout

| Path | What lives there |
| --- | --- |
| `crates/core` | The data model, the connector contract, the engine, the queries. Language-independent. |
| `crates/lang-ts`, `crates/lang-python`, `crates/lang-rust` | Language connectors, each with its built-in rule packs in `packs/*.toml`. Each depends on `gsttt-core` only. |
| `crates/contracts` | Contract-format connectors: OpenAPI, GraphQL, protobuf, Prisma, SQL, env files. Depends on `gsttt-core` only. |
| `crates/config` | The TOML config (`.gsttt/config.toml`), the rule packs (`.gsttt/packs/*.toml`), the JSON Schemas of both, external connectors over JSON lines. |
| `crates/mcp` | The MCP server. |
| `crates/gsttt` | The binary: CLI, output layer, the connector registry. The only crate that writes to the terminal. |
| `crates/spec`, `crates/scenario`, `xtask` | OpenSpec tooling: the spec parser, `#[scenario]`, the spec gate. |
| `openspec/` | Specs (`specs/`) and change proposals (`changes/`). |
| `plugin/` | The Claude Code plugin that ships the MCP server and the hook. |

A module crate never depends on another module crate. The binary wires them together.

## Commands

All of them run from the repository root.

| Command | What it proves |
| --- | --- |
| `cargo check --workspace --all-targets` | It compiles. |
| `cargo nextest run --workspace` | Tests pass. Use `-p <crate>` for one crate. The `xtask` tests run `cargo deny`, so `cargo-deny` must be installed. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lints are clean. CI runs exactly this. |
| `cargo fmt --all --check` | Formatting. Run `cargo fmt --all` to fix. |
| `cargo insta review` | Review changed snapshots; never accept a snapshot you have not read. |
| `cargo xtask spec` | Every OpenSpec scenario has a test that names it with `#[scenario]`, and every `#[scenario]` names a real scenario. |
| `cargo run -- index` | Dogfooding: gsttt indexes this repository. |

Send long output to a file and read the part you need: `cargo nextest run --workspace > target/test.log 2>&1; tail -c 4000 target/test.log`.

## Rules of the code

- **No panics in shipped code.** `unwrap`, `expect`, `panic!`, `todo!`, `unreachable!`, indexing that can go out of bounds and string slicing by byte offsets are denied by clippy. One bad file must never take the index down: a connector that cannot read a file returns a `ParseStatus` that says why, and the engine catches a panic and turns it into a coverage line. Tests may unwrap.
- **Every exception says why.** `#[expect(lint, reason = "…")]`, never `#[allow]`.
- **Errors**: `thiserror` enums in libraries, `miette` in the binary. An error message names the file and what was expected.
- **No printing outside `crates/gsttt`.** Libraries return values and log with `tracing`.
- **Modules are files**: `foo.rs` with `foo/bar.rs`, never `mod.rs`.
- **Before writing new logic, search for it.** The workspace usually already holds the helper, mapping or validator you are about to write — grep first, reuse or extend what exists, and write new code only when nothing fits. Extending an existing helper beats a parallel copy; when a parallel copy is unavoidable, say where the other one lives and pin the two against each other with a test.
- **Determinism**: the same tree gives the same index, byte for byte. Sort before you emit; never iterate a hash map into output.
- **Speed is a feature**: no per-file allocation you can avoid on the hot path, no disk access during resolution (resolve against `RepoView`), no `String` where a `&str` into the source lives long enough.
- Comments say why, not what. Doc comments on every public item, in plain sentences.

## Tests

- Every scenario in `openspec/specs/**/spec.md` is proven by a test annotated `#[scenario("<capability>", "<scenario name>")]`. `cargo xtask spec` fails on a scenario without a test and on a test naming a scenario that does not exist. A scenario of an open change whose tasks are not all ticked may still lack its test: the gate reports it as pending.
- Connector tests run on small fixture trees under `crates/<crate>/tests/fixtures/` and snapshot the facts with `insta` (JSON). A snapshot is reviewed like code.
- Property tests (`proptest`) for normalisers and parsers of user input: paths, contract keys, value expressions.

## Specs and changes (OpenSpec)

- A change that adds or alters behaviour starts as `openspec/changes/<change>/` with `proposal.md`, `tasks.md` and spec deltas, validated by `openspec validate <change> --strict`.
- Tick tasks in `tasks.md` as they land. When the pull request merges, the `archive-specs` workflow archives every change whose tasks are all done and commits the updated specs. Never run `openspec archive` by hand.

## Commits

Conventional commits (`feat(core): …`, `fix(lang-ts): …`), imperative, one logical change each. A pull request per change; CI must be green.
