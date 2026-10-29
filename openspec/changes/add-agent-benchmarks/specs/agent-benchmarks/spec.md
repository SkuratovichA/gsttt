# Spec Delta

## Purpose

Agent benchmarks are the comparison of the README and the site of what Claude Code spends and how right it is when it answers questions about how the code of a pinned repository is connected: alone, with a TypeScript language server, with codebase-memory-mcp and with the gsttt plugin. The questions and their answers are checked in, and anyone with a logged-in Claude Code can rerun every number from the repository.

## ADDED Requirements

### Requirement: Pinned agent runs
`bench/bench.toml` SHALL pin the agent runs in an `agents` table: the model, the repeats, the seed, the wall-clock limit of a run in seconds and its limit of turns. A repository the questions come from SHALL name the directory they are about, its tsconfig, the number of questions of each kind, and the commands that install it, build it where its tests need a build, and run its tests. typescript-language-server SHALL be pinned at a version.

#### Scenario: The agent runs are pinned
- **WHEN** `bench/bench.toml` is read
- **THEN** the `agents` table has a model, at least one repeat, a seed, a limit of at least 60 seconds and a limit of turns, and typescript-language-server has a version

#### Scenario: Questions about tests need a test command
- **WHEN** a repository asks for questions of the kind `tests` and names no test command
- **THEN** reading the pins fails, naming the repository

### Requirement: Questions about users
A question of the kind `users` SHALL name a function that a file of the scope exports, and its answer SHALL be every other file of the scope in which the TypeScript language service, over a program of every tracked TypeScript file of the scope with the compiler options of the pinned tsconfig, finds a reference to it. A function SHALL be drawn only when 3 to 30 files make its answer. The prompt SHALL ask for every other TypeScript file, since no other file can be in the answer.

#### Scenario: A function with too few or too many users is not drawn
- **WHEN** the candidates are a function referred to by 2 files, one referred to by 31 and one referred to by 5
- **THEN** only the one referred to by 5 files becomes a question

#### Scenario: The prompt asks for TypeScript files
- **WHEN** the prompt of a question about users is written
- **THEN** it asks for every other TypeScript file that refers to the function by name, and not for every other file

### Requirement: Questions about tests
A question of the kind `tests` SHALL name a function with a block body, not a generator, that a file of the scope exports, and its answer SHALL be the test files that fail in both of two runs of the pinned test command with a `throw` as the first statement of that body and pass in both of two runs without it. A function SHALL be drawn only when 1 to 30 test files make its answer.

#### Scenario: Only the files the throw breaks are the answer
- **WHEN** with the throw `a.spec.ts` fails in both runs and `b.spec.ts` in one, and `c.spec.ts` fails with the throw and in one of the two runs without it
- **THEN** the answer is `a.spec.ts` alone

### Requirement: A build before every run of the tests
When a repository pins a build command, `cargo xtask agents questions` SHALL run it from the tree as it is right before each of the four runs of the pinned test command, those with a `throw` included, so that the tests read a build that holds it, and again after each file is restored. A build that fails with a `throw` in place SHALL pass the function over, and one that fails without it SHALL stop the draw with an error that names its log.

#### Scenario: The tests see the throw through the build output
- **WHEN** a test file reads a build of a function and the function is made to throw
- **THEN** the test file fails in both runs with the throw and is in the answer, and once the answer is back the build holds the function as git holds it

#### Scenario: A build that fails with the throw passes the function over
- **WHEN** the build fails while the function throws
- **THEN** the function is passed over, no test run follows that build, the file is as git holds it and the build is made again from it

#### Scenario: A build that fails without a throw stops the draw
- **WHEN** the build fails before a run of the tests without a throw
- **THEN** the draw fails with an error that names the log of the build

