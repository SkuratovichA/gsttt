# Proposal: Walk a removed declaration by its old name

## Why

`gsttt impact --changed` on a file in which one declaration was removed or renamed walks the file whole, so every importer of the file is listed although most take other names. That is the listing a grep of the file name gives. An importer that was not changed still names the old name in its import, so the old name is known and can be walked like any name.

## What Changes

- The comparison of the two versions of a file no longer gives up on a removed or renamed declaration. It returns the declarations that differ and, apart, the names the old version declares or exports that the new one does not, members `Class.member` included. A declaration that stops being exported, such as `export function foo` turned into `function foo` or a removed `export default`, is among them.
- The order check compares only the declarations both versions hold, so a declaration that moves, or a top-level statement that changes, still changes the file whole.
- `Change` and `ChangedPart` gain `removed`. `ChangedPart` leaves it out of the JSON when it is empty, so every answer without a removed name stays as it was.
- The walk starts from the importers of each removed name, at the first hop. A removed member goes the way the exports of a member go, through the class the new version exports and the names that take its members, and a removed class is its own name.
- The text header names them where it names the declarations: `removed formatId in src/format.ts`.

## Limits

- The contract sites of a removed declaration, such as a route a removed handler served, are not reached: the index holds the new version, so its clients are not found, before or after this change.
- `start_overridden` is not run for a removed method, so the classes that override it are not asked about.
- `export { b as c }` rewritten is a changed statement, not a declaration, and still walks the file whole.
- Versions: no fact and no reader output changes, so no `READER_VERSION`, `FACT_FORMAT` or `LAYOUT` bump.

## Impact

- Capability `cli`.
- `changed_declarations` in `crates/core/src/engine/declarations.rs`, `Change` in `crates/core/src/engine/changes.rs`, `ChangedPart` and the header in `crates/core/src/query/impact.rs`, and `reach_changed`, `seed_of`, `walk_from` and `Step::exports` in `crates/core/src/query/walk/names.rs`.
- A tree in which no declaration is removed gives the same answers.
