# config Specification

## Purpose
The config is a Lua file, `.gsttt/init.lua`, in the style of a Neovim config: it turns modules on and off, sets their options, adds and removes rule packs, and declares connectors that run as a command. It is evaluated in a sandbox into a `Setup`, deterministically, so the same files always give the same index.

## Requirements

### Requirement: Config file
The config SHALL be read from `.gsttt/config.toml` under the repository root. Without that file the setup SHALL turn on every detected module with default options. A TOML syntax error, or a value the setup does not take, SHALL fail the run with a message of the form `<file>:<line>:<column>: <message>`, the line and the column counted from 1 and the column in characters, and the message SHALL be cut at 1 KiB.

#### Scenario: No config means defaults
- **WHEN** the repository has no `.gsttt/config.toml` and no `.gsttt/packs/`
- **THEN** the setup detects modules, lists no module options, packs, disabled modules or externals, and has the default size limit

#### Scenario: An error names its line
- **WHEN** line 3 of `.gsttt/config.toml` sets `max_file_bytes = "big"`
- **THEN** evaluation fails with a message that starts with `.gsttt/config.toml:3:` and names `max_file_bytes`

#### Scenario: A syntax error names its line and column
- **WHEN** line 2 of `.gsttt/config.toml` is `exclude = ["ä" "b"]`, which lacks a comma
- **THEN** evaluation fails with a message that starts with `.gsttt/config.toml:2:` and the column of the error, counted in characters and not in bytes

#### Scenario: A long error is cut
- **WHEN** the top level of `.gsttt/config.toml` has a quoted key of a hundred thousand characters
- **THEN** the evaluation error is 1 KiB long and ends in `…`

### Requirement: Strict keys
An unknown key, an unknown module or a malformed pack SHALL fail evaluation with a message naming it, and an unknown key at the top level SHALL be named with the known modules and the known keys.

#### Scenario: A misspelt module is named with the known ones
- **WHEN** the config has a `[tss]` table
- **THEN** evaluation fails with a message naming `tss` and listing `ts`, `python` and `rust` among the known modules

#### Scenario: A misspelt key is named
- **WHEN** the config sets `exlude = ["a"]`
- **THEN** evaluation fails with a message naming `exlude`

#### Scenario: A misspelt key of an external is named
- **WHEN** an `[[external]]` entry sets `comand` on line 4
- **THEN** evaluation fails with a message that starts with `.gsttt/config.toml:4:` and names `comand`

### Requirement: Option tables
A module's table and an external's `options` SHALL become JSON: an array becomes a list, a table a map with its keys sorted, and a string, an integer, a float or a boolean stays what it is. An empty module table SHALL give no options, like a module turned on without them. A date, a time or a float that is not finite has no JSON form, and nothing in the config or a pack takes one, so it SHALL be refused wherever it appears, with its key and its line.

#### Scenario: Tables become lists and maps
- **WHEN** the config has a `[ts]` table with `paths = { "@app/*" = ["src/*"] }`, `conditions = ["import", "types"]` and `extra = []`
- **THEN** the options are `{"conditions":["import","types"],"extra":[],"paths":{"@app/*":["src/*"]}}`

#### Scenario: A value with no JSON form is refused
- **WHEN** the `[ts]` table sets `since = 1979-05-27`, or `limit = nan`
- **THEN** evaluation fails with a message that names `ts.since`, or `ts.limit`, with its line, and says that the value has no JSON form

### Requirement: Source files
The config and the pack files SHALL be read only from a regular file of at most 1 MiB that, once symbolic links are resolved, lies inside the repository and, for a pack, inside `.gsttt/packs`. Any other file SHALL fail the evaluation at once with a message that names the file and says why, and nothing SHALL wait for a named pipe to be written. A config that is a link to nothing SHALL count as no config, and a pack that is one SHALL fail.

#### Scenario: A config that links outside the repository is refused
- **WHEN** `.gsttt/config.toml` is a symbolic link to a file outside the repository
- **THEN** evaluation fails with a message that names `.gsttt/config.toml` and says it resolves outside the repository, and the file is not read

#### Scenario: A pipe is not a config
- **WHEN** `.gsttt/config.toml` is a named pipe that nothing writes to
- **THEN** evaluation fails at once with a message that names `.gsttt/config.toml` and says it is not a regular file

#### Scenario: A huge config is refused
- **WHEN** `.gsttt/config.toml` is larger than 1 MiB
- **THEN** evaluation fails with a message that names `.gsttt/config.toml` and the limit

