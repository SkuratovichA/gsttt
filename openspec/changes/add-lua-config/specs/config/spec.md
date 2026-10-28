# Spec Delta

## Purpose

The config is a Lua file, `.gsttt/init.lua`, in the style of a Neovim config: it turns modules on and off, sets their options, adds and removes rule packs, and declares connectors that run as a command. It is evaluated in a sandbox into a `Setup`, deterministically, so the same files always give the same index.

## ADDED Requirements

### Requirement: Config file
The config SHALL be read from `.gsttt/init.lua` under the repository root. Without that file the setup SHALL turn on every detected module with default options and no packs of the user. A Lua syntax or runtime error SHALL fail the run with the file, the line and Lua's message.

#### Scenario: No config means defaults
- **WHEN** the repository has no `.gsttt/init.lua`
- **THEN** the setup detects modules, lists no module options, packs, disabled modules or externals, and has the default size limit

#### Scenario: An error names its line
- **WHEN** line 3 of `.gsttt/init.lua` calls a nil value
- **THEN** evaluation fails with a message that starts with `.gsttt/init.lua:3:`

### Requirement: The gsttt module
`require("gsttt")` SHALL return a module whose functions fill the setup: `setup` sets `exclude`, `http_prefixes`, `max_file_bytes` and `detect`; a function per module of the registry turns that module on with its table as options; `disable` keeps a module off even when it is detected; `pack` adds a pack read like a built-in one; `disable_pack` leaves a built-in pack out; and `external` declares a connector run as a command with `name`, `cmd`, `files`, `version` and `env`.

#### Scenario: A config fills the setup
- **WHEN** the config calls `g.setup { exclude = { "fixtures/**" }, http_prefixes = { "/api" } }`, `g.ts { tsconfig = "tsconfig.base.json" }`, `g.disable "openapi"`, `g.disable_pack "axios"` and `g.external { name = "kotlin", cmd = { "gsttt-kotlin" }, files = { "**/*.kt" }, version = "1", env = { "JAVA_HOME" } }`
- **THEN** the setup holds each of those values, matching the reviewed snapshot

### Requirement: Strict keys
An unknown key, an unknown module or a malformed pack SHALL fail evaluation with a message naming it, and an unknown module's message SHALL list the known ones.

#### Scenario: A misspelt module is named with the known ones
- **WHEN** the config calls `g.tss {}`
- **THEN** evaluation fails with a message naming `tss` and listing `ts`, `python` and `rust` among the known modules

#### Scenario: A misspelt key is named
- **WHEN** the config calls `g.setup { exlude = { "a" } }`
- **THEN** evaluation fails with a message naming `exlude`

### Requirement: Option tables
Tables passed to the gsttt module SHALL become JSON values: a table whose keys are `1` to `n` becomes a list, an empty table an empty list, and any other table a map.

#### Scenario: Tables become lists and maps
- **WHEN** the config calls `g.ts { conditions = { "import", "types" }, paths = { ["@app/*"] = { "src/*" } }, extra = {} }`
- **THEN** the options are `{"conditions":["import","types"],"paths":{"@app/*":["src/*"]},"extra":[]}`

### Requirement: Local modules
`require` SHALL load `<name>` from `.gsttt/lua/<name with dots as slashes>.lua` or `.gsttt/lua/<name>/init.lua`, evaluate each module once per evaluation, and load nothing from anywhere else; a missing module SHALL fail with the paths it tried, and a name containing `..` or starting with `/` SHALL fail.

#### Scenario: A pack in a local module
- **WHEN** `.gsttt/lua/packs/billing.lua` returns a pack table and the config calls `g.pack(require("packs.billing"))`
- **THEN** the setup holds the pack `billing`

#### Scenario: Requires cannot leave the config directory
- **WHEN** the config calls `require("../secrets")`
- **THEN** evaluation fails with a message naming the module

### Requirement: Source files
The config and its local modules SHALL be read only from a regular file of at most 1 MiB that, once symbolic links are resolved, lies inside the repository (`.gsttt/init.lua`) or inside `.gsttt/lua` (a module). Any other file SHALL fail the evaluation at once with a message that names the file and says why, and nothing SHALL wait for a named pipe to be written.

#### Scenario: A config that links outside the repository is refused
- **WHEN** `.gsttt/init.lua` is a symbolic link to a file outside the repository
- **THEN** evaluation fails with a message that names `.gsttt/init.lua` and says it resolves outside the repository, and the file is not run

#### Scenario: A pipe is not a config
- **WHEN** `.gsttt/init.lua` is a named pipe that nothing writes to
- **THEN** evaluation fails at once with a message that names `.gsttt/init.lua` and says it is not a regular file

#### Scenario: A huge config is refused
- **WHEN** `.gsttt/init.lua` is larger than 1 MiB
- **THEN** evaluation fails with a message that names `.gsttt/init.lua` and the limit

### Requirement: Sandbox
The config, its modules and every pack SHALL be evaluated with only the `table`, `string`, `math` and `utf8` libraries and the base functions that cannot reach outside the state: no `io`, `os`, `package`, `debug`, `load`, `loadfile`, `dofile`, `collectgarbage`, `math.random` or `math.randomseed`.

#### Scenario: The machine is out of reach
- **WHEN** the config calls `io.open("/etc/passwd")`, `os.getenv("HOME")` or `dofile("/etc/passwd")`
- **THEN** evaluation fails, and no file is read

#### Scenario: Built-in packs share the sandbox
- **WHEN** a pack source calls `loadfile("x.lua")`
- **THEN** reading the pack fails with a message naming the pack

