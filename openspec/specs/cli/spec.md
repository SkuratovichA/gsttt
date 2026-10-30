# cli Specification

## Purpose
The command line is how a person at a terminal and a script in CI ask gsttt about a repository: it finds the root, keeps the index fresh before it answers, prints bounded or JSON answers with exit codes a script can act on, checks and shows the config, decides which external connectors may run, and gives an agent's session one line about the index when it starts.

## Requirements

### Requirement: Commands

The `gsttt` binary SHALL offer the commands `index`, `impact`, `dependents`, `dependencies`, `path`, `contract`, `coverage`, `status`, `mcp`, `hook session-start`, `init`, `config check`, `config show`, `config trust`, `config untrust`, `schema external` and `completions <shell>`, and the global flags `-C <dir>`, `--json`, `--all` and `--no-refresh`. With no command it SHALL print the usage to stderr and exit 2.

#### Scenario: No command prints the usage
- **WHEN** `gsttt` runs with no arguments
- **THEN** stderr lists every command, stdout is empty and the exit code is 2

#### Scenario: Completions are printed for a shell
- **WHEN** `gsttt completions zsh` runs
- **THEN** stdout holds a zsh completion script for `gsttt` and the exit code is 0

### Requirement: Version

`gsttt --version` SHALL print `gsttt` and the version: `GSTTT_VERSION` as it was at build time, or the package version followed by `-dev` when it was unset. `gsttt status` SHALL name the version that built the index in the same form.

#### Scenario: A development build says so in its version
- **WHEN** `gsttt --version` runs on a binary built without `GSTTT_VERSION`
- **THEN** stdout is `gsttt <package version>-dev` and the exit code is 0

#### Scenario: status names the version --version prints
- **WHEN** a repository is indexed and `gsttt status` runs
- **THEN** stdout says that the index was built by the version `gsttt --version` prints

### Requirement: Exit codes

A command SHALL exit 0 when it answered, including an answer that found nothing and an `impact` some of whose inputs the index does not hold. It SHALL exit 1 when it could not answer: no index under `--no-refresh` or for `status`, a configuration error, a failed index, an I/O error, or a subject of `dependents`, `dependencies` or `path` that the index does not hold.

#### Scenario: A subject the index does not hold exits 1
- **WHEN** `gsttt dependencies lib/missing.ts` runs and no file `lib/missing.ts` is indexed
- **THEN** stderr says `lib/missing.ts` is not in the index, stdout is empty and the exit code is 1

### Requirement: Usage errors

A command SHALL exit 2 when it cannot run as given: an unknown command or flag, a missing or conflicting argument, a path outside the root, a bad `-C`, a refused root, an empty contract query, `--no-refresh` with `index`, `--base` without `--changed`, `--changed` outside git, or an unknown `--base` revision.

#### Scenario: An unknown flag exits 2
- **WHEN** `gsttt impact --frobnicate src/a.ts` runs
- **THEN** stderr names `--frobnicate`, stdout is empty and the exit code is 2

#### Scenario: An empty contract query exits 2
- **WHEN** `gsttt contract ""` runs
- **THEN** stderr says the query is empty, stdout is empty and the exit code is 2

### Requirement: Output streams

stdout SHALL carry the answer and nothing else, and SHALL stay empty on exit 1 and 2, `--json` included. Errors SHALL go to stderr in plain English, naming the file and what was expected where there is a file, with a `help:` line when the user can act on it.

#### Scenario: A failure leaves stdout empty
- **WHEN** `gsttt --json --no-refresh dependents src/a.ts` runs in a repository that has no index
- **THEN** stdout is empty, stderr says there is no index yet and that `gsttt index` builds one, and the exit code is 1

### Requirement: Logs and colour

Logs SHALL go to stderr, filtered by `GSTTT_LOG` and showing errors only by default. Colour SHALL be used only on a terminal with `NO_COLOR` unset.

#### Scenario: Logs stay off stdout
- **WHEN** `gsttt --json index` runs with `GSTTT_LOG=debug`
- **THEN** stdout parses as one JSON document and stderr holds log lines

#### Scenario: Output to a pipe has no colour
- **WHEN** `gsttt coverage` runs with both streams connected to pipes
- **THEN** neither stream holds an escape sequence

### Requirement: The root

