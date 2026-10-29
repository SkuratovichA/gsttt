# Design: impact by symbol

## What was measured

On vite at `981f07fa`, scope `packages/vite`, with the release of `6a8a111` and `--all`:

| question | name | tests that fail | tests listed | files listed | importers | importers that take the name |
| --- | --- | --- | --- | --- | --- | --- |
| vite-tests-1 | `moduleListContains` | 2 | 201 | 241 | 84 | 4 |
| vite-tests-2 | `ssrLoadModule` | 5 | 201 | 241 | 1 | 1 |
| vite-tests-3 | `getHash` | 19 | 201 | 241 | 84 | 9 |
| vite-tests-4 | `analyzeImportedModDifference` | 4 | 203 | 252 | 3 | 1 |

Keeping only the importers that take the name and walking per file from them lists the same 201 and 203 tests: the importers that take the name import much else, and so does everything behind them. Following names on every hop is what narrows, and it needs to know which declaration of a file uses which name.

## The answer is the direct callers

A name certainly affects the declarations that refer to it, and nothing behind them is known to be affected: a caller may keep its behaviour when what it calls changes. So `impact` asked about names lists the direct callers and stops, unless a depth is asked. A caller is a declaration, so that a function used in one method of a class makes that method a caller and not the class. Each hop of a larger depth goes from the declarations reached to the declarations that refer to them.

Tests come out of this as follows. A test calls what it tests in a callback of `test` or `it`, which is code at load, so a test file that uses a name directly is a direct caller and is listed among the tests. A test that reaches the name through other code is found by asking about the callers, or by a larger depth.

## Facts

`FileFacts.symbols: Option<Symbols>`, `None` for a connector that records no declarations and for a file read past errors.

```rust
pub struct Symbols {
    /// The declarations of the file, by the line they start on, then by name.
    pub declared: Vec<Declared>,
    /// What the file's code refers to as the file loads, sorted, once each.
    pub on_load: Vec<String>,
    /// Each name the file exports from a local of another name: `b` from `a` for `export { a as b }`, `default` from `f` for `export default f`. A name exported under its own local name is not listed.
    pub exports: Vec<LocalExport>,
}
pub struct Declared {
    /// `getHash`; `ModuleRunner.import` for a member of a class; `default` for `export default <expression>`.
    pub name: String,
    pub line: Line,
    pub end: Line,
    /// The names it refers to, sorted, once each: declarations of the file, `Class.member` for `this.member` in the class, and the local names of the file's imports. A variable also lists the declarations that write it.
    pub uses: Vec<String>,
}
pub struct LocalExport { pub name: String, pub local: String }
```

`Import.exported` of a static import, type-only included, carries the local name of every entry of `names`, in the same order, when the file has symbols: `[React]` for `import React from 'react'` (names `[default]`), `[ns]` for `import * as ns` (names `[*]`), `[b]` for `import { a as b }`. The field already allows "the name the file binds an entry as", and the graph reads it today only for a re-export, whose meaning does not change. `FACT_FORMAT` goes from 2 to 3, so every cached fact is read again.

## What a TypeScript declaration is and uses

- Declarations: top-level `function`, `class`, each name of a variable declarator, destructuring included, `enum`, `interface`, `type`, `namespace` and `module`, `export default` of an expression as `default`, and every member of a class as `Class.member`: methods, accessors, fields, `constructor`, private `#names`. Overloads and merged declarations are joined by name, with the lines of the first and the end of the last. A static block belongs to the class.
- Uses: every identifier reference that names a top-level declaration or an import binding of the file, JSX tag names, type references and `typeof` included. Shadowing is ignored, which can only add uses. `this.m` and `this.#m` inside class `C` are uses of `C.m`; `C.s` anywhere in the file is a use of `C.s` and of `C`. An export specifier is not a use. A declaration does not list itself.
- Writes: a top-level variable lists as uses the declarations that assign it, update it (`x++`), assign a member of it (`x.y = …`), `delete` from it, or call one of `set`, `add`, `delete`, `clear`, `push`, `pop`, `shift`, `unshift`, `splice`, `sort`, `reverse` and `fill` on it. Vite keeps 30 caches in module-level `Map` and `Set` objects, and a function that fills one affects the functions that read it.
- At load: everything a top-level statement that is not a declaration refers to, nested callbacks included, and, inside a declaration, what is referred to under a call, `new`, a tagged template or a decorator that is evaluated at load: in a variable initializer, a static field or block, a computed key, a decorator and `extends`. Function bodies, parameter defaults and instance field initializers run later and do not count. `const x = f()` puts `f` both at load and in the uses of `x`; `const y = f` puts it only in the uses of `y`.

