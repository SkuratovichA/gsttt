# Proposal: Follow a member that holds a value to the files that read through it

## Why

A name asked about reaches only the files that read it right after a value of its class. A file that reads `self.poetry.file.read()` calls `TOMLFile.read` through two properties that each name their class, and the walk stops at the first one, so the file is not listed.

## What Changes

- Python: a property, an annotated or assigned field, and a method with a return annotation that name a class are recorded in `holds` as members that hold a value of that class. A member of a builtin type holds nothing and ends a chain.
- Python: an attribute read right on another is recorded as a read of the pair, so `poetry.file` is a read beside `poetry` and `file`. A chain on a value the file shows is a builtin records no pair.
- Core: the walk by name follows a member that holds a value of a class on to the files that read the chain, up to four names. A file counts only when it reads each name of the chain right on the previous one, and it counts only through a class it imports. Members that hold each other end the walk, and a crowded head starts no chain.
- Core: `Symbols` gets a field `holds`, kept apart from `heirs` like `overrides`, because a nested class is already an heir under a dotted name.
- Versions: the `version` of the Python connector goes from 6 to 7, because it reads and stores more. `FACT_FORMAT` goes from 20 to 21 and `LAYOUT` of the snapshot from 13 to 14, because the stored shape of `Symbols` gains a field.

## Impact

- Capabilities `queries` and `python`.
- `Chain`, `Step` and `read_somewhere` in `crates/core/src/query/walk/names.rs`, `FileSymbols::holds` and `FileSymbols::reads_on` in `crates/core/src/graph/symbols.rs`, `Symbols.holds` and the docs of `Symbols.reads` in `crates/core/src/fact.rs`.
- `Table`, `Recorder` and `finish` in `crates/lang-python/src/symbols.rs` and `crates/lang-python/src/symbols/table.rs`.
- Python snapshots of files with such members or chains gain a `holds` list or a read of a pair. A tree whose files hold no such member gives the same answers.

## Limits

- A chain broken by a local, `f = self.poetry.file; f.read()`, is not followed.
- A file is judged by what it reads as a whole: a pair read on another value counts for it, as a single read does today.
- A chain goes no further than the first member of a nested class.
- Only Python records these facts for now.