### Requirement: A run that outlives its limit
A run of the pinned test command or of the build that outlives its limit while a `throw` is in place SHALL be ended with the processes it started, and the function SHALL be passed over with the reason `a run outlived its limit`, and not with `a run wrote no report`, which says that the run ended and left none.

#### Scenario: A run that outlives its limit is passed over for that
- **WHEN** a run of the test command with the throw outlives its limit
- **THEN** the function is passed over with the reason `a run outlived its limit`, and the file is as git holds it

### Requirement: Test files that are neither right nor wrong
A question about tests SHALL keep as ignored the test files that fail in a run of the pinned test command without a throw, and the test files that pass in both runs without it and fail in only one of the two runs with it, and an answer SHALL be scored as if it did not list them. No test file SHALL be in the answer and ignored at once, and questions that hold one are refused.

#### Scenario: A file the throw breaks in one run only is ignored
- **WHEN** with the throw `a.spec.ts` fails in both runs, `b.spec.ts` in the first run only and `c.spec.ts` in the second run only, and all three pass without it
- **THEN** the answer is `a.spec.ts` alone and the question ignores `b.spec.ts` and `c.spec.ts`

#### Scenario: A file is never right and ignored at once
- **WHEN** a question about tests lists a test file in its answer and in its ignored files
- **THEN** the questions are refused when they are written or read, with an error that names the question and the file

### Requirement: The prompt about tests
The prompt of a question about tests SHALL say that the function fails every time it is called, by throwing an error or, for an async function, by returning a promise that rejects with one, and SHALL ask which test files of the unit tests that the pinned command runs would fail.

#### Scenario: The prompt says how the function fails
- **WHEN** the prompt of a question about tests is written
- **THEN** it says the function failed every time it was called, by throwing an error or, if it is async, by returning a promise that rejects with one

### Requirement: Drawing the questions
`cargo xtask agents questions` SHALL take the candidates of each repository and kind in the order of the SHA-256 of the seed, the repository, the file and the name, until it has as many questions as the pins ask for, and SHALL write them to `bench/agents/questions.json`, each with an id, the repository, the kind, the file, the name, the prompt and the answer in byte order. It SHALL leave the checkout as it found it.

#### Scenario: The same pins draw the same questions
- **WHEN** questions are drawn twice from the same candidates with the same seed
- **THEN** both files are the same byte for byte

#### Scenario: Candidates are taken in the order of their hash
- **WHEN** twelve candidates are ordered with a seed
- **THEN** they come in the order of the SHA-256 of the seed, the repository, the file and the name, the lowest first

#### Scenario: A throw leaves no trace in the checkout
- **WHEN** a function was made to throw and the tests have run
- **THEN** `git status --porcelain` of the checkout is what it was before

#### Scenario: A throw is taken back when the run fails
- **WHEN** a function was made to throw and the run then ended in an error or a panic
- **THEN** the file is as git holds it, and `git status --porcelain` of the checkout is what it was before

### Requirement: A test command the draw can read
`cargo xtask agents questions` SHALL stop with an error that names the report when a run of the pinned test command writes one that is not Vitest's JSON, and with an error that names the log when a run without a throw reports no test file.

#### Scenario: A report that cannot be read stops the draw
- **WHEN** a run of the test command writes a report that is not JSON
- **THEN** the draw fails with an error that names the report, and the checkout is as it was

#### Scenario: A run without a throw that reports no test file stops the draw
- **WHEN** a run of the test command without a throw reports no test file
- **THEN** the draw fails with an error that names the log of the run

### Requirement: The setups
`cargo xtask agents run` SHALL ask every question in four setups that all have the tools Bash, Read, ToolSearch and Skill: `plain` with nothing more, `lsp` with the LSP tool backed by the pinned typescript-language-server, `cbm` with the tools of the pinned codebase-memory-mcp over its index of the checkout, and `gsttt` with the plugin of this repository running the binary under test over its index of the checkout.

