# Proposal: Credit the packs that find SQL in the provides

## Why

The coverage line of a project says which packs are behind each kind of contract, and an agent reads it to learn what it can trust. The Python connector built that list from `Pack::contracts`, which covers calls, decorators, chains and assigns but not `[[embeds]]`. The `dbapi` pack has nothing but an embed rule and `sqlalchemy` finds its SQL by one, so the `table` calls of a Python project named neither. The Rust connector already credited its embed rules with a mapping private to it, and a later change that reads raw SQL in TypeScript needs the same credit, so the mapping belongs in core once.

## What Changes

- Core: `Pack::contracts` also lists what the embed rules of a pack find, and the language of a rule gives the kind (`sql` gives `table`, `graphql` gives `graphql`, any other language is a kind of its own). The code runs an embedded query, so the role is `calls`.
- Python: `dbapi` and `sqlalchemy` are named in `via` of the `table` calls.
- Rust: the private mapping is gone. Its output does not change.
- TypeScript: the `graphql` pack is named in `via` of the `graphql` calls, which its `[[embeds]]` rule finds.

## Impact

- Capability `python`: one requirement with two scenarios. Capability `typescript`: one requirement with one scenario.
- `crates/core/src/rules.rs`, and the `provides.rs` of `lang-python`, `lang-rust` and `lang-ts`.
- Provides are not the facts of a file: they are computed from the packs when a project is opened and stored only in the coverage of the index. So `FACT_FORMAT`, `READER_VERSION`, `LAYOUT` and the versions of the connectors stay where they are: no fact, key or archived type changes. A stored index keeps the old line until the index is rebuilt, which a new version of gsttt does, since the snapshot checks it.
