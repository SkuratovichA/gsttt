# benchmarks Specification

## Purpose
Benchmarks are the speed comparison of the README and the site: how long gsttt and the tools people use today take to build the import graph of a whole repository, cold and warm, on public repositories pinned at a commit, so that anyone can reproduce every number from the repository alone.

## Requirements

### Requirement: Pinned inputs
`bench/bench.toml` SHALL name each repository by URL, release tag and full commit, with its language, the `tsconfig.json` its TypeScript tools read when it has one and the packages grimp reads when it is Python, and each tool by version, with the release archive of codebase-memory-mcp named per platform with its SHA-256. gsttt is not pinned: it is the binary the task is given.

#### Scenario: Every repository is pinned at a commit
- **WHEN** `bench/bench.toml` is read
- **THEN** every repository has a unique name, a URL, a tag and a commit of 40 lower-case hexadecimal digits

#### Scenario: Every tool is pinned at a version
- **WHEN** `bench/bench.toml` is read
- **THEN** every tool that the language of a repository uses, except gsttt, has a version, and every archive of codebase-memory-mcp has a SHA-256 of 64 lower-case hexadecimal digits

### Requirement: Setup
`cargo xtask bench` SHALL clone each repository it measures into `bench/repos/<name>` at its commit, install the Node tools with pnpm into `bench/tools/node`, grimp with uv into a virtual environment in `bench/tools/python`, and codebase-memory-mcp from its release archive into `bench/tools/cbm`, checking the SHA-256 of the archive before it unpacks anything. It SHALL reuse what is already there at the pinned commit or version.

#### Scenario: An archive with another checksum is refused
- **WHEN** the SHA-256 of an archive is not the pinned one
- **THEN** the task fails naming the archive and both sums, and unpacks nothing

### Requirement: Cold and warm runs
hyperfine SHALL time every run. A cold run SHALL start after the cache of the tool, when it keeps one, is removed and, for codebase-memory-mcp, its daemon is stopped. A warm run SHALL follow a run that left the cache in place and SHALL have a warm-up run. The median of each SHALL be kept in whole milliseconds. The file cache of the operating system is not dropped, because that needs root.

#### Scenario: A cold run starts without the tool's cache
- **WHEN** the cold run of a tool that keeps a cache is planned
- **THEN** its prepare step removes exactly that cache, which for gsttt is `.git/gsttt`

#### Scenario: A warm run keeps the cache
- **WHEN** the warm run of a tool is planned
- **THEN** it has no prepare step and at least one warm-up run

### Requirement: The tools follow the languages
A TypeScript repository SHALL be measured with gsttt, dependency-cruiser, madge and codebase-memory-mcp, a Python one with gsttt, grimp and codebase-memory-mcp, and a Rust one with gsttt and codebase-memory-mcp. Every tool SHALL read the whole checkout, except grimp, which reads the packages that `bench/bench.toml` names.

#### Scenario: Each repository is measured with the tools of its language
- **WHEN** the runs of a TypeScript, a Python and a Rust repository are planned
- **THEN** each uses the tools of its language, and every command but grimp's reads the whole checkout

### Requirement: The results
`cargo xtask bench` SHALL write the results to `bench/results/benchmarks.json` through a temporary file, with the keys `measuredAt`, `machine` and `repositories` of `name`, `url`, `files` and `tools` of `tool`, `version`, `coldMs`, `warmMs` and `command`, in that order. `command` SHALL be the command as a reader types it in the checkout. The file SHALL hold no path of the machine that ran the task, and no user name.

#### Scenario: The results have the published shape
- **WHEN** results are written
- **THEN** the file has exactly these keys in this order, and parses back

#### Scenario: No local path reaches the results
- **WHEN** results are written for checkouts and tools under a home directory
- **THEN** the file holds neither that directory, nor the path of the checkout, nor the user name