#### Scenario: Each setup adds only its own tool
- **WHEN** the commands of the four setups are planned
- **THEN** each allows the four base tools, only `lsp` allows the LSP tool and loads a language server, only `cbm` loads an MCP server of its own, and only `gsttt` loads the gsttt plugin

### Requirement: The tools a setup adds
A tool SHALL be one a setup adds when it is `LSP`, by its whole name, for `lsp`, or when its name starts with the prefix of the MCP server of `cbm` or `gsttt`, which ends in `__`; `plain` adds none. The init check and the table of the agent runs SHALL both tell the tools of a setup from the others by this rule.

#### Scenario: A name that only begins like a tool of a setup is not one of its tools
- **WHEN** a setup is asked whether it adds `LSPx`, or a tool of a server whose name begins like its own
- **THEN** it does not, and it does add `LSP` or a tool of its own server, as its setup has them

### Requirement: The arms of a setup
Every setup but `plain` SHALL be asked in two arms: `installed`, as the setup is, and `told`, with the same command line and the same environment and one `--append-system-prompt` more, whose sentence names the tools of the setup by the prefix of their names. `plain` has no told arm. The task SHALL check the init event of a told run as any other.

#### Scenario: A told run is its installed run with one sentence more
- **WHEN** the commands of `lsp`, `cbm` and `gsttt` are planned in both arms
- **THEN** each told command is its installed command with `--append-system-prompt` and a sentence added, and its environment is the same

#### Scenario: The sentence names the tools of the setup
- **WHEN** the told commands of the three setups are planned
- **THEN** the sentence of `gsttt` is "The tools whose names start with `mcp__plugin_gsttt_gsttt__` answer questions about the code of this repository: load them with ToolSearch, and use them before you search or read files yourself.", and those of `lsp` and `cbm` differ in the prefix alone, `LSP` and `mcp__codebase-memory-mcp__`

#### Scenario: A setup that adds no tools is asked as installed only
- **WHEN** the `plain` setup is planned in the told arm
- **THEN** the command is refused, and `plain` is planned as installed alone

#### Scenario: The init event of a told run is checked like any other
- **WHEN** the init event of a told run lists an MCP server beyond its setup's
- **THEN** the task fails naming the setup and what was loaded, and keeps the runs before it

### Requirement: Isolation of a run
A run SHALL load none of the settings, hooks, memory, plugins, MCP servers or CLAUDE.md files of the machine, SHALL run in a checkout outside every directory that holds a CLAUDE.md or an AGENTS.md, and SHALL find `gsttt` on its PATH in the `gsttt` setup only. The task SHALL check the init event of every run, plugin names and sources included, against its setup and stop at the first run that loaded anything else, naming what it loaded.

#### Scenario: A run that loaded something else stops the task
- **WHEN** the init event of a `plain` run lists an MCP server or a tool beyond its own
- **THEN** the task fails naming the setup and what was loaded

#### Scenario: gsttt is out of reach of the other setups
- **WHEN** the PATH of a `plain`, `lsp` or `cbm` run is planned
- **THEN** none of its directories holds a program named `gsttt`

#### Scenario: Claude Code and the tools are out of reach of every setup
- **WHEN** a directory of the PATH of a run holds a program named `claude`, `codebase-memory-mcp`, `typescript-language-server` or `tsc`
- **THEN** the run is refused before it starts, naming the setup and the program

#### Scenario: A plugin from another source stops the task
- **WHEN** the init event of a run lists a plugin with the name of the plugin of its setup from another source than the one the task gave it
- **THEN** the task fails naming the setup and the plugin

### Requirement: The init event of a run
A run SHALL write its init event whatever becomes of its process: one that exited, was stopped at its limit or was ended after its result without having written one is an error that stops the task, naming the transcript, the time the run took and the tail of its standard error, and is never recorded as a timeout.

#### Scenario: A run stopped at its limit with no init event is an error
- **WHEN** a run is stopped at its limit before it wrote an init event
- **THEN** the task fails naming its transcript and the time it took, and records no run