The root SHALL be found from the current directory, or from the directory `-C` names, by `Workspace::locate`, without starting `git`. Outside git the directory itself SHALL be the root and the state SHALL live in the user's cache. A root that is the home directory or `/` SHALL be refused with exit 2 before anything is read, because indexing it means reading every file the user owns.

#### Scenario: -C works from another directory
- **WHEN** `gsttt -C <repo>/src index` runs from a directory outside the repository
- **THEN** the snapshot belongs to `<repo>` and the exit code is 0

#### Scenario: A missing -C directory exits 2
- **WHEN** `gsttt -C does/not/exist status` runs
- **THEN** stderr names `does/not/exist` and the exit code is 2

#### Scenario: The home directory is refused
- **WHEN** `gsttt index` runs in the home directory and the home directory is in no git repository
- **THEN** stderr says gsttt does not index the home directory and suggests running it inside a project, no state is written, and the exit code is 2

#### Scenario: A directory outside git is indexed
- **WHEN** `gsttt index` runs in a directory outside any git repository
- **THEN** the exit code is 0, the snapshot is in the user's cache directory and nothing is written inside the directory

### Requirement: Path arguments

A path argument SHALL be read relative to the current directory after `-C`, normalised by its text alone, with `.` removed and `..` folded, so that a deleted file can still be named, and SHALL then be turned into a path from the root. An absolute path inside the root SHALL be accepted. A path that still leaves the root once its links are resolved SHALL exit 2. The rule SHALL be one function in `gsttt-core`, so that the CLI and the MCP server read paths the same way.

#### Scenario: A path is read from the current directory
- **WHEN** `gsttt dependents ../lib/util.ts` runs from `<repo>/app`
- **THEN** the answer is about `lib/util.ts`

#### Scenario: An absolute path inside the root is accepted
- **WHEN** `gsttt dependents <repo>/lib/util.ts` runs from `<repo>/app`
- **THEN** the answer is about `lib/util.ts`

#### Scenario: A path outside the root exits 2
- **WHEN** `gsttt dependents ../../elsewhere.ts` runs from `<repo>/app`
- **THEN** stderr says the path is outside the repository and the exit code is 2

### Requirement: Paths through a link

A path argument whose text leaves the root SHALL be read again with its longest existing prefix resolved through the filesystem and the rest kept as text, so that a path spelled through a link to a directory of the repository, as `/tmp` is on macOS, is accepted. A link that leads outside the repository SHALL NOT make a path outside it count as inside.

#### Scenario: A path spelled through a linked directory is accepted
- **WHEN** a link names the repository and `gsttt dependents <link>/lib/util.ts` runs, with `-C <link>` or from inside the repository
- **THEN** the answer is about `lib/util.ts`

### Requirement: Refresh before answering

Every question, which is `impact`, `dependents`, `dependencies`, `path`, `contract` and `coverage`, SHALL refresh the index first and answer from the result, so that the answer describes the tree as it is. With `--no-refresh` a question SHALL answer from the stored index, and its footer SHALL say that the tree has changed since. `status` SHALL never refresh. `index` SHALL always refresh.

#### Scenario: A question sees an edit made after the last index
- **WHEN** a file gains an import of `lib/util.ts` after `gsttt index` ran, and `gsttt dependents lib/util.ts` runs
- **THEN** the answer lists the edited file

#### Scenario: --no-refresh answers from the stored index
- **WHEN** a file gains an import of `lib/util.ts` after `gsttt index` ran, and `gsttt --no-refresh dependents lib/util.ts` runs
- **THEN** the answer does not list the edited file, its footer says the index is stale, and the exit code is 0

#### Scenario: status does not refresh
- **WHEN** a file changes after `gsttt index` ran, and `gsttt status` runs twice
- **THEN** both runs say one file changed since the index was built, and the snapshot file is not rewritten

#### Scenario: status without an index exits 1
- **WHEN** `gsttt status` runs in a repository that was never indexed
- **THEN** stderr says there is no index yet and the exit code is 1

### Requirement: Answers

A question SHALL print the text of its answer, with lists cut as the queries cut them. `--all` SHALL print every entry. `--json` SHALL print the answer as one JSON document with every entry, whatever `--all` says. Every answer SHALL end with its coverage and freshness as the queries render them.

#### Scenario: A long list is cut
- **WHEN** `gsttt dependents lib/util.ts` runs and 45 files import it
- **THEN** 30 files are listed, followed by a line saying 15 more were left out

