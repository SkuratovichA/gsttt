# Spec Delta

## Purpose

The config is a TOML file, `.gsttt/config.toml`: it turns modules on and off, sets their options and declares connectors that run as a command, and the user's rule packs are TOML files in `.gsttt/packs/`. Both are read into a `Setup` deterministically, so the same files always give the same index, and the crate generates JSON Schemas so that editors complete and check them.

## ADDED Requirements

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
The crate SHALL generate draft-07 JSON Schemas of the config and of a pack file. In the config's, each module's table SHALL be the schema of that module's options, and every table the reader checks for unknown keys SHALL have `additionalProperties` false. Each type SHALL be described once, under `definitions`.

#### Scenario: Options become a schema of the module's table
- **WHEN** the config schema is generated for a registry with a module `ts` whose options have a string `tsconfig` and a list of strings `conditions`, each with a description
- **THEN** `properties.ts`, followed through its reference, describes `tsconfig` as a string and `conditions` as an array of strings, each with its description

#### Scenario: The schema closes the top level
- **WHEN** the config schema is generated
- **THEN** its top level has `additionalProperties` false and a property for every top-level key, for every module of the registry and for `external`

#### Scenario: A pack file has a schema
- **WHEN** the pack schema is generated
- **THEN** it describes `name`, `language`, `detect` and the five kinds of rules, and refuses unknown keys

## MODIFIED Requirements

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
The config and the pack files SHALL be read only from a regular file of at most 1 MiB that, once symbolic links are resolved, lies inside the repository and, for a pack, inside `.gsttt/packs`. Any other file SHALL fail the evaluation at once with a message that names the file and says why, and nothing SHALL wait for a named pipe to be written.

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

## REMOVED Requirements

### Requirement: The gsttt module
**Reason**: The config is TOML data, so there is no module to call.
**Migration**: Each function becomes part of `.gsttt/config.toml`: the options of `setup`, `disable` and `disable_pack` (as `disable_packs`) are top-level keys, a module's call is its table, `external` is an `[[external]]` entry, and `pack` is a file in `.gsttt/packs/`. See "Config keys" and "Pack files".

### Requirement: Local modules
**Reason**: TOML has no `require`, and a pack no longer needs a module to live in.
**Migration**: A pack kept in `.gsttt/lua/` becomes a file in `.gsttt/packs/`, see "Pack files".

### Requirement: Sandbox
**Reason**: Nothing in the config is run, so there is nothing to sandbox.
**Migration**: None.

### Requirement: Strings without patterns
**Reason**: The config has no string library to call.
**Migration**: None.

### Requirement: No addresses
**Reason**: A TOML value has no address that could leak into the setup.
**Migration**: None.

### Requirement: Log output
**Reason**: The config cannot print.
**Migration**: None; a comment in the file says what a print would have said.

### Requirement: Limits
**Reason**: Reading TOML runs no code, so there are no instructions, allocations or time to bound.
**Migration**: The file size limit stays in "Source files", the nesting limit is "Nesting depth", and the 1 KiB cut of a message is in "Config file".

### Requirement: Library calls are bounded
**Reason**: The config calls no library.
**Migration**: None.

### Requirement: A metatable cannot hand work to the collector
**Reason**: A TOML value has no metatable.
**Migration**: None.

### Requirement: Iteration order
**Reason**: The config is never iterated by code it holds.
**Migration**: Modules enter the setup in the registry's order and option keys are sorted, see "Config keys" and "Option tables".

### Requirement: Editor support
**Reason**: Editors check TOML against a JSON Schema, not LuaLS annotations.
**Migration**: The JSON Schemas of "Config schema", named by the starter's `#:schema` line, see "Editor settings".