### Requirement: What a run can reach
Every run SHALL pass `--settings` with a sandbox that makes the run fail at start when it is unavailable, lets the Bash tool write only in the checkout and in a temporary directory of the run, and allows it no domain of the network. The files of the setups SHALL be unreadable to the Bash tool by the sandbox and to the Read tool by `Read` rules, and so SHALL the binary under test in every setup but `gsttt`.

#### Scenario: A run is confined by the sandbox
- **WHEN** the command line of a run is planned in each of the four setups
- **THEN** it carries the sandbox settings, only the checkout and the temporary directory of the run are writable, no domain of the network is allowed, and each denied path is both a denied read of the sandbox and two `Read` rules, one for the path and one for everything below it

### Requirement: The index of gsttt is private to its setup
A run of a setup but `gsttt` SHALL NOT read `.git/gsttt` of its checkout, where the index of gsttt is built: the sandbox denies it to the Bash tool and a `Read` rule to the Read tool.

#### Scenario: The index of gsttt is out of reach of the other setups
- **WHEN** the command line of a run is planned in each of the four setups
- **THEN** `.git/gsttt` of the checkout is denied to the sandbox and to the Read tool in `plain`, `lsp` and `cbm`, and is not denied in `gsttt`

### Requirement: Every worktree of the repository is out of reach
No run SHALL read this repository or any worktree of it, the main one or a linked one, wherever it was made, since each holds the sources of the tool under test and the questions with their answers. The worktrees SHALL be those that `git worktree list` gives.

#### Scenario: Every worktree of the repository is out of reach of the runs
- **WHEN** the directories no run may read are planned from a linked worktree, from the main worktree and from another linked worktree made outside the directory of the repository
- **THEN** each list holds its own worktree first and every worktree of the repository once

### Requirement: What a run can read of Claude Code
A run SHALL be denied the state of Claude Code but the project directory of its own checkout, where Claude Code saves what a run reads back: every entry of the home directory named `.claude*` but the active configuration directory, every entry of that directory but `projects`, every entry of `projects` but the project directory, and every entry in it when the run starts. `/tmp/claude-<uid>` SHALL be denied the same way, and all of it listed before every run.

#### Scenario: A run is denied the state of Claude Code but its own project directory
- **WHEN** the command line of a run is planned over a home directory with several `.claude*` entries, a configuration directory with other projects and a project directory of the checkout that holds an earlier run, and a temporary root of the same shape
- **THEN** each entry is denied to the sandbox and to the Read tool but the home directory, the configuration directory, `projects`, the root and the project directory of the checkout, and so is each entry already in that project directory

#### Scenario: The state of Claude Code is listed before every run
- **WHEN** an entry is made in one of those directories after one run is planned and before the next
- **THEN** the next run is denied it

#### Scenario: The project directory is named after the canonical path of its checkout
- **WHEN** the project directory of the checkout `/private/var/folders/8b/t787z9xj78lggd7wng56cxsh0000gn/T/gsttt-agents-runs/vite` is named
- **THEN** it is `-private-var-folders-8b-t787z9xj78lggd7wng56cxsh0000gn-T-gsttt-agents-runs-vite`, every character that is not an ASCII letter or digit being a dash

### Requirement: The shell of a run
A run SHALL get `SHELL=/bin/bash` whatever the shell of the task is, since zsh fails a command whose glob matches nothing and carries the aliases of the user into the run.

#### Scenario: A run gets bash whatever the shell of the host is
- **WHEN** a run is planned on a host whose `SHELL` is `/bin/zsh`, on one whose `SHELL` is another shell and on one that sets none
- **THEN** its environment holds `SHELL` once, as `/bin/bash`

