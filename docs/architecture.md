# Architecture

gsttt reads a repository into a graph of files and contracts and answers one family of questions about it: what a change affects. It is one binary with no daemon and no model; the same tree and config always give the same graph. This document is the design every crate implements. The behaviour each part must show is pinned by the specs in `openspec/specs/`, and a spec wins where the two disagree.

## Crates

| Crate | Role | Depends on |
| --- | --- | --- |
| `gsttt-core` | Data model, connector contract, rule packs, engine, snapshot, queries, answers | — |
| `gsttt-lang-ts`, `-python`, `-rust` | Language connectors and their built-in packs (`packs/*.lua`, embedded with `include_str!` and exported as `PACKS: &[(&str, &str)]`) | core (config as a dev-dependency, to test packs) |
| `gsttt-contracts` | Contract-format connectors: OpenAPI, GraphQL, protobuf, Prisma, SQL, env files | core |
| `gsttt-config` | `.gsttt/init.lua` → `Setup`, pack evaluation, LuaLS stubs, external connectors | core |
| `gsttt-mcp` | MCP server over the queries | core |
| `gsttt` | The binary: connector registry, CLI, hook, output | everything |
| `gsttt-spec`, `gsttt-scenario`, `xtask` | Spec parser, `#[scenario]`, the spec gate | — |

A module crate never depends on another module crate; the binary wires them. Libraries never print; they return values and log with `tracing`.

## One run

A run turns the working tree into an `Index`. Every query runs one first (a refresh), so an answer is never older than the tree.

1. **Locate.** `Workspace::locate(start)` finds the work tree root, its git dir and the common dir. The state dir is `<git dir>/gsttt/`, so each worktree has its own. Outside git the root is `start` and the state dir is `$XDG_CACHE_HOME/gsttt/<xxh3 of the root path>/` (`~/.cache` when unset). A bare repository is an error.
2. **List files.** In git: the tracked files present in the work tree, plus untracked files git does not ignore. Submodules, symbolic links and skip-worktree entries are left out. Outside git: `RepoView::scan_reporting`. A directory the process cannot list or search is skipped, in git and outside it, and named in the coverage problems. Then `Setup.exclude` (gitignore syntax) removes more. The result is a `RepoView` in path order, so `FileId` order is path order.
3. **Content ids.** Every file gets its git blob id (SHA-1 of `blob <len>\0<bytes>`). A clean tracked file takes it from the git index and is never read. Any other file keeps the id the snapshot stored for it when its size and mtime are unchanged and the stored mtime is more than two seconds older than `listed_at`, the unix second at which the listing that stored it began: that is git's "racy" rule applied to the engine's own stamps, because a file written closer than that to a listing can be saved again with the same size and mtime, so its stamp proves nothing. The rest is hashed. A file above `Setup.max_file_bytes` is not read: its id is made of its size and mtime (`ContentId::from_stat`), so touching it makes it count as modified.
4. **Open connectors.** In registry order (`ts`, `python`, `rust`, `openapi`, `graphql`, `proto`, `prisma`, `sql`, `env`, then externals in config order), every module that `Setup::runs` is opened with its options and the packs of its language. Options are deserialised into the connector's `Options`; a bad option fails the run with the module and the field named. `ConnectorError::Project` (a project root the connector cannot read at all) skips that project and becomes a coverage line. A manifest that does not parse is not such an error: a broken `tsconfig.json` is `Failed(Syntax)` on that file, and its project goes on with default settings.
5. **Assign.** Each file goes to exactly one project: the first connector in registry order with a project whose `matches` accepts it, and among that connector's projects the one with the deepest root. A file no project takes is counted as unclaimed by extension, with the language of that extension when it is a programming language: `Main.kt` is one unclaimed `kt` file, language Kotlin.
6. **Parse.** For each owned file the fact key is `xxh3_128(FACT_FORMAT, connector name, connector version, canonical options JSON, fingerprint of the language's packs, project root, project salt, path, content id)`. Facts come from the first of: this worktree's snapshot, the snapshots of sibling worktrees (`<common dir>/gsttt/` and `<common dir>/worktrees/*/gsttt/`), a parse. A file larger than `Setup.max_file_bytes` or not UTF-8 gets a `ParseFailure` and no facts. Parsing runs on rayon, one file per task. `parse` must be a pure function of the path, the text and the project's configuration: it never reads another file and never stores a `FileId`.
7. **Prepare.** Each project's `prepare` sees the id, path and facts of every file it owns, once, before any resolution. Rust builds its module tree here; Python learns its packages.
8. **Resolve.** Every import of every file goes through its project's `resolve` against the `RepoView`; nothing touches the disk. `File` and `Files` become edges; `External`, `Builtin` and `Unresolved` are counted in coverage.
9. **Embedded documents.** A GraphQL template or an SQL string inside code goes to the first project, in registry order, whose `embeds()` names its language; that project's `parse_embedded` returns contract facts with lines offset to the host file. A language nobody embeds becomes a blind spot in coverage. Embedded documents are re-read on every run; the host file's facts carry their text.
10. **Link.** Each project's `link` sees all its files with facts, resolved imports and connector extras, and returns facts that need more than one file: Express `app.use('/api', router)`, FastAPI `include_router(prefix=…)`, Django `include()`, tRPC router composition, axum `nest`.
11. **Build.** `GraphBuilder` takes the files, their tags, the edges and every contract site (parsed, embedded and linked); coverage counts files by status, imports by resolution and unread sites by reason. A linked site replaces the parsed site of the same file, line, role and contract kind, so a declaration that a mount re-keys is a site once, under its linked key; a link that re-keys a line returns a fact for every parsed fact of that line, role and kind, one per mount, the root included. Embedded sites are never replaced, and a file whose link returned nothing or panicked keeps its parsed sites.
12. **Store.** The index and every file's key, content id, size, mtime, facts and extra go into the snapshot. A run whose keys, file set, config fingerprint and gsttt version all match the snapshot skips steps 7–12 and answers from it.

