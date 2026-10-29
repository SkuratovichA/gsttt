# Proposal

## Why

In a run of the agent benchmark, an agent called `impact` with `all: true` on `packages/vite/src/node/utils.ts` of vite, and the answer was 62,972 characters on 447 lines. Claude Code, the client the plugin is made for, saves a successful text result longer than 50,000 characters to a file and hands the model only a reference to it, and it warns when a tool's output passes 10,000 tokens. The agent could not read that file, so the call was lost. gsttt exists to spend fewer of an agent's tokens and to give it better answers, and an answer of hundreds of entries spends the agent's context and buries the part it needs, so the limit belongs in the server and not in a raised limit of the client.

## What Changes

- The MCP server holds every answer to 25,000 characters, which stays under both limits of the client. An answer that would be longer shows the most entries of each list that fit, and says so on its first line, which starts with `cut:`.
- `Entries::First(n)` in `gsttt-core` cuts every list at `n` entries, and the names of a line as `Entries::Capped` cuts them.
- The `all` argument of the tools says that it lifts the cap of 30 entries as far as the size of an answer allows.
- The skill and `docs/architecture.md` say the same.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `mcp`: an answer is cut to a size.

## Non-goals

- The command line's `--all` and every JSON answer stay complete: a person or a script that asks for everything gets everything.
- No `anthropic/maxResultSizeChars` in the tool listing: raising the limit of the client spends the agent's context, which is what this change is against.

## Impact

`crates/core` (`Entries`), `crates/mcp` (the budget and the descriptions of `all`), `plugin/skills/gsttt/SKILL.md` and `docs/architecture.md`. An answer within 25,000 characters is returned exactly as before; only a longer answer changes.
