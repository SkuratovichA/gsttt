# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The core

- [x] 2.1 `ImportFlags::MODULES`, and the walk reading a name of such an import as the module when the file reached is that module
- [x] 2.2 A star barrel that lists the asked name among its own exports keeps its row
- [x] 2.3 A star barrel that declares the asked name itself, or re-exports it by name, yields to it, and a name a file exports itself is not read as the module of the same stem
- [x] 2.5 A star barrel lists the declaration that reads the name it binds again
- [x] 2.4 The blanking of text that Python and TypeScript share lives in `gsttt_core::text`

## 3. The connector

- [x] 3.1 Imports list a name for each name they bind and flag a `from` import that may name a module
- [x] 3.2 Declarations, uses, reads, run on load, exports, heirs and overrides of a Python file
- [x] 3.3 A file with a syntax error, or a star import and an unbound name, records none of them
- [x] 3.4 The provides name pytest fixtures, names read from a string by Django and functions replaced at run time
- [x] 3.5 The code of a Python file for `--changed`
- [x] 3.6 Versions: the `python` connector goes from 4 to 5
- [x] 3.7 A local keeps its type up to the next binding of its name, and a member read on a builtin value is no read
- [x] 3.8 Strings of `Literal` and the metadata of `Annotated` are no names, the first iterable of a comprehension is read where it stands, a class local to a function is an heir of nothing, and `X = wrap(X)` uses the import
- [x] 3.9 A loose annotation and several classes give no class, a local given values of different types is unknown at every read, and a member read beside a base the file does not declare stays a read
- [x] 3.10 The first binding of a name in a file with a star import uses the name the star may bring in
- [x] 3.11 A module exports every name it binds, its imports among them, and every form of `__all__`

## 4. The scenarios

- [x] 4.1 Python: a test for each scenario, a snapshot of the facts of a file with declarations, and property tests for the parser of imports and for the recorder
- [x] 4.2 Queries: tests for a name that may be a module, a name a file exports itself, a star barrel that lists the name, declares it, re-exports it by name or reads it before it binds it again, and a Rust file asked about by name
- [x] 4.3 CLI: a test for a changed Python function and a changed Rust file, one for a Python name that reaches a file that imports it or reads it before binding it again, and one for a TypeScript export by name beside a star export

## 5. The notes

- [x] 5.1 `docs/architecture.md` and the gsttt skill say that Rust is the one language that records no declarations