Phases 6, 8 and 9 run file-parallel and 7 and 10 project-parallel. Output never depends on scheduling: results are collected by `FileId`, and anything emitted from a hash map is sorted first.

## Failure containment

A panic is caught around every connector call: per file for `parse`, `resolve` and `parse_embedded`, per project for `prepare` and `link`. A panicking parse becomes `ParseFailure::Panicked` with the message; any other panic becomes a coverage line naming the connector, the project and the call. The run always finishes, and the answer's coverage says what was lost. The release profile keeps `panic = "unwind"` for this.

## Snapshot

`<state dir>/snapshot.gsttt` holds:

- a 24-byte prefix: the magic `GSTTSNAP`, a `u32` layout version (`LAYOUT`, now 3, bumped by hand whenever the prefix, the header or the body changes shape, or the index in the body comes out of another rule, because a stored index answers as it is), a `u32` header length and the xxh3_64 checksum of the body as a `u64`;
- a JSON header: `built_by`, `built_at` (RFC 3339), `head`, `dirty`, `files`, the connectors that ran and a one-line coverage summary;
- padding to 16 bytes and the rkyv body (`Index`, the per-file records of step 12, the config and registry fingerprints).

It is written to a temporary file in the same directory, flushed to disk with `fsync` and then renamed, so a reader sees either the old or the new snapshot, never half of one, and a crash after the rename cannot leave a snapshot whose blocks never reached the disk. A write first removes the temporary files that nobody has touched for an hour, which a process that died in the middle of a write leaves behind. A snapshot with another magic, layout or `built_by` is not read; the run starts cold and replaces it. The body has to match the checksum of the prefix and to pass rkyv's validation on read, and a body that fails either counts as no snapshot. `peek` reads the header alone, which is what lets the hook answer in a few milliseconds.

