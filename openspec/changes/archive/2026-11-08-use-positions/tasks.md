# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, the spec deltas and `tasks.md`

## 2. The line of a use

- [x] 2.1 `Use { name, line }` and the `Uses` collector in `fact.rs`, the symbols table of the graph keeping the line beside the name, and `LAYOUT` and `FACT_FORMAT` bumped in the same commit
- [x] 2.2 The TypeScript connector fills the line of every use, member chains and constructors included
- [x] 2.3 The Python connector fills the line of every use, a member of a chain on the line the member stands on
- [x] 2.4 The Rust connector fills the line of every use, grouped imports included
- [x] 2.5 The Go connector fills the line of every use, a method on the line the method stands on
- [x] 2.6 A scenario test per connector and one for the snapshot rebuilt after the bump

## 3. The line in the answer

- [x] 3.1 The walk by name carries the mention a caller was reached through, and the impact answer a spot per file at distance 1
- [x] 3.2 The source of a nearest hit read at query time, only while the index is fresh and only while the line still holds the name, in the CLI and in the MCP server
- [x] 3.3 `at` and `snippet` in the JSON of an entry, and `path:line` with the text under it in the text answer
- [x] 3.4 Scenario tests: the line and its text, a line that no longer holds the name, a stale index, the cut at 30 and the width of 160

## 4. Generated files

- [x] 4.1 The tag set in one place of the engine from a header marker, a `linguist-generated` rule and the names and directories only tools use, with a miss preferred to a false positive
- [x] 4.2 `generated: N files (ask with generated=true)` in the answer, the list only when asked, `generated` in the MCP query and `--generated` on the command line
- [x] 4.3 Scenario tests: a header marker, a file that merely mentions a marker, a `.gitattributes` rule and its unset, a name or directory, and the answer that counts them apart

## 5. The skill

- [x] 5.1 The advice to pass `tests: true` while working out an edit, and the other advice that lengthens an answer, removed from `plugin/skills/gsttt/SKILL.md` and from the wording the server carries
- [x] 5.2 The distribution check pins the wording

## 6. The documents

- [x] 6.1 `docs/architecture.md` describes the line of a use, the spot of an entry and the generated tag
