# Design

## Context

The binary is the only crate that writes to the terminal (`docs/architecture.md`, Surfaces). The engine hands it an `Index` and a `Freshness`. The queries turn those into answers that implement `Answer`, which is `Display` and `Serialize`, with `text(Entries)` for cut or full lists. The config crate evaluates `.gsttt/init.lua` into a `Setup` and generates the editor files. The CLI glues these together and adds three things of its own: path rules, exit codes and trust.

## Decisions

### Layout

- `crates/gsttt/src/main.rs` parses the arguments, installs the panic hook and maps the outcome to an exit code. It has no other logic.
- A library target in the same crate holds the commands, so tests can run them with fake connectors. The tests for the real binary use `env!("CARGO_BIN_EXE_gsttt")` with `std::process::Command`, which needs no new test dependency.

```rust
pub enum Outcome { Answered, Failed, Usage }
pub fn run(cli: Cli, io: &mut Io) -> Outcome;

/// The process's streams and terminal, replaced by buffers and a scripted terminal in tests.
pub struct Io<'a> {
    pub stdout: &'a mut dyn Write,
    pub stderr: &'a mut dyn Write,
    pub terminal: &'a mut dyn Terminal,
}
pub trait Terminal {
    /// Whether stdin and stderr are both terminals, which `config trust` requires.
    fn interactive(&self) -> bool;
    fn read_line(&mut self) -> io::Result<String>;
}
```

- `Outcome` covers the exit codes 0 (`Answered`), 1 (`Failed`) and 2 (`Usage`). The exit code 101 is no `Outcome`: `panics::guard` runs the command and returns the exit code itself, 101 when a panic reached it.
- `completions <shell>` is `clap_complete::generate` over the same `Cli`, for the shells `clap_complete::Shell` lists.
- Numbers in text are grouped by thousands with `query::render::thousands` (`crates/core/src/query/render.rs:300` on the queries branch), made public for this. The CLI does not get a second formatter.

### Exit codes and streams

| Code | When |
| --- | --- |
| 0 | the command answered, including "nothing found" and an `impact` whose inputs are partly unknown |
| 1 | it could not answer: no index with `--no-refresh`, a configuration error, an `IndexError`, a subject path the index does not hold |
| 2 | it cannot run as given: clap's own errors, an empty contract query, a path outside the root, `--changed` outside git, a root that is `$HOME` or `/` |

- On 1 and 2, the message goes to stderr through `miette`, with a `help:` line where there is something to do. Nothing goes to stdout, even with `--json`.
- Logs go to stderr through `tracing-subscriber`. The filter comes from `GSTTT_LOG`, and defaults to `error`.

### The root and paths

- `Workspace::locate` finds the root after `-C`.
- The CLI refuses a root equal to `$HOME` or `/`, comparing canonical paths.
- A path argument is joined to its base and normalised by its text. The base is the current directory for the CLI and the root for the MCP server. Normalising removes `.` and folds `..` without touching the disk, so a deleted file can still be named.
- A result that still leaves the root is an error the caller maps to exit 2.
- The root is a canonical path, so a path spelled through a link, as everything under `/tmp` is on macOS, leaves it by its text. `Repository::argument` reads such a path again, and the text-only helper stays as it is: it folds the path against `/`, resolves the longest prefix that exists through the filesystem, appends the rest as text and reads the result against the root. A path that still leaves the root is the error above, and the root itself keeps its own error.
- The helper lives beside `RepoPath::join` in core, so both surfaces use it:

```rust
impl RepoPath {
    /// `argument` read against `base` (a directory inside `root`), as a path from `root`.
    pub fn from_argument(root: &Utf8Path, base: &Utf8Path, argument: &str) -> Result<Self, OutsideRoot>;
}
```

### Refresh

- Every question builds an `Engine` and calls `refresh()`, then answers from `Refresh.index` with `Freshness::Fresh`.
- `--no-refresh` and `status` call `load()` and `freshness()`. `load()` returning `None` is the "no index yet" failure.
- A subject that the index does not hold is told apart through the answer itself: `in_index` on `Dependents` and `Dependencies`, `unknown` on `Route`. In that case the CLI writes the answer's text to stderr and exits 1. `Impact.unknown` does not fail, because an impact over several inputs still answers for the others.

### Repository

One type holds what every command after argument parsing needs. The MCP server change implements its `IndexSource` trait over this type, so the type has the shape that trait needs and keeps no state between calls other than the evaluated setup. A caller that must see an edit to the config opens it again, as the server does for every call.

```rust
pub struct Repository { workspace: Workspace, registry: Registry, setup: Setup }

/// An index and how the tree stands against it.
pub struct Loaded { pub index: Arc<Index>, pub freshness: Freshness }

/// A refresh: the index it left, and what the run did.
pub struct Refreshed { pub loaded: Loaded, pub unchanged: bool, pub parsed: u32, pub reused: u32, pub elapsed: Duration }

/// A path argument. `on_disk` is false when nothing is at the path, so that the caller asks about it as deleted.
pub struct Target { pub path: RepoPath, pub on_disk: bool }

impl Repository {
    /// Locates the root from `start`, refuses `$HOME` and `/`, builds the registry and evaluates the config.
    pub fn open(start: &Utf8Path) -> Result<Self, RepositoryError>;
    pub fn root(&self) -> &Utf8Path;
    pub fn refreshed(&self) -> Result<Refreshed, RepositoryError>;
    pub fn stored(&self) -> Result<Option<Loaded>, RepositoryError>;
    pub fn argument(&self, base: &Utf8Path, argument: &str) -> Result<Target, RepositoryError>;
    pub fn changed(&self, base: Option<&str>) -> Result<Vec<Change>, RepositoryError>;
}
```