### Requirement: A denied path is a plain path
A path denied to a run SHALL be UTF-8 and hold none of `*`, `?`, `[`, `]`, `{` and `}`, by its own name or its canonical one, since a `Read` rule would take the character for a pattern and match other paths than the one. The task SHALL stop before the run and name the path.

#### Scenario: A path that a rule of the Read tool would take for a pattern stops the task
- **WHEN** a path to deny holds one of those characters, or is not UTF-8, whether it was given or listed from a directory
- **THEN** no command line is planned and the error names the path, saying for a character of a pattern that a `Read` rule would take it for one

### Requirement: One task at a time
`cargo xtask agents questions` and `cargo xtask agents run` SHALL take an exclusive lock on one file in the temporary directory of the system before they do anything else and hold it to their end. A second one SHALL be refused with a message that names the file, because the directories they empty and the processes they end are shared. `table` SHALL take none.

#### Scenario: A second task is refused while one holds the lock
- **WHEN** a task holds the lock and another is started
- **THEN** the second is refused with a message that names the lock file and says that another task holds it, and a task started once the first has let go is not refused

### Requirement: The directories of a run
A run SHALL have a temporary directory of its own, made new for it and removed when it ends, and SHALL find `node` in a directory that holds nothing else. When a run ends, whatever became of it, the directory Claude Code made for its session in the temporary root that every session of the user shares SHALL be removed, and nothing else in that root.

#### Scenario: A run has a temporary directory of its own
- **WHEN** runs are made, one that answers, one that fails and one that a signal stops
- **THEN** each is given a new directory outside its checkout, and every one is gone when its run is over

#### Scenario: The directory of node holds node alone
- **WHEN** the PATH of a run is planned
- **THEN** the directory it finds `node` in holds a link to `node` and nothing else

#### Scenario: The session directory of a run is removed
- **WHEN** runs are made while the temporary root also holds the directory of another session and a file
- **THEN** the directory of the session of each run is gone when the run is over, and the other session and the file are as they were

#### Scenario: The session directory of a run the task stops on is removed
- **WHEN** the init event of a run lists something the setup must not have, and the task stops on it
- **THEN** the directory of the session of that run is gone as well

### Requirement: The checkout of the runs
Each repository SHALL have one checkout for the runs, prepared once: cloned at its commit, installed with its install command, not built, and indexed by gsttt and by codebase-memory-mcp. A checkout whose `node_modules` is gone SHALL be installed again. The binary under test and the files of the setups SHALL be kept outside the directory of the checkouts.

#### Scenario: The binary and the files of the setups are outside the checkouts
- **WHEN** the directories the task keeps for itself are planned
- **THEN** none of them is inside another or inside the directory of the checkouts

#### Scenario: The install is made again when node_modules is gone
- **WHEN** a prepared checkout loses its `node_modules` and is prepared again
- **THEN** the install command runs again, and it did not run the time before, when `node_modules` and the record of the preparation were there

### Requirement: What a run leaves in the checkout
After every run, whatever became of it, HEAD and its form (a branch, or a detached commit) SHALL be put back, the tracked files reset, and the untracked and ignored files compared one by one, and only the files the run added SHALL be removed. A run that changed any of this SHALL be marked. The task SHALL stop when the checkout cannot be put back as it was, and SHALL NOT clean the whole tree.

#### Scenario: A run that writes into the checkout is marked and undone
- **WHEN** a run leaves a new file in the checkout
- **THEN** the run is marked as one that changed the checkout, and the file is gone before the next run

#### Scenario: A run that moves HEAD is marked and HEAD is put back
- **WHEN** a run moves HEAD to another commit, makes a branch, or resets the branch HEAD was on
- **THEN** the run is marked, and HEAD is on the commit and the branch, or detached, as it was

#### Scenario: A file that was there before the run is not removed with the files the run added
- **WHEN** a run adds a file in a directory that already held ignored and untracked files
- **THEN** the file is removed, and every file that was there before the run is still there