#### Scenario: A pack that links outside its folder is refused
- **WHEN** `.gsttt/packs/x.toml` is a symbolic link to `.gsttt/config.toml`
- **THEN** evaluation fails with a message that names `.gsttt/packs/x.toml` and says it resolves outside `.gsttt/packs`

#### Scenario: A pack folder that links outside the repository is refused
- **WHEN** `.gsttt/packs` is a symbolic link to a folder outside the repository that holds `x.toml`
- **THEN** evaluation fails with a message that names `.gsttt/packs/x.toml` and says it resolves outside the repository

#### Scenario: A config that links to nothing is no config
- **WHEN** `.gsttt/config.toml` is a symbolic link to a file that does not exist
- **THEN** evaluation succeeds with the setup of a repository that has no config

#### Scenario: A pack that links to nothing is refused
- **WHEN** `.gsttt/packs/x.toml` is a symbolic link to a file that does not exist
- **THEN** evaluation fails with a message that names `.gsttt/packs/x.toml` and says it is a link that leads nowhere

### Requirement: Fingerprint
The setup SHALL have a fingerprint computed from its canonical JSON form, so that an edit that changes no value leaves it unchanged.

#### Scenario: Comments do not change the fingerprint
- **WHEN** a comment is added to `.gsttt/config.toml`
- **THEN** the fingerprint of the setup is unchanged, and changing `http_prefixes`, an option of a module or an option of an external changes it

#### Scenario: Moving a module's table does not change the fingerprint
- **WHEN** the `[ts]` table moves from the top of `.gsttt/config.toml` to its end, or the keys inside it change places
- **THEN** the fingerprint of the setup is unchanged

### Requirement: Editor settings
The crate SHALL generate a starter `config.toml` whose first line is the directive `#:schema ./schema/config.json`, which TOML editors such as taplo resolve against the folder of the file, so that it names `.gsttt/schema/config.json`. The starter SHALL list the detected modules as commented tables, and SHALL read as the setup of a repository without a config.

#### Scenario: The editor settings are relative to the config folder
- **WHEN** the starter is generated
- **THEN** its first line is `#:schema ./schema/config.json`, a path relative to `.gsttt/`, the folder of the config

#### Scenario: The starter is a valid config
- **WHEN** the starter for a repository where `ts` and `python` are detected is read as `.gsttt/config.toml`
- **THEN** it gives the same setup as no config, and with the `# ` taken off its `[ts]` and `[python]` lines it turns those two modules on with no options

### Requirement: External connectors
An external SHALL be a connector that claims the files its `files` globs match and speaks JSON lines with one process started in the repository root on its first request, so that a run answered from the snapshot starts none.

#### Scenario: An unused external starts nothing
- **WHEN** a run opens an external whose files are all answered from the snapshot, so it parses and resolves nothing
- **THEN** its command never runs

### Requirement: External declaration
An `[[external]]` entry SHALL take `provides`, the contract kinds the connector finds with how and what it cannot see, and `options`, a table of named values that is empty when left out. The provides SHALL be known without starting the process, so that coverage does not depend on the cache, and the options SHALL be sent to the process in `open`. Both SHALL be part of the cache key of the external's files.

#### Scenario: Declared provides need no process
- **WHEN** a config declares an external with `provides` and a run asks the external what it provides
- **THEN** it answers with what the config declared, and its command never runs

#### Scenario: Options reach the process
- **WHEN** a config declares an external with `options` and a request starts its process
- **THEN** the `open` the process reads carries those options as an object with sorted keys

#### Scenario: Options must be named
- **WHEN** the `options` of an external are a list, a number, a string or a boolean
- **THEN** evaluation fails with a message that names the external, its `options` and what they are instead of a table of named values

#### Scenario: A changed declaration drops the cached facts
- **WHEN** the command, the version, the listed variables, the provides or the options of an external change
- **THEN** the cache key of its files changes, and it does not change when only the order of the keys of the options does

### Requirement: External name
The name of an external SHALL be 1 to 64 letters, digits, `-` and `_`. A config that declares an external and a caller that builds its connector SHALL both be refused a name that is not, with a message that holds at most the start of the name.

#### Scenario: A name is bounded
- **WHEN** an external is given a name that is empty, longer than 64 characters or holds anything but letters, digits, `-` and `_`
- **THEN** it is refused with a message that names the external and the rule, and is short however long the name was

