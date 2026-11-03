# Proposal: Naming a class to take its members reads none of them

## Why

Asked about `DispatcherBase.destroy` in undici, `impact` listed ten files the compiler does not, among them `lib/dispatcher/client.js` and `index.js`. `class Client extends DispatcherBase` names `DispatcherBase` only to take its members, and `socket.destroy()` elsewhere in the file made the file a reader of the member. `module.exports.Agent = Agent` in `index.js` only hands the class on, and got a row for a member nothing in the file names. The answer has to stay small: gsttt must not behave like a grep on a method name.

## What Changes

- Asked about a member, a declaration that takes the members of the class it names (`class B extends A`, `const a = new A()`, `const a = make()`) is at most a holder of the class, so the file is counted in the note and not listed. A read through it is written `B.member`, which is matched on its own.
- The same holds in a file that declares the class, and for a bare use of such a name that hands the class on.
- Asked about a member, a file that only hands the class on (`module.exports.A = A`) gets no row and the walk goes on through it, as it does through a TypeScript re-export. Asked about the class, it keeps its row.
- No version moves: no fact changes.

## Impact

- Capability `queries`.
- `crates/core/src/query/walk/names.rs`: `refers_decl` and `takes_members` replace `refers_any`, `Reach::refers` and the last lines of `bind_names`.
