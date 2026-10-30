# Architecture

gsttt reads a repository into a graph of files and contracts and answers one family of questions about it: what a change affects. It is one binary with no daemon and no model; the same tree and config always give the same graph. This document is the design every crate implements. The behaviour each part must show is pinned by the specs in `openspec/specs/`, and a spec wins where the two disagree.

## Crates

| Crate | Role | Depends on |
| --- | --- | --- |
| `gsttt-core` | Data model, connector contract, rule packs, engine, snapshot, queries, answers | — |
| `gsttt-lang-ts`, `-python`, `-rust` | Language connectors and their built-in packs (`packs/*.toml`, embedded with `include_str!` and exported as `PACKS: &[(&str, &str)]`) | core (config as a dev-dependency, to test packs) |
| `gsttt-contracts` | Contract-format connectors: OpenAPI, GraphQL, protobuf, Prisma, SQL, env files | core |
| `gsttt-config` | `.gsttt/config.toml` and `.gsttt/packs/*.toml` → `Setup`, the JSON Schemas of both, external connectors | core |
| `gsttt-mcp` | MCP server over the queries | core |
| `gsttt` | The binary: connector registry, CLI, hook, output | everything |
| `gsttt-spec`, `gsttt-scenario` | Spec parser, `#[scenario]` | — |
| `xtask` | The spec gate (`xtask/src/spec/`), and the build and checks of a release: `cargo xtask package`, `check`, `notes`, `version` (`xtask/src/dist/`) | — |

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
11. **Build.** `GraphBuilder` takes the files, their tags, the declarations of the files whose connector records them, the edges and every contract site (parsed, embedded and linked); coverage counts files by status, imports by resolution and unread sites by reason. A linked fact may name the parsed fact of its file, line and role that it keys again (`LinkedFact::replaces`, set by `LinkedFact::rekeyed`), and the build drops the site of each parsed fact so named, so a declaration that a mount re-keys is a site under each linked key and not under its parsed one; a parsed fact that no linked fact names keeps its site, whatever shares its line. A link that keys a parsed fact again returns it under every mount, the root included. Embedded sites are never replaced, and a file whose link returned nothing or panicked keeps its parsed sites.
12. **Store.** The index and every file's key, content id, size, mtime, facts and extra go into the snapshot. A run whose keys, file set, config fingerprint and gsttt version all match the snapshot skips steps 7–12 and answers from it.

Phases 6, 8 and 9 run file-parallel and 7 and 10 project-parallel. Output never depends on scheduling: results are collected by `FileId`, and anything emitted from a hash map is sorted first.

## Failure containment

A panic is caught around every connector call: per file for `parse`, `resolve` and `parse_embedded`, per project for `prepare` and `link`. A panicking parse becomes `ParseFailure::Panicked` with the message; any other panic becomes a coverage line naming the connector, the project and the call. The run always finishes, and the answer's coverage says what was lost. The release profile keeps `panic = "unwind"` for this.

## Snapshot

`<state dir>/snapshot.gsttt` holds:

- a 24-byte prefix: the magic `GSTTSNAP`, a `u32` layout version (`LAYOUT`, now 6, bumped by hand whenever the prefix, the header or the body changes shape, or the index in the body comes out of another rule, because a stored index answers as it is), a `u32` header length and the xxh3_64 checksum of the body as a `u64`;
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

An origin, which `path_after_origin` reads in one place, may be written with placeholders: a placeholder scheme and host as in `${}://${}/v1/x`, or a placeholder, `//` and a host that holds a placeholder as in `${}//${}/v1/x`, which is what a protocol that holds its own colon leaves. Every segment of an HTTP path is trimmed of whitespace, so `/bookings /{id}` and `/bookings/{id}` are one key.

Every site has a role. `declares`: a schema or spec defines it (OpenAPI, `.proto`, GraphQL SDL, a Prisma model, a migration, `.env.example`). `serves`: code implements it (a controller, a resolver). `calls`: code uses it (a fetch, a query, an env read). A Prisma model declares both its model name and its mapped table, so `prisma.bookingItem` and `FROM booking_items` meet at the schema.

Keys meet by equality, except HTTP: a call and a route meet when the methods are equal or either is `*`, and the paths, after `Setup.http_prefixes` is stripped from either side, have the same number of segments, each equal or `{}` on either side. `GET /api/bookings/me` meets `GET /bookings/{}` under the prefix `/api`.