#### Scenario: A file that a run deleted stops the task
- **WHEN** a run deletes a file the preparation left, which git cannot restore
- **THEN** the task fails naming the checkout

### Requirement: The indexes of the runs
The index of gsttt in each checkout and the cache of codebase-memory-mcp SHALL be kept aside when they are made. After a run of the setup that reads one, the index SHALL be put back as it was when it differs, and the run SHALL be marked. What the tool writes into its index on its own SHALL NOT count as a change. A copy that is missing for the setup that reads an index SHALL be an error.

#### Scenario: A run that changes an index is marked and the index is put back
- **WHEN** a run of the `gsttt` setup changes the index of its checkout, or a run of the `cbm` setup the cache of codebase-memory-mcp
- **THEN** the run is marked, and the index holds the files and the bytes it held before the run

#### Scenario: What a tool writes into its index on its own is not a change
- **WHEN** the server of codebase-memory-mcp writes its activity database and its logs while it answers
- **THEN** the run is not marked, and those files are left as they are

#### Scenario: A missing copy of an index is an error
- **WHEN** a run of the `gsttt` or the `cbm` setup ends and no copy of its index was kept
- **THEN** putting the index back fails with an error that names the repository and the setup, while the `plain` and `lsp` setups, which have no index, are not asked for a copy

### Requirement: The daemon of codebase-memory-mcp
The daemon of codebase-memory-mcp keeps its index open, so it SHALL be stopped, in the environment the tool runs with, before its cache is thrown away and before the cache is put back. A stop that fails SHALL stop the task.

#### Scenario: The daemon is stopped before its cache is thrown away
- **WHEN** the task throws the cache of codebase-memory-mcp away
- **THEN** `daemon stop` has run with the cache of the task while the cache still held its files

#### Scenario: The daemon is stopped before its cache is put back
- **WHEN** a run of the `cbm` setup ends and the cache is put back
- **THEN** `daemon stop` has run with the cache of the task while the cache still held what the run left in it

### Requirement: Answers and scores
The prompt SHALL ask for the answer as a fenced block marked `files`, one path per line, and the answer SHALL be the last such block of the final message. Paths SHALL be made relative to the checkout and kept once, and one outside the checkout or the question's scope counts as wrong. A run SHALL be scored by precision, recall and F1, and a run without an answer by zeros. A test file the question ignores SHALL count as neither right nor wrong.

#### Scenario: The last files block is the answer
- **WHEN** the final message has two blocks marked `files` and one marked `ts`
- **THEN** the answer is the lines of the second block marked `files`

#### Scenario: Paths in the checkout count as relative
- **WHEN** an answer lists `<checkout>/src/a.ts`, `./src/a.ts` and `src/b.ts`
- **THEN** the answer is `src/a.ts` and `src/b.ts`

#### Scenario: Precision, recall and F1
- **WHEN** the answer is `a.ts`, `b.ts` and `x.ts` and the answer of the question is `a.ts`, `b.ts`, `c.ts` and `d.ts`
- **THEN** precision is 2/3, recall is 1/2 and F1 is 4/7

#### Scenario: A run without an answer scores zero
- **WHEN** the final message has no block marked `files`
- **THEN** precision, recall and F1 are 0

### Requirement: The order and the limits of the runs
The runs SHALL go one at a time, by repeat, then by question, then by setup and arm: seven runs of a question in a repeat. A run SHALL be stopped at the wall-clock limit, scored as without an answer, and keep the tokens of the messages it streamed. A run that ends on the usage limit of the account SHALL stop the task, keeping the runs before it.

#### Scenario: The setups take turns
- **WHEN** two questions are planned in the four setups with two repeats
- **THEN** the first seven runs are the first question in the first repeat in the order `plain`, `lsp`, `lsp` told, `cbm`, `cbm` told, `gsttt` and `gsttt` told, and the last seven are the second question in the second repeat in that order