### Requirement: Strings without patterns
The `string` library SHALL have no pattern matcher: `string.match`, `string.gmatch` and `string.gsub` are absent, and `string.find` looks only for the text it is given, whatever its fourth argument says.

#### Scenario: Patterns are not available
- **WHEN** the config calls `("abc"):match("a.c")`, `string.gsub("abc", "b", "x")` or `string.gmatch("abc", ".")`
- **THEN** evaluation fails because the function is nil, and the message names it

#### Scenario: Find reads its text as given
- **WHEN** the config evaluates `("a.b"):find(".")`, `("a.b"):find("%.")` and `("axb"):find("a.b", 1, false)`
- **THEN** the first finds positions 2 and 2, the others find nothing, because the text is never a pattern

### Requirement: No addresses
Nothing SHALL show an address, which differs from run to run: `tostring` of a table or a function without a `__tostring` gives its type, and `string.format` refuses `%p`.

#### Scenario: Addresses stay out of a config
- **WHEN** the config shows a table and a function with `tostring`, formats a table with `%s`, or formats one with `%p`
- **THEN** the first three give `table`, `function` and `table`, and `%p` fails with a message that names it

### Requirement: Log output
`print` and `warn` SHALL write to the log, at most a thousand lines in one evaluation and at most 4 KiB in a line, a line without control characters other than tab, and after the thousandth line one more line SHALL say that the rest is dropped.

#### Scenario: Output is bounded
- **WHEN** the config prints five thousand lines
- **THEN** the log holds the first thousand of them and then one line that says the rest is dropped

#### Scenario: A printed line is plain text
- **WHEN** the config prints a line of ten thousand characters, and another with an escape sequence and a newline in it
- **THEN** the first is cut to 4 KiB and ends in `…`, and the second is one line without the control characters

### Requirement: Limits
Evaluation SHALL stop with an error naming the limit when it allocates more than 64 MiB, runs more than 100 million instructions or takes more than two seconds. A table handed to the gsttt module SHALL be refused, with an error that names where in it the limit was reached, when it holds more than a million values, a table that appears twice counting twice. An error message that reaches the caller SHALL be cut at 1 KiB, so that a config cannot make its own text as long as it likes.

#### Scenario: Runaway configs stop
- **WHEN** the config runs `while true do end`, or builds a string of a billion bytes
- **THEN** evaluation fails with a message naming the instruction limit or the memory limit

#### Scenario: A table that expands is refused
- **WHEN** the config builds a table that holds the table below it twice, forty levels deep, and passes it to `g.setup`
- **THEN** evaluation fails with a message that names `g.setup.exclude` and the limit of a million values

#### Scenario: A loop of cheap calls stops at the time limit
- **WHEN** the config loops over a call that is cheap in instructions and dear in time, so that the instruction limit would stop it only after minutes
- **THEN** evaluation fails with a message naming the time limit

#### Scenario: A long error message is cut
- **WHEN** the config raises an error with a message of a hundred thousand characters
- **THEN** the evaluation error is 1 KiB long and ends in `…`

### Requirement: Iteration order
`pairs` SHALL visit list keys in order and then the other keys sorted, so that evaluation is deterministic. `next` SHALL not be available, because it would show the order of the table itself.

#### Scenario: Iteration order is fixed
- **WHEN** the config builds `exclude` by iterating `pairs` over a table with the keys `b`, `a` and `c`
- **THEN** `exclude` lists them as `a`, `b`, `c` on every evaluation

### Requirement: Fingerprint
The setup SHALL have a fingerprint computed from its canonical JSON form, so that an edit that changes no value leaves it unchanged.

#### Scenario: Comments do not change the fingerprint
- **WHEN** a comment is added to `.gsttt/init.lua`
- **THEN** the fingerprint of the setup is unchanged, and changing `http_prefixes` changes it

### Requirement: Editor support
The crate SHALL generate LuaLS annotations from the registry's module names and option schemas: a `---@meta` file declaring the `gsttt` module, one `---@class` per module's options with a `---@field` per property (optional, typed, with its description), string enums as literal unions, lists as `T[]`, and the pack and rule types; a `.luarc.json` pointing LuaLS at those annotations and at `.gsttt/lua/`; and a starter `init.lua` that lists the detected modules as comments.

#### Scenario: Options become annotated classes
- **WHEN** annotations are generated for a module `ts` whose option schema has a string `tsconfig` and a list of strings `conditions`, each with a description
- **THEN** the output declares `---@class gsttt.TsOptions` with `---@field tsconfig? string` and `---@field conditions? string[]` and their descriptions, and a function `ts` that takes `gsttt.TsOptions`

### Requirement: External connectors
An external SHALL be a connector that claims the files its `files` globs match and speaks JSON lines with one process started in the repository root on its first request, so that a run answered from the snapshot starts none.

#### Scenario: An unused external starts nothing
- **WHEN** a run opens an external whose files are all answered from the snapshot, so it parses and resolves nothing
- **THEN** its command never runs

### Requirement: External declaration
A `g.external` SHALL take `provides`, the contract kinds the connector finds with how and what it cannot see, and `options`, a table of named values that is empty when left out. The provides SHALL be known without starting the process, so that coverage does not depend on the cache, and the options SHALL be sent to the process in `open`. Both SHALL be part of the cache key of the external's files.

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
A process that exits, answers out of turn, sends a line that does not parse or takes longer than its timeout for one request SHALL fail every file it has not answered, as a panic naming the connector, and the run SHALL go on. The timeout SHALL be 10 seconds.

#### Scenario: A hung process fails its files and nothing else
- **WHEN** an external's process does not answer the `parse` of `src/B.kt` within the timeout
- **THEN** `src/B.kt` and every later file of that external fail as panics naming it, and the other connectors' files are indexed

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