There is no other cache. Sharing between worktrees comes from reading sibling snapshots in step 6, and a removed worktree takes its snapshot with it.

## Contracts

A contract is a seam between files that imports do not show. Its key is `<kind> <key>`, canonical so that two sites naming the same seam produce equal keys (`ContractKey::new`):

| Kind | Example | Canonical form |
| --- | --- | --- |
| `http` | `http GET /bookings/{}` | method upper case or `*`; path through `normalize_http_path` (host, query, trailing slash and a leading placeholder base dropped; parameters become `{}`) |
| `graphql` | `graphql Query.bookings` | root type and field, as written |
| `grpc` | `grpc booking.v1.BookingService/GetBooking` | package, service, method |
| `trpc` | `trpc booking.get` | procedure path |
| `table` | `table booking_items` | lower case; a table or an ORM model |
| `queue` | `queue booking-created` | as written |
| `env` | `env DATABASE_URL` | upper case: pydantic settings read `database_url` from `DATABASE_URL`, and no project tells two variables apart by case alone |
| other | `<kind> <key>` | kinds a pack or an external connector introduces; trimmed |

Every site has a role. `declares`: a schema or spec defines it (OpenAPI, `.proto`, GraphQL SDL, a Prisma model, a migration, `.env.example`). `serves`: code implements it (a controller, a resolver). `calls`: code uses it (a fetch, a query, an env read). A Prisma model declares both its model name and its mapped table, so `prisma.bookingItem` and `FROM booking_items` meet at the schema.

Keys meet by equality, except HTTP: a call and a route meet when the methods are equal or either is `*`, and the paths, after `Setup.http_prefixes` is stripped from either side, have the same number of segments, each equal or `{}` on either side. `GET /api/bookings/me` meets `GET /bookings/{}` under the prefix `/api`.

Framework knowledge lives in rule packs (`gsttt_core::rules`), not in connector code. A pack is a Lua table of call, decorator, chain, embed and assignment rules; a connector finds the sites with `callee_matches` and hands each to the rule's `apply`, which returns a fact, a blind spot or nothing. A pack is active in a project when one of its `detect` package names is a dependency in that project's manifest, or always when `detect` is empty. Built-in packs live beside their connector; a user's packs come from the config and replace a built-in pack of the same name.

A call or decorator rule can take a key prefix from where its receiver was made (`receiver`: `router = APIRouter(prefix="/x")`, `Blueprint(url_prefix=…)`) or from a decorator of the enclosing class (`class`: NestJS `@Controller("bookings")`), and a decorator rule can add one read at the decorator itself (`prefix`). A rule with `receiver` or `class` applies only where that maker is found: the receiver is a name bound in the same file to a call matching `from`, or the innermost enclosing class carries a decorator matching `from`. A receiver imported from another file is left to `link`, which sees both files. A `Prefix` without `key` only gates the rule (`const app = express()`), which is also how a rule stays off `cache.get("/x")`.

## Queries

Queries are pure functions over `&Index` in `gsttt_core::query`. Each returns an answer that implements `Display` (the text the CLI prints and the MCP server returns) and `Serialize` (for `--json`).

- **impact(files)**: every file a change to `files` can affect. It is the fixpoint of two steps from every affected file: its dependents through import edges, skipping edges that are only Rust `mod` declarations; and contract crossing, where a file that declares a contract affects every file that serves or calls it, and a file that serves one affects every file that calls it. A file that only calls a contract affects nobody through it. Each affected file carries its distance and the first reason it was reached (`imports src/a.ts`, `calls http GET /bookings/{}`). Test files (`FileTags::TEST`) are listed apart, because "which tests to run" is the commonest use.
- **dependents(file)** and **dependencies(file)**: one level, with edge kinds; dependencies also lists packages, builtins and unresolved imports with their reasons.
- **path(from, to)**: the shortest chain of imports and contract hops (calls → serves → declares) between two files.
- **contract(query)**: a canonical key gives its sites by role, HTTP keys with every route or call that meets them; any other text is a case-insensitive search over keys.
- **coverage()**: per connector, files by status, the share of imports resolved, unresolved imports and unread sites by reason with the first place each occurred, what the connector `provides` with its blind spots, and unclaimed files by language.
- **status()**: when and from what the index was built, and whether the tree has changed since.