#### Scenario: --all lists every entry
- **WHEN** `gsttt --all dependents lib/util.ts` runs and 45 files import it
- **THEN** all 45 files are listed

#### Scenario: --json prints one document with every entry
- **WHEN** `gsttt --json dependents lib/util.ts` runs and 45 files import it
- **THEN** stdout parses as one JSON object whose `files` holds 45 entries

### Requirement: The index summary

`gsttt index` SHALL refresh the index and print one line with the number of files, the commit, the number of dirty files, the time taken and the numbers of files parsed and reused, followed by the coverage line, for example `Indexed 4,213 files at a1b2c3d (3 dirty) in 1.24 s: 112 parsed, 4,101 reused.` The commit SHALL be left out outside git, and the dirty count when it is 0.

#### Scenario: index reports what it parsed and reused
- **WHEN** `gsttt index` runs twice with one file edited in between
- **THEN** the second run says 1 file was parsed and every other file was reused

### Requirement: An unchanged tree

When nothing has changed since the snapshot, `gsttt index` SHALL print `The index already describes the tree: 4,213 files at a1b2c3d.`, followed by the coverage line.

#### Scenario: index on an unchanged tree says so
- **WHEN** `gsttt index` runs twice with nothing changed in between
- **THEN** the second run says the index already describes the tree

### Requirement: The index summary as JSON

Under `--json`, `gsttt index` SHALL print an object with `files`, `parsed`, `reused`, `unchanged`, `head`, `dirty`, `elapsed_ms` and `coverage`, the last being the coverage line.

#### Scenario: index --json carries the counts
- **WHEN** `gsttt --json index` runs on a tree of three source files
- **THEN** stdout is a JSON object whose `files` is 3 and which has the keys `parsed`, `reused`, `unchanged`, `head`, `dirty`, `elapsed_ms` and `coverage`

### Requirement: The impact command

`gsttt impact` SHALL take either paths or `--changed`, never both. A named path that is not on disk SHALL be passed as deleted. `--changed` SHALL take the files that differ from `HEAD`, or from the best common ancestor of `HEAD` and `<rev>` with `--base <rev>`, as `Engine::changed_files` lists them: modified, added and untracked files as changed, and deleted files as deleted. `--depth N` SHALL limit how far the answer follows dependents.

#### Scenario: --changed includes untracked files
- **WHEN** a new file is created and not added to git, and `gsttt impact --changed` runs
- **THEN** the new file is among the changed files of the answer

#### Scenario: --changed passes a deleted file as deleted
- **WHEN** `lib/util.ts` is deleted after it was committed, and `gsttt impact --changed` runs
- **THEN** the answer lists `lib/util.ts` as deleted

#### Scenario: A named path that is not on disk counts as deleted
- **WHEN** `lib/util.ts` is deleted, and `gsttt impact lib/util.ts` runs
- **THEN** the answer lists `lib/util.ts` as deleted

#### Scenario: --base without --changed exits 2
- **WHEN** `gsttt impact --base main lib/util.ts` runs
- **THEN** stderr says `--base` needs `--changed` and the exit code is 2

#### Scenario: --changed outside git exits 2
- **WHEN** `gsttt impact --changed` runs in a directory outside any git repository
- **THEN** stderr says `--changed` needs a git repository and the exit code is 2

### Requirement: The registry

The binary SHALL register the built-in connectors in this order: `ts`, `python`, `rust`, `openapi`, `graphql`, `proto`, `prisma`, `sql`, `env`, followed by the trusted external connectors in the order the config declares them. The built-in packs SHALL be read once per run.

#### Scenario: config show lists the modules in registry order
- **WHEN** `gsttt config show` runs in a repository with no config
- **THEN** it lists `ts`, `python`, `rust`, `openapi`, `graphql`, `proto`, `prisma`, `sql` and `env` in that order, each with its options

### Requirement: External connectors start only when trusted

An external connector SHALL start only when it is trusted for this root. An untrusted external SHALL not be started, its files SHALL count as unclaimed, and the coverage of every answer SHALL carry a problem that names it, its command and `gsttt config trust`.

#### Scenario: An untrusted external is not started
- **WHEN** the config declares an external whose command creates a marker file, and `gsttt index` runs
- **THEN** the marker does not exist, the exit code is 0, and the coverage names the external and `gsttt config trust`

