# Proposal: Name the member a local value of a class is used for

## Why

Asked about one method, `impact` lists the declarations that use it or use its class alone, because a use of a type alone refers to every member. `new OrderService().cancel(id)` was recorded as a use of `OrderService` alone, so a caller of `cancel` was listed as a caller of `place`, and so was `const s = new OrderService(); s.cancel(id)`, `function f(s: OrderService) { s.cancel(id); }` and the static `OrderService.create()`. Every wrong row costs the reader tokens and trust. A member that the code names should be recorded as that member.

## What Changes

- TypeScript: a name bound below the top level of a file by `const` to `new` of a class, or declared by a `const` or a parameter with a plain class type, holds that class. The scope table of the file records it, and a name bound twice in a scope, by `let`, `var`, a destructuring or a catch parameter holds none. A type with arguments, a union or an array is no plain class type.
- TypeScript: a member read from such a name, from a `new` in place, from an imported class or from a class of the file that declares it is a use of `Class.member` only. The name read as a value is a use of the class, its type is a use of `Class.`, which refers to no member, and a `new` whose value goes no further than its members is a use of `Class.constructor`.
- `Symbols::through_class` is the one place that turns a type and a member into a use; a use through a field goes through it too and is unchanged.
- Queries: no change to `crates/core`. A declaration whose only uses of a class name other members is no longer listed when `impact` is asked about a member.
- Versions: `READER_VERSION` of the TypeScript connector goes from 17 to 18, because the same source now gives other facts.

## Impact

- Capabilities `typescript` and `queries`.
- `crates/lang-ts/src/analyze.rs`, `analyze/symbols.rs`, `analyze/roots.rs`, `analyze/constants.rs`, `analyze/constants/scopes.rs`, `project.rs`, and `docs/architecture.md`.
- A tree whose members are read only through `this` and fields gives the same index as before. A tree that reads a member from a local instance, a parameter or a class gives uses of `Class.member` where it gave `Class`.