Framework knowledge lives in rule packs (`gsttt_core::rules`), not in connector code. A pack is a TOML file of call, decorator, chain, embed and assignment rules; a connector finds the sites with `callee_matches` and hands each to the rule's `apply`, which returns a fact, a blind spot or nothing. A pack is active in a project when one of its `detect` package names is a dependency in that project's manifest, or always when `detect` is empty. Built-in packs live beside their connector; a user's packs come from `.gsttt/packs/` and replace a built-in pack of the same name.

A call or decorator rule can take a key prefix from where its receiver was made (`receiver`: `router = APIRouter(prefix="/x")`, `Blueprint(url_prefix=…)`) or from a decorator of the enclosing class (`class`: NestJS `@Controller("bookings")`), and a decorator rule can add one read at the decorator itself (`prefix`). A rule with `receiver` or `class` applies only where that maker is found: the receiver is a name bound in the same file to a call matching `from`, or the innermost enclosing class carries a decorator matching `from`. A receiver imported from another file is left to `link`, which sees both files. A `Prefix` without `key` only gates the rule (`const app = express()`), which is also how a rule stays off `cache.get("/x")`.

## Queries

Queries are pure functions over `&Index` in `gsttt_core::query`. Each returns an answer that implements `Display` (the text the CLI prints and the MCP server returns) and `Serialize` (for `--json`).

- **impact(files)**: every file a change to `files` can affect. It is the fixpoint of two steps from every affected file: its dependents through import edges, skipping edges that are only Rust `mod` declarations; and contract crossing, where a file that declares a contract affects every file that serves or calls it, and a file that serves one affects every file that calls it. A file that only calls a contract affects nobody through it. Where an import edge records the names the importer takes, the walk follows names and not files. A changed file, and a file reached across a contract, is affected in all its names. An importer is affected in all its names as soon as it imports by name one of the affected names of the file or a name the file does not export, and as soon as it imports a namespace, imports for side effects, imports dynamically, by `require`, or mocks it. An edge into a file whose exports are unknown is followed at file level, whatever names it carries, so a connector that lists no exports, Python and Rust today, gets the files, distances and reasons of a walk over files. A file that only re-exports is affected in the names it passes on: a named re-export passes on the names it exports them as, and `export *` passes on every affected name but `default`, where the names of a file are those it exports itself and those it takes in through its own `export *`; an `export *` always reaches its file, held with no names when there is nothing to pass on. A file that does not list what it exports could pass on any name, so it is affected in all of them. Only a file affected in all its names crosses contracts. A file keeps the distance and the reason of the first time the walk reached it, and a barrel that gains names is visited again with only the new ones, so a graph whose edges name nothing costs what a walk over files costs. Each affected file carries its distance and the first reason it was reached (`imports useFoo from src/a.ts`, `re-exports fromA from src/a.ts`, `re-exports useFoo as useBar from src/a.ts` where the barrel renames it, `imports src/a.ts` where it takes every name, `calls http GET /bookings/{}`), and the text cuts a list of names after three where the JSON keeps them all. Test files (`FileTags::TEST`) are listed apart, because "which tests to run" is the commonest use.
- **impact(files, names)**: the walk over declarations. Asked about names that the files declare, `impact` lists their direct callers and stops, because a caller may keep its behaviour when what it calls changes; a request that gives a depth makes each further hop go from the declarations reached to the declarations that refer to them. A connector that reads declarations records them as `FileFacts.symbols`: every declaration with the lines it spans and the names it refers to (`uses`: declarations of the file, `Class.member` for `this.member`, the names at the head of the type of `this.field` and `Type.member` for `this.field.member`, and the local names of its imports), what the file's code refers to as it loads (`on_load`), and each name it exports from a local of another name; `Import.exported` of a static import carries the local name of each name it takes, which is how a walk finds the declarations that use it. A use of `Type.member` is a use of that member only, and a bare use of `Type` a use of every member, so a call through a field is a caller of the member it calls and no other; the member goes on across the files that import its class, and a namespace import takes every member. A member of a class is a declaration of its own, `Class.member`. The graph keeps the symbols of every file in one flat table (`graph/symbols.rs`: every string back to back, no interning and no map per file), so that building it costs a copy and a query reads a few slices. They live twice, in the stored facts, which a refresh reuses for the files that did not change, and in the graph, which the queries read. The walk keeps, for each file, which of its declarations it has reached, whether it has reached the code that runs as the file loads, and whether it has reached the whole file, which is how a file whose connector records no declarations is followed: as without names, with a note. One hop from the declarations reached in a file reaches the declarations of that file that use them, and its load when `on_load` holds one. An importer that takes an export whose local is reached, by name or through a chain of re-exports, is reached in the declarations that use the local name it binds, or in its load, and an import that nothing uses reaches nothing. A barrel on the way, and an importer that exports again a name it imports, are listed with the reason that they re-export or import it and add no hop, and `export *` passes on every name but `default`. A namespace import is a use of every name by whatever uses the namespace; a dynamic import and `require` that take the module itself are a use of every name, and one that names what it reads, such as `(await import('./a')).foo` or a destructuring, a use of those names, by the declaration whose lines hold the import or by the load; an import type such as `import('./a').foo` is a use of the names it writes, by the declaration whose lines hold it and nowhere else, so outside every declaration it reaches nothing, because a type does not run as the file loads; a mock takes no name, because a test that mocks a module runs what replaces it, and calls a file followed whole only where it mocks it; a side-effect import uses no name, and reaches its importer only from a file that is reached at load or followed whole. The callers of a member `C.m` outside `C` are the declarations that refer to `C`, because a call through a value names no class, and the answer says so. A file reached at load is listed as such, a test file among the tests, and one hop from it reaches at load every file that loads or mocks it, which a type-only import does not, and crosses all its contracts. A contract site belongs to the narrowest declaration whose lines hold it, a member before its class, and to the load of its file when none does. A declaration reached by name crosses the contracts of its own sites and no others, and across a contract the walk reaches the declaration, or the load, that each site of the other side belongs to, or the whole file when it records no declarations. A file without declarations reached on the way, and every edge into a file whose exports are unknown, are followed whole. With names the depth is 1 unless the request gives one; when it stops the walk with callers left to follow, a note says so, and that asking about a caller by name, or with a larger depth, goes on. An entry carries its callers, each a declaration with the line it starts on and its own distance from the declarations asked about, in byte order of their names and cut after three like the names of an edge, and `load` when its load is reached, with its distance; a caller or a load farther away than the file is listed says so, `outer (line 5, distance 2)` and `; at load (distance 2)`. The JSON keeps them all, as `callers` (`name`, `line`, `distance`) and `load` (`distance`), beside the `names` asked and the `notes`. The header reads `impact of getHash in packages/vite/src/node/utils.ts, within 1 hop`. The notes say a name that a file does not declare, a file followed whole because its connector records no declarations, the members whose callers are found through their classes, which are one note for all of them, cut after three in the text and whole in the JSON, and a walk that the depth stopped. Without names the walk, its answers and its JSON are the ones above, unchanged. Names go with paths, and `--changed` refuses them, because which declarations a diff changes is not derived.
- **dependents(file)** and **dependencies(file)**: one level, with edge kinds; dependencies also lists packages, builtins and unresolved imports with their reasons.
- **path(from, to)**: the shortest chain of imports and contract hops (calls → serves → declares) between two files.
- **contract(query)**: a canonical key gives its sites by role, HTTP keys with every route or call that meets them; any other text is a case-insensitive search over keys.
- **coverage()**: per connector, files by status, the share of imports resolved, unresolved imports and unread sites by reason with the first place each occurred, what the connector `provides` with its blind spots, and unclaimed files by language.
- **status()**: when and from what the index was built, and whether the tree has changed since.