#### Scenario: Trust belongs to one repository
- **WHEN** an external is trusted in one repository, and a second repository declares the same external with the same script
- **THEN** the second repository does not start it

### Requirement: The trust key

Trust SHALL be recorded under a key that covers the external's name, command, files, version, environment, options and provides, and the content of every element of its command that names a regular file inside the root, so that a change to any of them, from an edit or from a pull, needs trust again.

#### Scenario: Editing the command takes trust away
- **WHEN** a trusted external's arguments change in `.gsttt/config.toml`, and `gsttt index` runs
- **THEN** the external is not started and the coverage names it

#### Scenario: Editing the program takes trust away
- **WHEN** a trusted external runs a script inside the repository, the script changes, and `gsttt index` runs
- **THEN** the external is not started and the coverage names it

### Requirement: The trust store

Trust SHALL be stored in `$XDG_STATE_HOME/gsttt/trust`, or in `~/.local/state/gsttt/trust` when that variable is unset, outside every repository. `gsttt config untrust` SHALL remove every trust recorded for this root.

#### Scenario: Untrust forgets the repository
- **WHEN** `gsttt config untrust` runs in a repository with a trusted external, and `gsttt index` runs
- **THEN** the external is not started

### Requirement: Granting trust

`gsttt config trust` SHALL require stdin and stderr to be terminals, print the name and full command of each untrusted external, and ask once whether to run them whenever gsttt indexes this root. An answer other than `y` or `yes` SHALL trust nothing. Without a terminal it SHALL trust nothing and exit 2, so that a program driving gsttt cannot trust on the user's behalf.

#### Scenario: A trusted external is started
- **WHEN** the user answers `y` to `gsttt config trust` for an external whose command creates a marker file, and `gsttt index` runs
- **THEN** the marker exists

#### Scenario: Trust needs a terminal
- **WHEN** `gsttt config trust` runs with stdin connected to a pipe that holds `y`
- **THEN** stderr says trusting needs a terminal, nothing is trusted and the exit code is 2

#### Scenario: Declining trusts nothing
- **WHEN** the user answers `n` to `gsttt config trust`
- **THEN** nothing is trusted and the exit code is 0

### Requirement: The session-start hook

`gsttt hook session-start` SHALL read the snapshot header alone, start no `git` process, evaluate no config and build no registry. Without a snapshot, or on any error, it SHALL print nothing. It SHALL always exit 0, so that it never disturbs the session that called it.

#### Scenario: The hook is silent without an index
- **WHEN** `gsttt hook session-start` runs in a directory that was never indexed
- **THEN** stdout and stderr are empty and the exit code is 0

#### Scenario: The hook does not read the config
- **WHEN** the repository has a snapshot and `.gsttt/config.toml` holds a syntax error, and `gsttt hook session-start` runs
- **THEN** stdout holds the line and the exit code is 0

### Requirement: The session-start line

With a snapshot the hook SHALL print one line, for example `gsttt: 4,213 files indexed at a1b2c3d (3 dirty), 2 h ago; use the gsttt tools for impact before changing shared code`, without the commit outside git and without the dirty count when it is 0. The age SHALL be `just now` under a minute, then whole minutes (`12 min ago`) under an hour, whole hours (`5 h ago`) under two days, and whole days (`3 days ago`).

#### Scenario: The hook prints one line from the snapshot header
- **WHEN** the snapshot header says 4,213 files at commit `a1b2c3d…` with 3 dirty files, written two hours ago
- **THEN** the line is `gsttt: 4,213 files indexed at a1b2c3d (3 dirty), 2 h ago; use the gsttt tools for impact before changing shared code`

### Requirement: The init command

`gsttt init` SHALL write `.gsttt/config.toml`, the starter config that names the config schema on its first line and the modules whose connectors detect a project in the tree, only when it is missing. It SHALL always write `.gsttt/schema/config.json` and `.gsttt/schema/pack.json`, the schemas generated from the registry, whose `$comment` says that `gsttt init` generated them and that edits to them are lost. It SHALL print each file it wrote and each it kept.

#### Scenario: init writes the starter files
- **WHEN** `gsttt init` runs in a repository with a `package.json`, a `tsconfig.json` and no `.gsttt/`
- **THEN** `.gsttt/config.toml` names the `ts` module, `.gsttt/schema/config.json` and `.gsttt/schema/pack.json` exist, and the exit code is 0