- `RepositoryError` has a sentence for `Display` that names what failed and what to do, because a server passes it to a model unchanged. The CLI words the failures it has more to say about (`-C`, `--changed` and `--base`) itself, and shows the rest as they are.
- `changed` fails with a usage error outside git, which has no history to compare with, and does not call the engine for it.

### Checking options

`DynConnector` gains a provided method that the blanket implementation for `Connector` overrides. An external connector keeps the default, because its options are free-form JSON that only the process reads:

```rust
// crates/core/src/connector.rs
fn check_options(&self, options: &serde_json::Value) -> Result<(), OptionsError> { Ok(()) }

// in the blanket impl: keys outside the schema's `properties` first, then the types
fn check_options(&self, options: &serde_json::Value) -> Result<(), OptionsError> {
    unknown_keys(&schemars::schema_for!(C::Options), options)?;
    serde_json::from_value::<C::Options>(options.clone()).map(drop).map_err(OptionsError::from)
}
```

### Registry

- The order is `ts`, `python`, `rust`, `openapi`, `graphql`, `proto`, `prisma`, `sql`, `env`, then the externals in declaration order.
- Each connector crate exposes its pack sources. The registry evaluates them once with `gsttt_config::builtin_packs` and passes them to `Engine::new`.
- An external enters the registry only when it is trusted.

### Trust

- The key is SHA-256, as hex, over:
  - the canonical JSON of the external's setup: name, cmd, files, version, env, options and provides;
  - for every element of `cmd` that names a regular file inside the root, read against the root, its path from the root and its bytes. A `git pull` can change such a file without touching `init.lua`. Covering every element, not only `cmd[0]`, also covers `python3 tools/connector.py`.
- The key is computed on every run that builds the registry. A connector binary of tens of megabytes costs a few tens of milliseconds to hash, and nothing caches the result, because a cache would be one more thing a pull could get out of step.
- The store is `$XDG_STATE_HOME/gsttt/trust`, or `~/.local/state/gsttt/trust` when that is unset. It holds one line per trusted external, `<key> <canonical root>`, sorted, and is written through a temporary file and a rename.
- `config trust`:
  - refuses unless both stdin and stderr are terminals;
  - prints each untrusted external's name and its full command, quoted;
  - asks once, `Run these commands whenever gsttt indexes <root>? [y/N]`.
- A process with no terminal cannot answer. The point is that an agent cannot trust on the user's behalf.
- `config untrust` removes this root's lines.
- An untrusted external is left out of the registry, so no connector claims its files and coverage counts them as unclaimed. On top of that, the binary adds one coverage problem to every index it answers from:

```text
the external connector `kotlin` (gsttt-kotlin --stdio) is not trusted in this repository; run `gsttt config trust` in a terminal to allow it
```

### Panics

- The hook records the first message and location in a process-wide `OnceLock` and prints nothing. A thread-local would not do: a panic on a rayon worker is handed to the waiting thread with `resume_unwind`, which does not run the hook again, so only the worker's own thread-local would hold the record.
- The engine's `catch_unwind` around connector calls turns a panic into a coverage line, and the record is dropped.
- `main` runs the command inside `catch_unwind` too. When a panic reaches it, `main` prints one line on stderr and exits 101:

```text
gsttt stopped on a bug at crates/core/src/engine/build.rs:142: index out of bounds. Please report it at https://github.com/SkuratovichA/gsttt/issues
```

- With `RUST_BACKTRACE` set, the hook also calls the previous hook, so the full output appears.

### Session-start line

- The hook calls `Workspace::locate` (gix, no `git` process) and `peek`. It reads no configuration and builds no registry.
- Any error prints nothing, and the exit code is 0.
- The line is a pure function, so its test fixes the clock instead of waiting:

```rust
pub fn session_line(header: &Header, now: jiff::Timestamp) -> Option<String>;
```

- The age is `just now` under a minute, whole minutes under an hour, whole hours under two days and whole days after that, from `built_at`. A `built_at` that does not parse, or lies in the future, gives no line.
- The commit is the first seven characters of `head`.

### CI smoke run

- The build matrix gains `smoke: true` on the entries whose runner can execute the target: `x86_64-unknown-linux-musl` on `ubuntu-latest`, and `aarch64-apple-darwin` on `macos-latest`.
- A step after Package, under `if: matrix.smoke`, unpacks the archive and runs `gsttt --version`, `gsttt index` and `gsttt impact crates/core/src/lib.rs` in the checkout.
- `xtask/tests/dist/ci.rs` pins the step and the two flags, the way it already pins the rest of the workflow.

### Init

- `init` opens every connector over the tree to learn which ones find a project, and passes their names to `gsttt_config::starter`.
- It writes `init.lua` and `.luarc.json` only when they are missing.
- It always rewrites `types/gsttt.lua`, whose first line says `gsttt init` generated it.

## Risks

- **A terminal check is a speed bump, not a sandbox.** A user who types `y` without reading still runs the command. The prompt prints the full command so there is something to read.
- **A trusted program can still load code that its command line does not name.** The key covers each file the command names, but not the modules a script imports or the files a binary reads. A pull that changes only those runs without a new prompt. This is listed as a known gap in the README.
