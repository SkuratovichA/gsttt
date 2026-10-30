# Proposal: impact by symbol

## Why

`impact` works per file, so a question about one function gets the answer for its whole file: on vite, a change to `getHash` in `utils.ts` lists 241 affected files and 201 test files, where 19 tests fail when it throws. Agents asked which tests a function can break got every test that fails, inside a list of 74 to 77 that hit the size cap of an answer, and they pruned it by guesswork. Following the name only at the first hop narrows nothing on vite: the importers that take the name are a few, but everything after them is per file again.

A function certainly affects only the code that refers to it: the functions and methods that call it, and their files. What lies behind those callers is not known to be affected, and an agent that changes a caller can ask about that caller in turn. So `impact` asked about a name answers with its direct callers, one declaration each, and the agent drills down one hop at a time.

## What Changes

- The TypeScript connector records the declarations of each file: top-level functions, classes, variables, enums, interfaces, type aliases and namespaces, and each member of a class as `Class.member`, each with its lines and the names it refers to. It records what the file's top-level code uses as the file loads, the local name each import binds, and the local declaration behind each export of another name.
- `impact` takes names. Asked about names declared in a file, it lists the declarations that refer to them, in the file itself and in the files that import them, through barrels, each under its file with its line, and the walk stops there. A larger depth follows the callers of the callers. Code that runs at load, a test calling what it tests among it, is a caller too, and a file reached at load is followed whole from there.
- The MCP `impact` tool takes `names`, and the command line takes `--name`, which may be repeated. The tool description, the server instructions and the plugin skill say that a question about one function is asked with its name.
- Python and Rust record no declarations yet: a file of theirs asked about by name is followed whole, and the answer says so.

## Impact

- `crates/core`: `FileFacts.symbols` and the doc of `Import.exported` (`fact.rs`), the fact format, the graph and its snapshot layout, the walk and the impact answer.
- `crates/lang-ts`: the declarations, the uses, the code at load and the local names of imports and exports.
- `crates/mcp`, `crates/gsttt`, `plugin/`: the `names` argument, `--name`, and the texts that tell an agent about them.
- The specs of `queries`, `typescript`, `mcp` and `cli`.
- Indexing does more work per TypeScript file, and the cold index of vite must stay near its 95 ms.
