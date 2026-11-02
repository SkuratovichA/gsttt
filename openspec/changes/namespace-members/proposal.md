# Proposal: Name only what a namespace import uses

## Why

`import * as lib from './svc'` made the importer a caller of every name of `svc.ts`, so an importer that only used `lib.other` was listed when `impact` was asked about `helper`. An agent asks what one function affects, and every file beyond that costs it tokens.

## What Changes

- Core: in the walk over names, a namespace import is a caller only of the names it uses: a declaration, or a load, that uses `lib.helper` is a caller of `helper` and not of `other`. A use of the namespace alone (`consume(lib)`, `lib[key]()`, `const { helper } = lib`) stays a use of every name, because the walk cannot tell which name it reads. A name of a class is read through a namespace as the class, so `lib.Service.make()` is a caller of `Service.place`, as before.
- The name goes on through `export * as ns from './svc'` and through `import * as lib from './svc'; export { lib }`: an importer of the barrel is a caller only of the names it reads through the namespace.
- Connector (`lang-ts`): a qualified type name (`x: lib.Foo`, `Array<lib.Foo>`), a type query (`typeof lib.helper`) and a JSX member (`<lib.Comp />`) are uses of the whole name, through the path by which the value `lib.other` becomes a use of `lib.other`. A value typed `lib.Foo` that goes elsewhere is a use of `lib.Foo` too, and no longer of the form `lib.Foo.`. They recorded the bare root `lib`, which the walk reads as the namespace alone and so as every name.
- A dynamic import and a `require` that take the module itself are unchanged: they bind no local, and the declaration that holds the import is a use of every name.
- The requirement "Imports that take every name" belongs to the walk over whole files and is unchanged.

## Impact

- Capabilities `queries` and `typescript`.
- `crates/core/src/query/walk/names.rs` (`Name::member_taken`, `Local::refers`), `docs/architecture.md`, and in `crates/lang-ts` `visit_ts_type_name` and `visit_jsx_member_expression` in `analyze.rs`.
- A facts change: `READER_VERSION` of `lang-ts` goes from 18 to 19, because the uses of a declaration with a qualified type name or a JSX member change. `FACT_FORMAT` does not change.
- An answer lists fewer files for a name asked about through a namespace import, never more.