Answers are short and honest:

- Paths are relative to the root; sites are `path:line`.
- A list stops at 30 entries with `… N more`; `--all` lifts the cap.
- An empty result says what was searched: `nothing imports src/x.ts (searched 4,213 files read by ts, python)`. When unresolved imports elsewhere end with the file's stem, they are listed as possible misses.
- Every answer ends with one line of coverage for the connectors it touched and one of freshness: `fresh at a1b2c3d, 3 files dirty` or, under `--no-refresh`, `stale: 2 files changed since the index`.

## Surfaces

**CLI**, named commands only:

```
gsttt index                         build or refresh, print a summary
gsttt impact <path>… | --changed [--base <rev>] [--depth N]
gsttt dependents <path>   gsttt dependencies <path>   gsttt path <from> <to>
gsttt contract <query>    gsttt coverage              gsttt status
gsttt mcp                           MCP server on stdio
gsttt hook session-start            one line of context for a Claude Code session
gsttt init                          write .gsttt/init.lua and LuaLS stubs
gsttt config check | show           evaluate the config; print errors or the effective setup
```

Global flags: `-C <dir>`, `--json`, `--all`, `--no-refresh`. `--changed` takes the files git reports as modified, added or untracked; `--base <rev>` adds the files changed since the merge base with `<rev>`; deleted files are listed and not traced. Exit codes: 0 answered, 1 error, 2 usage. The CLI's text is English; the binary is the only crate that writes to the terminal, and it renders errors with `miette`.

**MCP** (`gsttt mcp`, `rmcp` over stdio). Tools: `impact`, `dependents`, `dependencies`, `path`, `contract`, `coverage`, `index_status`. `initialize` answers without touching the repository; each tool call refreshes, then returns the same text the CLI prints. A failure is a tool error with the message, never a protocol error. The crate depends on core only: the binary hands it an `IndexSource` that refreshes and returns the index.

**Hook.** `gsttt hook session-start` reads the snapshot header with `peek` and prints one line (`gsttt: 4,213 files indexed at a1b2c3d (3 dirty), 2 h ago; use the gsttt tools for impact before changing shared code`) or nothing when there is no snapshot. It runs no git command and finishes in under 10 ms.

**Plugin.** `plugin/` is a Claude Code plugin: `.claude-plugin/plugin.json`, `.mcp.json` starting `gsttt mcp`, `hooks/hooks.json` running the hook on `SessionStart`, and a skill that says when to ask gsttt. The repository root carries `.claude-plugin/marketplace.json`, so `/plugin marketplace add SkuratovichA/gsttt` installs it. The plugin expects `gsttt` on `PATH`, and its hook prints an install hint when the binary is missing.

## Configuration

`.gsttt/init.lua` is optional; without it every detected module runs with defaults.

```lua
local g = require("gsttt")

g.setup { exclude = { "fixtures/**" }, http_prefixes = { "/api" } }
g.ts { tsconfig = "tsconfig.base.json" }  -- turn a module on, with options
g.disable "openapi"
g.pack(require("packs.billing"))          -- .gsttt/lua/packs/billing.lua
g.disable_pack "axios"
g.external { name = "kotlin", cmd = { "gsttt-kotlin" }, files = { "**/*.kt" }, version = "1" }
```

