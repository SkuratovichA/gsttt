# Spec Delta

## Purpose

Agent benchmarks are the comparison of the README and the site of what Claude Code spends and how right it is when it answers questions about how the code of a pinned repository is connected: alone, with a TypeScript language server, with codebase-memory-mcp and with the gsttt plugin. The questions and their answers are checked in, and anyone with a logged-in Claude Code can rerun every number from the repository.

## ADDED Requirements

### Requirement: Pinned agent runs
`bench/bench.toml` SHALL pin the agent runs in an `agents` table: the model, the repeats, the seed, the wall-clock limit of a run in seconds and its limit of turns. A repository the questions come from SHALL name the directory they are about, its tsconfig, the number of questions of each kind, and the commands that install it and run its tests. typescript-language-server SHALL be pinned at a version.

#### Scenario: The agent runs are pinned
- **WHEN** `bench/bench.toml` is read
- **THEN** the `agents` table has a model, at least one repeat, a seed, a limit of at least 60 seconds and a limit of turns, and typescript-language-server has a version

#### Scenario: Questions about tests need a test command
- **WHEN** a repository asks for questions of the kind `tests` and names no test command
- **THEN** reading the pins fails, naming the repository

### Requirement: Questions about users
A question of the kind `users` SHALL name a function that a file of the scope exports, and its answer SHALL be every other file of the scope in which the TypeScript language service, over a program of every tracked TypeScript file of the scope with the compiler options of the pinned tsconfig, finds a reference to it. A function SHALL be drawn only when 3 to 30 files make its answer.

#### Scenario: A function with too few or too many users is not drawn
- **WHEN** the candidates are a function referred to by 2 files, one referred to by 31 and one referred to by 5
- **THEN** only the one referred to by 5 files becomes a question

### Requirement: Questions about tests
A question of the kind `tests` SHALL name a function with a body that a file of the scope exports, and its answer SHALL be the test files that fail in both of two runs of the pinned test command with a `throw` as the first statement of that body and pass in both of two runs without it. A function SHALL be drawn only when 1 to 30 test files make its answer.

#### Scenario: Only the files the throw breaks are the answer
- **WHEN** with the throw `a.spec.ts` fails in both runs and `b.spec.ts` in one, and `c.spec.ts` fails with the throw and in one of the two runs without it
- **THEN** the answer is `a.spec.ts` alone

### Requirement: Drawing the questions
`cargo xtask agents questions` SHALL take the candidates of each repository and kind in the order of the SHA-256 of the seed, the repository, the file and the name, until it has as many questions as the pins ask for, and SHALL write them to `bench/agents/questions.json`, each with an id, the repository, the kind, the file, the name, the prompt and the answer in byte order. It SHALL leave the checkout as it found it.

#### Scenario: The same pins draw the same questions
- **WHEN** questions are drawn twice from the same candidates with the same seed
- **THEN** both files are the same byte for byte

#### Scenario: Candidates are taken in the order of their hash
- **WHEN** two candidates are ordered with a seed
- **THEN** the one with the lower SHA-256 of the seed, the repository, the file and the name comes first

#### Scenario: A throw leaves no trace in the checkout
- **WHEN** a function was made to throw and the tests have run
- **THEN** `git status --porcelain` of the checkout is what it was before

### Requirement: The setups
`cargo xtask agents run` SHALL ask every question in four setups that all have the tools Bash, Read, ToolSearch and Skill: `plain` with nothing more, `lsp` with the LSP tool backed by the pinned typescript-language-server, `cbm` with the tools of the pinned codebase-memory-mcp over its index of the checkout, and `gsttt` with the plugin of this repository running the binary under test over its index of the checkout.

#### Scenario: Each setup adds only its own tool
- **WHEN** the commands of the four setups are planned
- **THEN** each allows the four base tools, only `lsp` allows the LSP tool and loads a language server, only `cbm` loads an MCP server of its own, and only `gsttt` loads the gsttt plugin