Answers are short and honest:

- Paths are relative to the root; sites are `path:line`.
- A list stops at 30 entries with `… N more`; `--all` lifts the cap. Through the MCP server an answer also stops at 25,000 characters, after a first line that starts with `cut:`.
- An empty result says what was searched: `nothing imports src/x.ts among 4,213 files read by ts and python`. When unresolved imports elsewhere end with the file's stem, they are listed as possible misses.
- Every answer ends with one line of coverage for the connectors it touched and one of freshness: `fresh at a1b2c3d, 3 files dirty` or, under `--no-refresh`, `stale: 2 files changed since the index`.

## Surfaces

**CLI**, named commands only:

```
gsttt index                         build or refresh, print a summary
gsttt impact <path>… [--name <name>]… | --changed [--base <rev>] [--depth N]
gsttt dependents <path>   gsttt dependencies <path>   gsttt path <from> <to>
gsttt contract <query>    gsttt coverage              gsttt status
gsttt mcp                           MCP server on stdio
gsttt hook session-start            one line of context for a Claude Code session
gsttt init                          write .gsttt/config.toml and the JSON Schemas that check it
gsttt config check | show           evaluate the config; print errors or the effective setup
```

Global flags: `-C <dir>`, `--json`, `--all`, `--no-refresh`. `--changed` takes the files git reports as modified, added or untracked; `--base <rev>` adds the files changed since the merge base with `<rev>`; deleted files are listed and not traced. `--name <name>` may be repeated and asks the paths about the names they declare; it is refused with `--changed`, and so is a blank name. Exit codes: 0 answered, 1 error, 2 usage. The CLI's text is English; the binary is the only crate that writes to the terminal, and it renders errors with `miette`.