### Requirement: External environment
An external's process SHALL get only the variables `PATH`, `HOME`, `USER`, `LOGNAME`, `LANG`, `LC_ALL`, `LC_CTYPE` and `TMPDIR` of gsttt's own environment, when gsttt has them, and the ones named in the `env` of its declaration; the rest of the environment, where secrets live, SHALL stay out of its reach.

#### Scenario: A process sees only the environment it was given
- **WHEN** gsttt runs with `PATH`, a variable listed in `env` and a variable that is neither inherited nor listed, and an external's process writes its environment
- **THEN** the environment holds `PATH` and the listed variable and not the other one

### Requirement: External protocol
An external's process SHALL receive `open` with the protocol version, the root and the `options` of its declaration, and answer `ready` with its name and version; then `parse` for each file with its path and text, answered by `facts`, and `resolve` for each import, answered by a `resolution` that repeats the `from` and the `import` it answers; and `done` at the end of the run. Messages SHALL carry the serde forms of the core types and name files by path.

#### Scenario: A well-behaved process indexes its files
- **WHEN** an external claims `**/*.kt` and its process answers every request
- **THEN** the files `src/A.kt` and `src/B.kt` carry the facts it sent and the imports resolve as it said

### Requirement: External output
The lines an external's process writes to its standard error SHALL go to the log as warnings that name the connector, each line cut to 4 KiB and without control characters other than tab, so that the process cannot flood the log or forge a line in it.

#### Scenario: A noisy process cannot flood or forge the log
- **WHEN** an external's process writes to its standard error a line of a million characters, a line with an escape sequence and a carriage return, and an ordinary line
- **THEN** the log holds three warnings naming the connector: the first cut to 4 KiB and ending in `…`, the second without its control characters, the third as it was written

### Requirement: External failures
A process that exits, answers out of turn, sends a line that does not parse or takes longer than its timeout for one request SHALL fail every file it has not answered, as a panic naming the connector, and the run SHALL go on. An import of a file parsed before the stop SHALL be unresolved with the reason `unsupported` and the failure message as its detail. The timeout SHALL be 10 seconds.

#### Scenario: A hung process fails its files and nothing else
- **WHEN** an external's process does not answer the `parse` of `src/B.kt` within the timeout
- **THEN** `src/B.kt` and every later file of that external fail as panics naming it, and the other connectors' files are indexed

#### Scenario: An import resolved after a stop is unresolved
- **WHEN** an external's process exits while it parses `src/B.kt`, after it parsed `src/A.kt` with an import
- **THEN** the import of `src/A.kt` is unresolved with the reason `unsupported` and the message that failed `src/B.kt` as its detail, and no `resolve` reaches the process

#### Scenario: A resolution of another import is out of turn
- **WHEN** an external's process answers the `resolve` of one import with a `resolution` that repeats another file or another import
- **THEN** the import is unresolved with the reason `unsupported` and a detail that names the connector and the import that was due, and no further request reaches the process

### Requirement: External process group
An external's process SHALL run in a process group of its own, and whenever gsttt ends it (after a failure, a timeout, or `done`) it SHALL kill the whole group, so that a wrapper such as `sh -c`, `npx` or `gradle` leaves no child that holds its pipes or outlives gsttt.

#### Scenario: A killed process takes its children with it
- **WHEN** an external's process starts a child that holds its pipes and then does not answer a request within the timeout
- **THEN** after the process is killed the child no longer runs

#### Scenario: A process that leaves takes its children with it
- **WHEN** an external's process starts a child and exits on `done`
- **THEN** once the run has ended the child no longer runs

### Requirement: External schema
The crate SHALL provide the JSON schema of the messages.

#### Scenario: The protocol has a schema
- **WHEN** the schema of the external protocol is generated
- **THEN** it describes each message type, matching the reviewed snapshot

### Requirement: Config keys
The top level of the config SHALL hold `exclude`, `http_prefixes`, `max_file_bytes`, `detect`, `disable` and `disable_packs`, one table per module of the registry, which turns that module on with the table as its options, and `[[external]]` entries. Modules SHALL enter the setup in the registry's order and externals in the file's order, and `disable` SHALL name known modules.

#### Scenario: The keys fill the setup
- **WHEN** the config sets `exclude = ["fixtures/**"]`, `http_prefixes = ["/api"]`, `disable = ["openapi"]` and `disable_packs = ["axios"]`, has a `[ts]` table with `tsconfig = "tsconfig.base.json"`, and an `[[external]]` named `kotlin` with `cmd = ["gsttt-kotlin"]`, `files = ["**/*.kt"]`, `version = "1"` and `env = ["JAVA_HOME"]`
- **THEN** the setup holds each of those values, matching the reviewed snapshot

