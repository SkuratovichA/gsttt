# Design

## Context

The binary is the only crate that writes to the terminal (`docs/architecture.md`, Surfaces). The engine hands it an `Index` and a `Freshness`. The queries turn those into answers that implement `Answer`, which is `Display` and `Serialize`, with `text(Entries)` for cut or full lists. The config crate reads `.gsttt/config.toml` and the packs beside it into a `Setup`, and generates the JSON schemas that editors read. The CLI glues these together and adds three things of its own: path rules, exit codes and trust.

## Decisions

### Layout

- `crates/gsttt/src/main.rs` parses the arguments, installs the panic hook and maps the outcome to an exit code. It has no other logic.
- A library target in the same crate holds the commands, so tests can run them with fake connectors. The tests for the real binary use `env!("CARGO_BIN_EXE_gsttt")` with `std::process::Command`, which needs no new test dependency.

```rust
pub enum Outcome { Answered, Failed, Usage }
pub fn run(cli: Cli, io: &mut Io) -> Outcome;

/// The process's output streams, replaced by buffers in tests.
pub struct Io<'a> {
    pub stdout: &'a mut dyn Write,
    pub stderr: &'a mut dyn Write,
}
```

- `config trust` is the one command that reads the terminal. It asks `std::io::IsTerminal` of the process's own stdin and stderr, and reads its answer from stdin. There is no stand-in for the terminal in `Io`: the reading of the answer is a function over a `BufRead`, unit-tested with bytes, and the scenarios run the real binary on a pseudo-terminal.
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
pub struct Repository { workspace: Workspace, registry: Registry, setup: Setup, externals: Vec<Declared> }

/// An `[[external]]` of the config, the key its trust is recorded under, and whether the user trusts it in this root.
pub struct Declared { /* private */ }

/// An index and how the tree stands against it.
pub struct Loaded { pub index: Arc<Index>, pub freshness: Freshness }

/// A refresh: the index it left, and what the run did.
pub struct Refreshed { pub loaded: Loaded, pub unchanged: bool, pub parsed: u32, pub reused: u32, pub elapsed: Duration }

/// A path argument. `on_disk` is false when nothing is at the path, so that the caller asks about it as deleted.
pub struct Target { pub path: RepoPath, pub on_disk: bool }

impl Repository {
    /// Locates the root from `start`, refuses `$HOME` and `/`, builds the registry, evaluates the config and adds the externals that are trusted.
    pub fn open(start: &Utf8Path) -> Result<Self, RepositoryError>;
    pub fn root(&self) -> &Utf8Path;
    /// The externals of the config in its order, each with whether it is trusted here.
    pub fn externals(&self) -> &[Declared];
    /// The setup the config evaluated to, and the registry with the built-in packs: what `config show` describes.
    pub const fn setup(&self) -> &Setup;
    pub const fn registry(&self) -> &Registry;
    /// The connectors that find themselves in the tree, in registry order, whatever the config says about them.
    pub fn detected(&self) -> Result<Vec<&'static str>, RepositoryError>;
    pub fn refreshed(&self) -> Result<Refreshed, RepositoryError>;
    pub fn stored(&self) -> Result<Option<Loaded>, RepositoryError>;
    pub fn argument(&self, base: &Utf8Path, argument: &str) -> Result<Target, RepositoryError>;
    pub fn changed(&self, base: Option<&str>) -> Result<Vec<Change>, RepositoryError>;
}
```

- `RepositoryError` has a sentence for `Display` that names what failed and what to do, because a server passes it to a model unchanged. The CLI words the failures it has more to say about (`-C`, `--changed` and `--base`) itself, and shows the rest as they are.
- `changed` fails with a usage error outside git, which has no history to compare with, and does not call the engine for it.

### Checking options

`DynConnector` gains a provided method that the blanket implementation for `Connector` overrides. The override runs the very same deserialization as `open`, one private function that both call, so the check cannot accept what `open` refuses. `TypeScriptOptions` has `deny_unknown_fields`, so an option the module does not declare fails there, with the module and the option named. The default accepts everything, so that no other implementer of `DynConnector` has to change:

```rust
// crates/core/src/connector.rs, in the trait
fn check_options(&self, _options: &serde_json::Value) -> Result<(), ConnectorError> { Ok(()) }

// the one function that reads options; `null` is the defaults
fn read_options<O: DeserializeOwned + Default>(connector: &str, options: &serde_json::Value) -> Result<O, ConnectorError>;