#### Scenario: init keeps the config and rewrites the schemas
- **WHEN** `.gsttt/config.toml` was edited and `.gsttt/schema/config.json` was changed by hand, and `gsttt init` runs
- **THEN** `config.toml` keeps the edit and `schema/config.json` matches what the registry generates

### Requirement: Checking the config

`gsttt config check` SHALL evaluate the config and check each module's options against what the module accepts. When everything is valid it SHALL print `The config is valid.`, followed by a line for each untrusted external, and exit 0. Otherwise it SHALL print every error, with its file, line and column where the config gives them, and exit 1.

#### Scenario: config check names the file and line of an error
- **WHEN** `.gsttt/config.toml` opens a table header on line 3 that it never closes, and `gsttt config check` runs
- **THEN** stderr names `.gsttt/config.toml:3:` and the exit code is 1

### Requirement: Checking module options

An option that a module does not declare, and an option whose value the module cannot read, SHALL be configuration errors that name the module and the option, for `gsttt config check` and for every command that evaluates the config.

#### Scenario: config check rejects an option a module does not declare
- **WHEN** `.gsttt/config.toml` sets `no_such_option = true` in the `[ts]` table, and `gsttt config check` runs
- **THEN** stderr names the `ts` module and `no_such_option`, and the exit code is 1

### Requirement: Showing the config

`gsttt config show` SHALL print the effective setup: each module with whether it is on and its options, the packs, and each external with whether it is trusted. Under `--json` it SHALL print the setup as JSON.

#### Scenario: config show marks an untrusted external
- **WHEN** the config declares an external that is not trusted, and `gsttt config show` runs
- **THEN** the external is listed as not trusted

### Requirement: The external protocol schema

`gsttt schema external` SHALL print the JSON schema of the messages of the external connector protocol.

#### Scenario: schema external prints the protocol schema
- **WHEN** `gsttt schema external` runs
- **THEN** stdout parses as a JSON schema of the external protocol's messages and the exit code is 0

### Requirement: Panics

A panic the engine contains SHALL print nothing, because it reaches the user as a coverage line. A panic that reaches `main` SHALL print one line on stderr naming what failed, where it failed when that is known, and the address to report it at, `https://github.com/SkuratovichA/gsttt/issues`, and the process SHALL exit 101. With `RUST_BACKTRACE` set, the standard panic output SHALL be printed as well.

#### Scenario: A contained panic prints nothing
- **WHEN** a connector panics while parsing one file during `gsttt index`
- **THEN** stderr holds no panic message, the exit code is 0 and the coverage names the file

#### Scenario: An uncontained panic prints one line
- **WHEN** a command panics outside every connector call
- **THEN** stderr holds one line with the panic message and the issue address, and the exit code is 101

### Requirement: gsttt indexes itself

The test suite SHALL index a copy of this repository's committed tree with the built binary, and SHALL fail when a Rust file outside a `tests/fixtures/` directory fails to parse. The CI build SHALL run the packaged binary on each target its runner can execute, `x86_64-unknown-linux-musl` and `aarch64-apple-darwin`, with `--version`, `index` and an `impact` of a file in the checkout.

#### Scenario: The repository indexes without a failed Rust file
- **WHEN** the binary indexes a copy of the repository's committed tree
- **THEN** the exit code is 0 and no Rust file outside `tests/fixtures/` failed to parse

#### Scenario: The release build runs the packaged binary
- **WHEN** the CI workflow is read
- **THEN** the build entries for `x86_64-unknown-linux-musl` and `aarch64-apple-darwin` run the packaged binary with `--version`, `index` and `impact`, and the other build entries do not

### Requirement: Names on the command line
`gsttt impact` SHALL take `--name <NAME>`, which may be repeated, with paths and never with `--changed`, SHALL refuse a blank name, and SHALL ask the query about those names in every path it is given.

#### Scenario: --name asks about a name
- **WHEN** `b.ts` uses `foo` of `a.ts` in `run`, and `gsttt impact a.ts --name foo` runs
- **THEN** the answer lists `b.ts` with the caller `run`

#### Scenario: --name with --changed exits 2
- **WHEN** `gsttt impact --changed --name foo` runs
- **THEN** stderr says `--name` needs paths and the exit code is 2

#### Scenario: A blank --name exits 2
- **WHEN** `gsttt impact a.ts --name ""` runs
- **THEN** stderr says `--name` needs a name, nothing is indexed, and the exit code is 2
