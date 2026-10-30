# Proposal: the depth counts hops through code

## Why

The depth of `impact` counted every hop, and a contract is a hop. Asked about a function that a handler uses, `impact` with the default depth reached the handler and then stopped before the client of the route the handler serves: the one file that breaks without a type error, because the contract is the only thing that joins it to the change. A contract is a fact a connector read, not a step through code, so it should not spend the depth.

Separately, an answer about a widely used name lists dozens of test files, and an agent that wants the files to edit has to read past them.

## What Changes

- The depth bounds the hops through code: imports, uses of a name, loads and mocks. A hop across a contract counts toward the distance of a file and not toward the depth. This holds in the walk by name and in the walk over files, so a file reached across a contract is followed through code as far as any other.
- A walk by name that a depth stops says so, as before, only when a file at the depth would reach a new file through code.
- The first line of an impact answer says `within N hop(s) through code`.
- `impact` can leave the tests out: `--no-tests` on the command line and `tests: false` on the MCP tool. The walk is the same, the answer lists no tests and says how many test files it left out, and the footer still covers every file the walk reached.

## Impact

- `crates/core`: the two walks keep, for each part they reach, the hops through code of the way that reached it first; `ImpactRequest.no_tests` and `Impact.tests_left_out`.
- `crates/gsttt`, `crates/mcp`: the flag, the argument and the schema of the tool.
- Specs: `queries` (Impact through imports, Impact by name, Depth by name, and two new requirements), `cli` (The impact command), `mcp` (Tools, and a new requirement).
- Docs: `docs/architecture.md` and the plugin skill.
