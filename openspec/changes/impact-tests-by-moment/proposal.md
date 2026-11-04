# Proposal: Count the tests before an edit, and speak of the moment of work

## Why

A controlled replay of nine real merged changes with Sonnet 5.5 agents found that an installed gsttt is never called (0 of 36 runs, the tool schemas are deferred in Claude Code and never loaded, the skill never loaded), and that when the prompt tells the agent to use it, the runs take 4.4 more turns and read and search as much as before. Of the files an answer named before the first edit, 142 of 442 were tests, and the agent set `tests=false` in 7 of 72 questions. Two things follow: the answer should cost less while the agent is still working out what to edit, and the text the agent reads when it decides whether to call should name the moment of work, not a kind of question.

## What Changes

- `impact` over MCP takes `tests` as an optional boolean. Left out, it is true with `changed` and false otherwise, so a question asked while the edit is still being worked out counts the test files (`6 test files left out (ask with tests=true)`) and the closing `changed` question lists them. The command line keeps listing the tests unless `--no-tests` is passed.
- The query carries an optional hint in its request, which the text of an answer prints in brackets after the count of the tests left out. The JSON holds the count alone.
- The description of the skill begins with the moment of work: before an edit of a file that other files import or call, and before a change is finished. The skill tells the agent to load the tool schemas first, by the id Claude Code gives a plugin's tool, and a test pins that id against the plugin manifest.
- The server instructions and the description of `impact` begin with the same moment, and the instructions tell a client that lists the tools by name only to load the schema of `impact` before the first call.

### Not done

- Generated files are not counted apart. The index defines `FileTags::GENERATED` but no connector sets it and nothing else in the repository detects a generated file, so a count would need a new heuristic for each language.
- The line of code beside each nearest hit is not printed. The index records the line a declaration starts on and the line of an import, never the line of a call: `Declared.uses` and `Symbols.reads` hold names without positions, and the snapshot keeps no source text. A call site needs a position for each use in every connector, which is a change of the facts of four languages, and reading a line from disk by a guessed position would not be the line of the call.

## Impact

- Capability `mcp`: the requirement "Impact without tests over MCP" changes, and the instructions are pinned by a new requirement.
- Capability `queries`: a hint beside the tests left out.
- Capability `distribution`: the skill and its description.
- `crates/core/src/query/impact.rs`, `crates/mcp/src/arguments.rs`, `ask.rs`, `handler.rs`, `tools.rs`, `plugin/skills/gsttt/SKILL.md` and their tests. The facts and the snapshot layout do not change, so no version is bumped.