#### Scenario: A usage limit stops the task
- **WHEN** a run ends with the message that the usage limit is reached
- **THEN** the task fails, and the results hold the runs before it and not that one

### Requirement: The end of a run
A run SHALL end with its result event: a process still alive five seconds after it SHALL be ended, and the run recorded from the result, with a duration that ends at the event. When a run ends, its process group SHALL be killed with every process whose working directory is the checkout or below it, bar those that already worked there when its process was started. Ctrl-C, SIGTERM and SIGHUP SHALL end a run the same way, and stop the task without recording that run.

#### Scenario: A run ends with its result
- **WHEN** a run writes its result event and its process stays alive
- **THEN** the process is ended after five seconds, and the run is recorded from the result and not as a timeout

#### Scenario: The group of a run ends with it
- **WHEN** a run is stopped at the limit and a job it put in the background would write a file after that
- **THEN** the file is never written

#### Scenario: A process out of the group is ended by its directory
- **WHEN** a process of a run leaves the group and the session and keeps the checkout as its working directory
- **THEN** it is ended with the run

#### Scenario: A process that was in the directory before a run is left alone
- **WHEN** a process that is not the run's already works in the checkout when the process of the run is started, and the run ends
- **THEN** that process is still alive, and the process the run put out of its group and session in the checkout is ended

#### Scenario: A signal stops the task
- **WHEN** the task gets SIGINT or SIGTERM while a run is going
- **THEN** the run is ended, the task fails naming the signal, and the results do not hold that run

#### Scenario: A hangup stops the task
- **WHEN** the task gets SIGHUP, as it does when its terminal is closed, while a run is going
- **THEN** the wait for the run ends with an error naming the signal, and the process of the run is ended

### Requirement: One result per stream
The stream of a run SHALL hold one result event. A second one is an error that stops the task, naming the run and its transcript, and the run is not recorded.

#### Scenario: A stream with two results stops the task
- **WHEN** the stream of a run holds two result events
- **THEN** the task fails naming the run and its transcript, and records no run

### Requirement: The directories a run may end processes in
The task SHALL end processes only in directories below one it made itself, and SHALL refuse any other directory before it starts the process of a run.

#### Scenario: A sweep refuses a directory the task did not make
- **WHEN** a sweep is asked for an empty path, `/`, the home directory, a relative path, or a directory outside the ones the task made
- **THEN** it is refused before any process is ended

#### Scenario: A job is refused before its process is started
- **WHEN** a job names a directory the task did not make, beside one it did
- **THEN** the job is refused with an error naming that directory, its process is never started, and no process is ended

### Requirement: A second signal
A second Ctrl-C, SIGTERM or SIGHUP SHALL end the task at once with the default action of the signal, whatever the task is waiting for.

#### Scenario: A second signal ends the task at once
- **WHEN** the task gets a signal it has already caught once
- **THEN** the task is ended by that signal itself, and the handlers are installed once however many runs ask for them

### Requirement: The results of the runs
`cargo xtask agents run` SHALL write `bench/results/agents.json` through a temporary file after every run, with the time, the machine, the Claude Code version, the model, the SHA-256 of the questions, the versions of the tools, and per run its question and kind, setup, arm, repeat, outcome, answer, precision, recall, F1, the four token counts, cost, turns, tool calls by name, tool errors by name, duration and whether it changed the checkout. It SHALL hold no path or user name of the machine.

#### Scenario: The agent results have the published shape
- **WHEN** agent results are written
- **THEN** the file has exactly the published keys in their order, and parses back

#### Scenario: No local path reaches the agent results
- **WHEN** a run answers with a path under the home directory and outside the checkout
- **THEN** the run counts it as wrong, and the results hold neither that path nor the user name

### Requirement: What a run called
A run SHALL record how many times it called each tool, by the name the stream gives it, and how many of those calls failed: every `tool_use` block of an `assistant` event counts once by its `id`, whichever event of its message carries it, and every `tool_result` block with `is_error` true counts once by its `tool_use_id`, under the name of that call. A run that ended without a result SHALL keep both.