- `g.<module>` exists for every module in the registry; an unknown name fails with the list of known ones. Options are checked against the module's schema when the engine opens it.
- `require` loads only from `.gsttt/lua/`. The sandbox has `table`, `string`, `math` and `utf8`, no `io`, `os`, `package`, `debug`, `load`, `loadfile` or `dofile`, a memory limit and an instruction limit; `print` goes to the log. Evaluation is deterministic.
- `gsttt init` writes `.gsttt/init.lua`, LuaLS annotations generated from the module schemas (`.gsttt/types/gsttt.lua`) and `.gsttt/.luarc.json`, so an editor completes and checks the config.

Pack merging is the engine's: built-in packs, minus `disable_packs`, with user packs replacing built-ins of the same name, grouped by `language`.

**External connectors** run as a command and speak JSON lines on stdin and stdout, one process per run, started in the root:

```
→ {"type":"open","protocol":1,"root":"","options":{}}
← {"type":"ready","name":"kotlin","version":"0.1.0","provides":[…]}
→ {"type":"parse","path":"src/A.kt","text":"…"}                      ← {"type":"facts","path":"src/A.kt","facts":{…FileFacts…}}
→ {"type":"resolve","from":"src/A.kt","import":{…Import…}}            ← {"type":"resolution","resolution":{…}}
→ {"type":"done"}
```

Files are named by path, never by id. The messages are the serde forms of the core types, and `gsttt schema external` prints their JSON schema. A process that exits, answers out of turn or takes longer than 10 s for one request fails the files it has not answered (`ParseFailure::Panicked` naming the connector) and the run goes on.

## Specs and tests

Behaviour is specified in OpenSpec (`openspec/specs/<capability>/spec.md`): requirements with SHALL, scenarios with WHEN and THEN. Every scenario is proven by a test annotated `#[scenario("<capability>", "<scenario>")]`. `cargo xtask spec` fails on a scenario without a test, a test naming a scenario that does not exist, a scenario proven twice, two scenarios of one capability with one title, and a test claiming a scenario that an open change removes; it reads the current specs and the deltas of open changes. A scenario that an open change adds or modifies may lack its test while that change has an unticked task, and is reported as pending; once every task is ticked it fails like any other, so a change lands in several pull requests without breaking the build. A change starts as `openspec/changes/<change>/`; when its pull request merges with every task ticked, the `archive-specs` workflow archives it and commits the updated specs.

## Performance targets

Measured on an Apple M-series laptop, release build:

| What | Target |
| --- | --- |
| Cold index, 10k-file TypeScript monorepo | < 2 s |
| Cold index, 100k files | < 15 s |
| Refresh with nothing changed, 10k files | < 100 ms |
| Refresh after one edited file, 10k files | < 300 ms |
| Hook | < 10 ms |
| MCP `initialize` | < 50 ms |

The hot path allocates as little as it can: facts borrow from the source where they live long enough, resolution never touches the disk, ids are dense `u32`s and walks use bitsets.

## Known gaps in 0.1

Each one is listed in its connector's `provides` blind spots, so `gsttt coverage` reports it instead of hiding it: file-system routes (Next.js `app/`, SvelteKit), Diesel `table!`, tonic server impls, Flask routes declared with several methods in a list beyond the first, imports with computed paths, dependency-injection containers, and code generated into git-ignored directories.

The engine has one gap that is not a connector's blind spot and that `gsttt coverage` does not report: when git filters a file on checkout (`core.autocrlf`, a `.gitattributes` filter), a clean file gets the blob id from the git index while its connector parses the bytes in the work tree, so its facts are keyed by a blob they were not made from, and a filtered file whose stored stat is stale is hashed as it lies, never equals the blob of HEAD, and shows as modified in `dirty` and in the changed files.

Another gap that `gsttt coverage` does not report: facts are filed under the id the listing gave, and the bytes `parse` reads afterwards re-key them only when the file's size or mtime moved in between. A save that keeps both, the same size in the same clock tick of the file system, leaves the facts of the new bytes under the id of the old ones, so a revert to the old content, or a sibling worktree that holds it, is given them until the next run replaces the entry.
