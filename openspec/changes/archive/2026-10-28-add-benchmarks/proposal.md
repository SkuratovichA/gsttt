# Proposal

## Why

The README and the site of the first release show a speed comparison: how long gsttt and the tools people use today take to build the import graph of a whole repository. The repository is about to go public, so anyone must be able to reproduce every number in it from the repository alone: the same repositories at the same commits, the same tools at the same versions, and one command that measures them and writes the results in the shape the site reads.

## What Changes

- `bench/bench.toml`: the repositories, each by URL, release tag and full commit, with its language, the `tsconfig.json` the TypeScript tools read and the packages grimp reads; and the tools, each by version, with the release archive of codebase-memory-mcp named per platform with its SHA-256.
- `xtask/src/bench.rs` with `xtask/src/bench/`: `cargo xtask bench --gsttt <binary> [--runs <n>] [--repository <name>]... [--out <file>]`. It clones the repositories into `bench/repos` and installs the tools into `bench/tools`, times every tool cold and warm with hyperfine, and writes the medians to `bench/results/benchmarks.json`.
- `.gitignore`: `/bench/tools` beside `/bench/repos`.

Which tools measure a repository follows its language. A TypeScript repository is measured with gsttt, dependency-cruiser, madge and codebase-memory-mcp, a Python one with gsttt, grimp and codebase-memory-mcp, and a Rust one with gsttt and codebase-memory-mcp. gsttt is not pinned: it is the binary the task is given, and the results carry the version it prints.

A cold run is cold for the tool, not for the machine. It starts after the tool's own cache is removed (and, for codebase-memory-mcp, its daemon is stopped), but the file cache of the operating system is not dropped, because that needs root. After the first run the files of a checkout are in memory for every tool alike, and the numbers must be read that way wherever they are shown.

## Capabilities

### New Capabilities

- `benchmarks`: the pins, the setup, the cold and warm runs, the choice of tools and the results file of the speed comparison.

### Modified Capabilities

None.

## Impact

`bench/bench.toml`, `xtask/src/bench.rs`, `xtask/src/bench/`, `xtask/src/main.rs`, `.gitignore`. The task needs `hyperfine`, `git`, `pnpm`, `uv` and `curl` on `PATH`, and downloads four clones, the Node tools, grimp and one release archive of codebase-memory-mcp. `bench/repos`, `bench/tools` and the temporary files of `bench/results` are never committed. The numbers are measured later, on an idle machine, and committed as `bench/results/benchmarks.json` by a change of their own.