### Requirement: Isolation of a run
A run SHALL load none of the settings, hooks, memory, plugins, MCP servers or CLAUDE.md files of the machine, SHALL run in a checkout outside every directory that holds a CLAUDE.md, and SHALL find gsttt on its PATH in the `gsttt` setup only. The task SHALL check the init event of every run against its setup and stop at the first run that loaded anything else, naming what it loaded.

#### Scenario: A run that loaded something else stops the task
- **WHEN** the init event of a `plain` run lists an MCP server or a tool beyond its own
- **THEN** the task fails naming the setup and what was loaded

#### Scenario: gsttt is out of reach of the other setups
- **WHEN** the PATH of a `plain`, `lsp` or `cbm` run is planned
- **THEN** none of its directories holds a program named `gsttt`

### Requirement: The checkout of the runs
Each repository SHALL have one checkout for the runs, prepared once: cloned at its commit, installed with its install command, and indexed by gsttt and by codebase-memory-mcp. Every run SHALL leave `git status --porcelain --ignored` of the checkout as it was after the preparation; a run that changed it SHALL be marked, and the checkout restored before the next run.

#### Scenario: A run that writes into the checkout is marked and undone
- **WHEN** a run leaves a new file in the checkout
- **THEN** the run is marked as one that changed the checkout, and the file is gone before the next run

### Requirement: Answers and scores
The prompt SHALL ask for the answer as a fenced block marked `files`, one path per line, and the answer SHALL be the last such block of the final message. Paths SHALL be made relative to the checkout and kept once, and one outside the checkout or the question's scope counts as wrong. A run SHALL be scored by precision, recall and F1, and a run without an answer by zeros. A test file that fails without the throw SHALL count as neither right nor wrong.

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
The runs SHALL go one at a time, by repeat, then by question, then by setup. A run SHALL be stopped at the wall-clock limit, scored as without an answer, and keep the tokens of the messages it streamed. A run that ends on the usage limit of the account SHALL stop the task, keeping the runs before it.

#### Scenario: The setups take turns
- **WHEN** two questions are planned in the four setups with two repeats
- **THEN** the first four runs are the first question in the four setups in the first repeat, and the last four are the second question in the second repeat

#### Scenario: A usage limit stops the task
- **WHEN** a run ends with the message that the usage limit is reached
- **THEN** the task fails, and the results hold the runs before it and not that one

### Requirement: The results of the runs
`cargo xtask agents run` SHALL write `bench/results/agents.json` through a temporary file after every run, with the time, the machine, the Claude Code version, the model, the SHA-256 of the questions, the versions of the tools, and per run the question, the setup, the repeat, the outcome, the answer, precision, recall, F1, the input, output, cache-read and cache-write tokens, the cost in USD, the turns and the duration. It SHALL hold no path or user name of the machine.

#### Scenario: The agent results have the published shape
- **WHEN** agent results are written
- **THEN** the file has exactly the published keys in their order, and parses back

#### Scenario: No local path reaches the agent results
- **WHEN** a run answers with a path under the home directory and outside the checkout
- **THEN** the run counts it as wrong, and the results hold neither that path nor the user name

### Requirement: Resuming the runs
A task started over SHALL skip the runs whose question, setup and repeat the results already hold, and SHALL refuse to add to results written with another model, another Claude Code version or other questions.

#### Scenario: Finished runs are skipped
- **WHEN** the results hold the first run and the task starts over
- **THEN** it runs every planned run but that one

#### Scenario: Results of another model are refused
- **WHEN** the results were written with another model
- **THEN** the task fails naming both models, and runs nothing

### Requirement: The table of the agent runs
`cargo xtask agents table` SHALL print, for each kind of question, a Markdown table with a row per setup and the mean over its runs of recall, precision, F1, tokens, turns and seconds, the mean cost over its runs that report one, and the number of its runs and of those without an answer.

#### Scenario: The table averages the runs of each setup
- **WHEN** a setup has two runs of a kind, with recall 1 and 0.5
- **THEN** its row of that kind shows the recall 0.75 and two runs