#### Scenario: A tool call repeated in the stream counts once
- **WHEN** two events of one message carry the same `tool_use` block
- **THEN** the run counts that call once, under the name of the tool

#### Scenario: A failed tool call counts under the name of its tool
- **WHEN** a run calls `LSP` twice and the result of one call says `is_error`
- **THEN** the run counts one error, under `LSP`

#### Scenario: A failure repeated in the stream counts once
- **WHEN** two events carry the failed `tool_result` block of the same call
- **THEN** the run counts that failure once

#### Scenario: A run without a result keeps its tool calls
- **WHEN** a transcript is cut off before its result
- **THEN** the calls made before the cut are counted, and so are the ones that failed

### Requirement: A run is kept once the results are written
A run SHALL be added to the results and reported as done only once the results with it in them are written. When they cannot be written, the run SHALL not be kept and the task SHALL stop.

#### Scenario: A run whose results cannot be written is not kept
- **WHEN** the results of a run that answered cannot be written
- **THEN** the task fails, the run is neither in the results nor on disk, and no later run is started

### Requirement: Resuming the runs
A task started over SHALL skip the runs whose question, setup, arm and repeat the results already hold, and SHALL refuse to add to results written with another model, another Claude Code version, other questions, other versions of the tools or another build of the binary under test.

#### Scenario: Finished runs are skipped
- **WHEN** the results hold the installed runs of `plain` and of `lsp` and the task starts over
- **THEN** it runs every planned run but those two, the told run of `lsp` among them

#### Scenario: A told run and an installed run of one question are two runs
- **WHEN** the results hold the installed run of a question in a setup
- **THEN** they do not hold its told run, and the file can hold both side by side but not either twice

#### Scenario: Results of another model are refused
- **WHEN** the results were written with another model
- **THEN** the task fails naming both models as soon as the first init event is written, and adds no run

#### Scenario: Results of other questions are refused
- **WHEN** the results were written for other questions
- **THEN** the task fails saying so, and runs nothing

#### Scenario: Results of other versions of the tools are refused
- **WHEN** the results were written with another version of a tool
- **THEN** the task fails saying so, and runs nothing

#### Scenario: Results of another build of the binary are refused
- **WHEN** the results were written with a binary of the same version whose bytes have another SHA-256
- **THEN** the task fails saying so, and runs nothing

### Requirement: The table of the agent runs
`cargo xtask agents table` SHALL print, for each kind of question, a Markdown table with a row per setup and arm: the mean over its runs of recall, precision, F1, tokens, turns, seconds and calls of its own tools, the mean cost over those that report one, and the number of runs, of those without an answer, of those that called an own tool and of those that changed the checkout. Results without a run SHALL print nothing.

#### Scenario: Results without a run print no table
- **WHEN** the results hold no run
- **THEN** nothing is printed, not even an empty line

#### Scenario: The table has a row for each setup and arm
- **WHEN** the results hold runs in `plain`, in `lsp` as installed and told, and in `gsttt` told alone
- **THEN** the table has the rows `plain`, `lsp`, `lsp, told` and `gsttt, told` in that order, and none for `cbm` or `gsttt`

#### Scenario: The table counts the calls of the tools a setup adds
- **WHEN** runs of `lsp` and `gsttt` call `LSP`, `mcp__plugin_gsttt_gsttt__impact`, `Read` and `ToolSearch`
- **THEN** only the `LSP` calls count for `lsp` and only the `impact` calls for `gsttt`, the runs that made one are counted, and `plain` shows a dash

#### Scenario: The table averages the runs of each setup
- **WHEN** a setup has two runs of a kind, with recall 1 and 0.5
- **THEN** its row of that kind shows the recall 0.75 and two runs
