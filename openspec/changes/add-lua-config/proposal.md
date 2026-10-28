# Proposal

## Why

A repository needs to say what detection cannot guess: a gateway prefix, a tsconfig that is not at the root, a pack for an in-house HTTP client, a connector for a language gsttt does not ship. A Lua config gives that the reach of code (functions, local modules, conditions) while staying sandboxed and deterministic, and LuaLS annotations generated from the modules' schemas make it as checkable in an editor as a typed config.

## What Changes

- `.gsttt/init.lua` evaluated into `gsttt_core::Setup` through `require("gsttt")`: `setup`, a function per module, `disable`, `pack`, `disable_pack`, `external`.
- `require` from `.gsttt/lua/` only; a sandbox without file, process or code loading and without a pattern matcher, with memory, instruction and time limits, a bounded log, a fixed `pairs` order and no addresses; the same sandbox for built-in packs, which today still have `load`, `loadfile` and `dofile`.
- A fingerprint of the evaluated setup for the snapshot's config check.
- LuaLS annotations, `.luarc.json` and a starter config, generated for `gsttt init`.
- External connectors over JSON lines, with a schema of the messages.

## Capabilities

### New Capabilities

- `config`: the Lua config, its sandbox, editor support and external connectors.

### Modified Capabilities

None.

## Impact

`crates/config/` only. The binary wires `gsttt init`, `gsttt schema external` and the registry's module list in the CLI change.
