# Proposal: the line of a use, and what an answer shows with it

## Why

An `impact` answer names the files a change reaches and, by name, the declarations that call it, with the line each declaration starts on. A person or an agent that wants to see the call still opens the file and looks for it, because the index records which names a declaration uses and not where. The answer also lists files a tool wrote beside the files someone did, so that a change to a type can fill the list with generated code.

## What Changes

- Every use a connector records, in the uses of a declaration, in the uses of the code that runs as a file loads and in the members read on a value of unknown class, carries the line of the node that makes it. The TypeScript, Python, Rust and Go connectors fill it, and the graph keeps it beside the name. A name used on several lines keeps the first.
- The archived layout changes, so the snapshot layout and the fact format move in the same commit, and a snapshot or a fact cache written before is rebuilt.
- An `impact` answer shows, for a file at distance 1, the line where it calls or imports what the walk came from and the text of that line, read from the working tree when the answer is made. The line is shown only when the index is fresh and the line still holds the name, the text is one line of at most 160 characters, and only the first 30 files and the first 30 tests are read. The JSON gains `at` and `snippet` on those entries.
- The engine tags a file as generated from a header marker among the first ten lines, a `linguist-generated` rule of a `.gitattributes`, or a name or directory only tools use. `impact` counts those files apart as `generated: N files (ask with generated=true)` and lists them only when asked, with `generated` an optional boolean of the MCP query and `--generated` on the command line.
- The skill no longer tells the agent how to make an answer longer: not to pass `tests: true` while working out an edit, not to give a larger depth, not to ask for every entry.

## Impact

- The `indexing`, `queries`, `cli`, `mcp`, `distribution`, `typescript`, `python`, `rust` and `go` capabilities.
- `crates/core` (the fact, the symbols table, the walk by name, the impact answer, the engine), the four language connectors, `crates/mcp`, `crates/gsttt` and `plugin/skills/gsttt/SKILL.md`.
- What the lines save an agent in tokens, turns or money is not measured here, and nothing in this change claims it.