## The walk by name

The walk keeps, for each file, whether its load is reached and which of its declarations are reached.

- **Start.** The asked names that the file declares are reached at distance 0. A name it does not declare goes into a note. A file without symbols is followed whole, as today, with a note.
- **One hop from reached declarations `D` of file `F`, at distance `d`:**
  - the declarations of `F` that use one of `D`, and the load of `F` when `on_load` holds one of `D`, at `d + 1`;
  - the exports of `F` whose local is in `D`; for each importer that takes one of them, by name or through a chain of re-exports, the declarations that use its local name, and its load when its `on_load` holds it, at `d + 1`. A barrel on the way is listed with the reason it re-exports the name, at `d + 1`, and adds no hop. So does an importer that exports again a name it imports, `export { foo }` or `export default foo` after `import { foo }`: it is listed with the reason it imports the name, and passes the name on under the name it exports it as. A namespace import counts as a use of every name of `F` by whatever uses the namespace. A dynamic import or `require` is a use of every name by the declaration whose lines hold the import, or by the load when none does. A side-effect import uses nothing by name;
  - for a member `C.m` in `D`: outside `C`, a call through a value names no class, so the declarations that refer to `C` count as its callers, and the answer says so in a note;
  - the contracts whose sites lie within the lines of a declaration in `D`, crossed as a file affected in all its names crosses them today. A site belongs to the narrowest declaration whose lines hold it, a member before its class, and to the load of its file when none does. Across a contract the walk reaches, on the other side, the declaration each site belongs to, or the load of its file, at `d + 1`, and a file without symbols whole.
- **One hop from a file `F` reached at load, at distance `d`:** every file that loads `F` is reached at load at `d + 1`: a static import that is not type-only, a re-export, a side-effect import, a dynamic import and `require`. `F` crosses all its contracts, as a whole file does today.
- **Files without symbols** reached on the way are followed whole from there, as today, and so is every edge into a file whose exports are unknown.
- **Depth.** With names, the walk stops after one hop unless the request gives a depth. When it stops with callers left to follow, the answer says so and how to go on.
- **Without names** the walk is today's, unchanged: same files, distances, reasons and snapshots.

## The answer

- Header: `impact of getHash in packages/vite/src/node/utils.ts, within 1 hop`.
- An entry reached by name carries its callers, each a declaration and the line it starts on, and whether its load is reached: `packages/vite/src/node/optimizer/index.ts  distance 1  imports getHash from packages/vite/src/node/utils.ts; in getDepHash (line 1190), getOptimizerBrowserHash (line 1222)`, and `…; at load` for a file whose top-level code uses it. Callers are cut after three like every list of names, unless all entries are asked.
- The asked file is listed when its own declarations call the names, with the reason `declares getHash`.
- JSON: `Affected` gains `callers: [{ "name", "line" }]` and `load: true`, both left out when empty or false, so an answer without names serialises as today.
- Notes: a name the file does not declare; a file followed whole because its connector records no declarations; callers of a member found through its class; the walk stopped at the depth.

## Arguments

The MCP `impact` tool takes `names`, a list of strings, and `gsttt impact` takes `--name`, which may be repeated. Names go with paths: with `changed` they are refused, because which declarations a diff changes is not derived yet.

## Speed

Recording uses adds work to every TypeScript file. The analyzer already visits every node once; uses are collected in the same visit, against a table of the file's top-level bindings built before it, and stored once per declaration. The cold index of vite was 95 ms with the release of `6a8a111` (`cargo xtask bench --gsttt <binary> --repository vite`), and the change must keep it within a few milliseconds of that.

## Gaps, said in the answer or here

- A call through a value is not resolved to a member: callers of `C.m` outside `C` are taken as the declarations that refer to `C`, which can miss a declaration that gets an instance without naming the class.
- Mutation of another module's state through an import, and names reached through `globalThis`, `eval` or a computed key, are not followed. The walk over files misses them too.
- Python and Rust are followed per file.

## Not in this change

- `impact` with `changed` mapping the changed lines of a diff to the declarations that hold them.
- A namespace import narrowed to the members it reads.
- A route registered at load attributed to its own statement: today the load of the file is reached, and it crosses all the contracts of the file.