#### Scenario: An empty table turns a module on
- **WHEN** the config sets `detect = false` and holds an empty `[python]` table
- **THEN** the setup runs `python` with no options and runs no other module

#### Scenario: The order of the tables is not the order of the modules
- **WHEN** the config has a `[rust]` table before a `[ts]` table
- **THEN** the setup lists `ts` before `rust`, in the order of the registry

#### Scenario: Disable names a known module
- **WHEN** the config sets `disable = ["tss"]`
- **THEN** evaluation fails with a message naming `tss` and listing the known modules

### Requirement: Pack files
Each file in `.gsttt/packs/` whose name ends in `.toml` and does not start with `.` SHALL add one pack, in the order of the file names, and other files SHALL be skipped. A pack's `name` SHALL equal its file name without `.toml`. The built-in packs of a connector SHALL be read by the same reader. A misspelt key of a pack or of a rule SHALL fail with the file, the line and the key.

#### Scenario: A pack file adds a pack
- **WHEN** `.gsttt/packs/ktor.toml` holds a pack named `ktor` for `rust` with one call rule
- **THEN** the setup's packs hold that pack

#### Scenario: A pack is named after its file
- **WHEN** `.gsttt/packs/ktor.toml` sets `name = "spring"`
- **THEN** evaluation fails with a message that names `.gsttt/packs/ktor.toml`, `spring` and `ktor`

#### Scenario: Only pack files are read
- **WHEN** `.gsttt/packs/` holds `b.toml`, `a.toml`, `.draft.toml` and `notes.md`
- **THEN** the setup's packs are those of `a.toml` and then `b.toml`, and nothing else is read

#### Scenario: A misspelt rule key is named with its line
- **WHEN** line 6 of `.gsttt/packs/ktor.toml` sets `calee` in a call rule
- **THEN** evaluation fails with a message that starts with `.gsttt/packs/ktor.toml:6:` and names `calee`

### Requirement: Nesting depth
A config or pack file whose tables and arrays nest deeper than 64 SHALL be refused before it is parsed into values, with the file, the line and the column where the limit is passed. Each part of a table header SHALL count one, and one more when it names an array of tables, each part of a dotted key but the last one, and each inline table or array one, so that no file can exhaust the stack of the thread that reads it.

#### Scenario: Deep nesting is refused
- **WHEN** `.gsttt/config.toml` nests inline arrays a hundred thousand deep, or nests eighty inline tables whose keys have eighty dotted parts each, and it is read on a thread with a 256 KiB stack
- **THEN** evaluation fails with a message that names the limit of 64 and the line and column where it is passed

#### Scenario: Nesting at the limit is read
- **WHEN** a `[ts]` table sets an option to arrays nested 63 deep, 64 levels in all
- **THEN** the config is read and the option holds the nested arrays

#### Scenario: Arrays of tables count twice
- **WHEN** `.gsttt/config.toml` holds the headers `[[ts.a]]`, `[[ts.a.b]]` and so on, each one part longer than the last, down to a header of 40 parts
- **THEN** evaluation fails with a message that names the limit of 64, although no header has more than 40 parts

### Requirement: Config schema
The crate SHALL generate draft-07 JSON Schemas of the config and of a pack file. In the config's, each module's table SHALL be the schema of that module's options, and every table the reader checks for unknown keys SHALL have `additionalProperties` false. Each type SHALL be described once, under `definitions`. No schema SHALL offer `null`, which a TOML file cannot hold, and the table of a module without options SHALL take no keys.

#### Scenario: Options become a schema of the module's table
- **WHEN** the config schema is generated for a registry with a module `ts` whose options have a string `tsconfig` and a list of strings `conditions`, each with a description
- **THEN** `properties.ts`, followed through its reference, describes `tsconfig` as a string and `conditions` as an array of strings, each with its description

#### Scenario: The schema closes the top level
- **WHEN** the config schema is generated
- **THEN** its top level has `additionalProperties` false and a property for every top-level key, for every module of the registry and for `external`

#### Scenario: A pack file has a schema
- **WHEN** the pack schema is generated
- **THEN** it describes `name`, `language`, `detect` and the five kinds of rules, and refuses unknown keys

#### Scenario: No schema offers null
- **WHEN** a module's options have a field that may be left out
- **THEN** the config schema neither allows `null` for that field nor gives it a `null` default

#### Scenario: A module without options takes an empty table
- **WHEN** a module has no options
- **THEN** its table in the config schema is an object that takes no keys
