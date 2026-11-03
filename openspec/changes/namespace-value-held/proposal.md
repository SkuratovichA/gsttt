# Proposal: A module handed on as a value uses no name by itself

## Why

Asked about one name, `impact` listed every file that hands the module around as a value: `foo(ns)`, `vars(httpx)`, `monkeypatch.setattr(helpers, …)`, `const m = require('./m'); register(m)`. Such a file names nothing, so the answer for each name of the module carried it, and on a small repository two test files made up 91 of 208 extra files. The answer has to stay small and exact: gsttt must not behave like a grep on a module. The compiler counts none of these files.

## What Changes

- Asked about a name, a declaration or a load that uses a namespace, or a local a `require` binds to the module, alone is a use of the name only where its file reads the name on some value, as a bare use of a class has been since `member-held-names`. Otherwise the file is counted in the existing note, `N files load this module and read none of the names asked about`, and not listed.
- Through a namespace, the name read is the member of the class the walk asked about, not the class.
- Asked about the file, with no name, every importer is still listed, so nothing is out of reach.
- `docs/architecture.md` says so.
- No version moves: no fact changes, only how a query reads them. `FACT_FORMAT`, `READER_VERSION`, the connector versions and `LAYOUT` stay.

## Capabilities

### Modified Capabilities

- `queries`: the requirements `A load names what it takes`, `Members reached through a field` and `A namespace import names what it reads` replace the three that said a use of a namespace alone is a use of every name.

## Impact

- Code: `crates/core/src/query/walk/names.rs`, `Local::refers`; the `namespace` field of `Local` and the `value` local that fed it are gone.
- Docs: `docs/architecture.md`. The plugin texts say nothing of a namespace used as a value.