**MCP** (`gsttt mcp`, `rmcp` over stdio). Tools: `impact` (which takes `names` beside `paths`), `dependents`, `dependencies`, `path`, `contract`, `coverage`, `index_status`. `initialize` answers without touching the repository; each tool call refreshes, then returns the same text the CLI prints. A failure is a tool error with the message, never a protocol error. The crate depends on core only: the binary hands it an `IndexSource` that refreshes and returns the index.

**Hook.** `gsttt hook session-start` reads the snapshot header with `peek` and prints one line (`gsttt: 4,213 files indexed at a1b2c3d (3 dirty), 2 h ago; use the gsttt tools for impact before changing shared code`) or nothing when there is no snapshot. It runs no git command and finishes in under 10 ms.

**Plugin.** `plugin/` is a Claude Code plugin: `.claude-plugin/plugin.json`, which declares the MCP server (`gsttt mcp`) and the hook that runs on `SessionStart` inline, `scripts/gsttt.sh`, which runs both, and a skill that says when to ask gsttt. The repository root carries `.claude-plugin/marketplace.json`, so `/plugin marketplace add SkuratovichA/gsttt` installs it. The wrapper finds `gsttt` on `PATH` or in the directory `install.sh` puts it in, and when it is in neither, the hook prints an install hint for the user and still ends with status 0, so a missing binary never breaks a session.

## Configuration

`.gsttt/config.toml` is optional; without it every detected module runs with defaults.

```toml
#:schema ./schema/config.json

exclude = ["fixtures/**"]
http_prefixes = ["/api"]
disable = ["openapi"]
disable_packs = ["axios"]

[ts]                                 # a table turns a module on, and holds its options
tsconfig = "tsconfig.base.json"

[[external]]
name = "kotlin"
cmd = ["gsttt-kotlin"]
files = ["**/*.kt"]
version = "1"
env = ["JAVA_HOME"]                  # variables it may read
options = { strict = true }          # sent in "open"

[[external.provides]]                # what it finds and cannot see
kind = "http"
role = "serves"
via = ["ktor"]
blind_spots = ["routes built at run time"]
```

- The top level holds `exclude`, `http_prefixes`, `max_file_bytes`, `detect`, `disable`, `disable_packs`, the `[[external]]` entries and one table per module of the registry. Any other key fails with the keys and the modules it could have been. Modules enter the setup in the order of the registry, whatever the order of the tables, and externals in the order of the file. A module's table is checked against the module's own options when the engine opens it.
- A pack is a file in `.gsttt/packs/` named for the pack: `billing.toml` holds `name = "billing"`, `language`, `detect` and the rules as `[[calls]]`, `[[decorators]]`, `[[chains]]`, `[[embeds]]`, `[[assigns]]` and `[[imports]]` tables. Only files that end in `.toml` and do not start with `.` are read, in the order of their names. The built-in packs are TOML files read by the same reader.
- A file is data: nothing in it runs, so the same file gives the same setup. The reader refuses a file that is not a regular file, one that links outside the repository, one over 1 MiB, a date or a time, a float that is not finite, and nesting deeper than 64 levels, where an array of tables counts twice. An error reads `<file>:<line>:<column>: <message>` and is cut at 1 KiB.
- `gsttt init` writes `.gsttt/config.toml` with its `#:schema` line, and the JSON Schemas of the config and of a pack in `.gsttt/schema/`, so an editor with a TOML language server (taplo, Even Better TOML) completes and checks both. The schemas are generated from the types the reader reads into, `config_schema` for the modules of the registry and `pack_schema`, so a key the editor accepts is a key the reader accepts.

Pack merging is the engine's: built-in packs, minus `disable_packs`, with user packs replacing built-ins of the same name, grouped by `language`.

**External connectors** run as a command and speak JSON lines on stdin and stdout, one process per run, started in the root:

```
→ {"type":"open","protocol":1,"root":"","options":{}}
← {"type":"ready","name":"kotlin","version":"0.1.0"}
→ {"type":"parse","path":"src/A.kt","text":"…"}                      ← {"type":"facts","path":"src/A.kt","facts":{…FileFacts…}}
→ {"type":"resolve","from":"src/A.kt","import":{…Import…}}            ← {"type":"resolution","from":"src/A.kt","import":{…Import…},"resolution":{…}}
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