// in the blanket impl, where `open` calls the same function
fn check_options(&self, options: &serde_json::Value) -> Result<(), ConnectorError> {
    read_options::<C::Options>(self.0.meta().name, options).map(drop)
}
```

- `Repository::open` checks every module of the setup against the connector of that name once the trusted externals have joined the registry, collects every failure in the order of the config, and fails with `RepositoryError::Options`: one header line and one indented line for each failure. So every command that opens a repository exits 1 on a bad option, and `config check` has nothing more to check than what `open` does. A table can only name a built-in module, because the evaluation of the config refuses any other, so a module always has its connector to read its options; the `options` of an `[[external]]` are inside its entry, and only its process reads them.
- The error names the module and the option, not a line of the file: the evaluation turns the TOML into JSON before the modules see it, and the position is gone by then. A syntax error, a key the config does not know and an invalid `[[external]]` still name their file, line and column, because the evaluation finds them.
- The evaluation stops at the first file that does not read, so `config check` prints one such error. The option errors of all modules are printed together.

### Registry

- The order is `ts`, `python`, `rust`, `openapi`, `graphql`, `proto`, `prisma`, `sql`, `env`, then the externals in declaration order.
- Each connector crate exposes its pack sources. The registry reads them once with `gsttt_config::builtin_packs` and passes them to `Engine::new`.
- An external enters the registry only when it is trusted, after the built-ins and in declaration order, as `Erased(External::new(setup)?)`.

### Trust

- The key is the SHA-256, as 64 lowercase hex digits, of the parts below, in this order. Every part is written as its length in bytes, as eight bytes in big-endian order, and then the part itself, so that no two lists of parts make the same input:
  - the canonical JSON of the external's setup, made by `keys::canonical_json`, the function the config's own fingerprint uses: name, cmd, files, version, env, options and provides;
  - then, for each element of `cmd`, in order, that names a regular file inside the root, read against the root with links resolved: the path of that file from the root, and its bytes. A `git pull` can change such a file without touching `.gsttt/config.toml`. Covering every element, not only `cmd[0]`, also covers `python3 tools/connector.py`. An element that names nothing, a directory or a file outside the root adds no part: it is a program found in `PATH`, a flag or the value of one.
- A file that the command names, that exists and cannot be read, makes the key fail. The external stays untrusted and `config trust` names the file, because a key that left it out would not change when the file did.

```rust
// crates/gsttt/src/trust/key.rs
let mut hasher = Sha256::new();
feed(&mut hasher, canonical_json(&setup).as_bytes());
for argument in &external.cmd {
    if let Some((from_root, resolved)) = file_in(root, argument) {
        feed(&mut hasher, from_root.as_str().as_bytes());
        feed(&mut hasher, &fs::read(&resolved)?);
    }
}
// feed: hasher.update(length.to_be_bytes()); hasher.update(part);
```

- The key is computed on every run that builds the registry. A connector binary of tens of megabytes costs a few tens of milliseconds to hash, and nothing caches the result, because a cache would be one more thing a pull could get out of step.
- The store is `$XDG_STATE_HOME/gsttt/trust`, or `$HOME/.local/state/gsttt/trust` when that is unset or empty. `UserDir::State` in `crates/core/src/engine/workspace.rs` picks it, by the same function that picks the cache folder. It holds one line per trusted external, `<key> <canonical root>`, sorted and without duplicates, and is written through a temporary file in the same folder and a rename. A missing file is an empty store. A store that cannot be read trusts nothing and logs why.
- `Repository::open` makes the key of each external of the config and looks it up for this root. A trusted external joins the registry after the built-ins, in the order of the config, as `Erased(External::new(setup)?)`, and a failure of `External::new` is an error of the config. An untrusted one stays out, so no connector claims its files and coverage counts them as unclaimed.
- `config trust`:
  - refuses unless both stdin and stderr are terminals. It checks before it reads anything, says that trusting needs a terminal, trusts nothing and exits 2. No variable and no flag skips the check: a process with no terminal cannot answer, and the point is that an agent cannot trust on the user's behalf;
  - prints on stderr each untrusted external's name and its full command, shell-quoted with `shlex` so that an argument with a space reads as one argument;
  - asks once, `Run these commands whenever gsttt indexes <root>? [y/N] `, and reads one line. After trimming, `y` or `yes` in lower case trusts every external it listed. Any other answer, an empty line and the end of input included, trusts nothing and exits 0;
  - with nothing to trust it says so, asks nothing and exits 0. The whole conversation is on stderr and stdout stays empty.
- `config untrust` finds the root, does not read the config, and removes this root's lines.
- On top of leaving it out, `Repository::refreshed` and `Repository::stored` add one coverage problem for each untrusted external to the index before it goes into its `Arc`, and sort the problems again with `Coverage::finish`, with the command quoted as `config trust` shows it:

```text
the external connector `kotlin` (gsttt-kotlin --stdio) is not trusted in this repository; run `gsttt config trust` in a terminal to allow it
```

- The queries of core do not list `Coverage.problems` yet, so `gsttt coverage` prints them itself, above its report as `problems (N):` and under `--json` as a `problems` array (`commands/questions.rs`). The test `the_coverage_document_holds_problems_once` fails when core adds the same key, which is the reminder to delete the wrapper.
- The scenarios that answer the prompt run the real binary on a pseudo-terminal opened with the `pty` module of `rustix`, a dev-dependency that adds no crate to the lock file, with the sandbox's environment and so with the sandbox's store.

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

### Showing the config

- `config show` is one document that is printed as text or, under `--json`, as one JSON object with the same parts: `root`, `modules`, `packs`, `externals`, and the settings `detect`, `exclude`, `max_file_bytes` and `http_prefixes`.
- Each module of the registry is listed in registry order, so the built-in ones and then the externals the user trusts, with `on` and the reason (`why`) in the order the engine decides: `disabled`, `configured` (the config has a table for it), `detection_off`, `detected`, `not_detected`. `on` comes from the reason, and a test pins the reason against `Setup::runs` for every combination of the four facts it rests on.
- `options` is what the config wrote for the module, `null` when it wrote none. The options a module fills in itself are not shown, because the engine would have to be asked what they come to.
- `packs` lists, for each module that is on, the packs it receives as `merge_packs` hands them over, with their origin: `built-in`, or `config` for a pack of the config or of `.gsttt/packs/` that replaces or adds to them.
- `externals` lists every `[[external]]` of the config, whether the user trusts it or not, with `cmd` and `files`. An external that is not trusted is not a module of the run, so it is only here.
- Whether a module is detected comes from `Engine::detected`, the one new method of the engine: it lists the tree the way a refresh does, with the same view and the same panic containment, and asks each connector in registry order. `Repository::detected` wraps it.

### Init

- `init` finds the root like the other commands and does not read the config, so it works beside a config that does not parse and in a repository that has none.
- It learns which connectors find a project from `Engine::detected`, the method `config show` uses, over the same listing a refresh uses. It asks the built-in connectors under the default setup, because the config is not read and an external connector is not one the user has trusted yet, and it passes their names to `gsttt_config::starter`.
- It writes `.gsttt/config.toml` only when it is missing, by creating it with `create_new`, so that a file made between the check and the write is never overwritten. The first line of the starter names the config schema.
- It always writes `.gsttt/schema/config.json` and `.gsttt/schema/pack.json`, the schemas generated from the registry. Each holds a top-level `$comment`, placed first, that says `gsttt init` generated the file and that edits to it are lost. They are indented with two spaces and end in a line break, so that the same registry gives the same bytes.
- A schema that is there is removed and created again. A link in its place is removed and not written through, so that a repository cannot make `init` overwrite a file outside it, and the new file has the permissions the user's umask gives any file.
- It prints `wrote <path>` or `kept <path>` for each file as it goes, in the order config, config schema, pack schema, so that a failure on the third still shows the first two. A file or folder it cannot make ends the run with exit code 1 and the path. It prints text under `--json` too, because the list of files is all it has to say.

### Schema

- `schema external` prints `gsttt_config::protocol_schema()` indented with two spaces and ending in a line break, the form `init` gives the schemas it writes, from one function, `print::indented`. It needs no repository, so it finds no root and runs in the home directory too, and it prints the same text under `--json`, because a schema is JSON already.

## Risks

- **A terminal check is a speed bump, not a sandbox.** A user who types `y` without reading still runs the command. The prompt prints the full command so there is something to read.
- **A trusted program can still load code that its command line does not name.** The key covers each file the command names, but not the modules a script imports or the files a binary reads. A pull that changes only those runs without a new prompt. This is listed as a known gap in the README.
- **A link inside the root that leads out of it is not covered.** The key follows the link and leaves out a target outside the root, so a pull that points the link elsewhere needs no new prompt.
