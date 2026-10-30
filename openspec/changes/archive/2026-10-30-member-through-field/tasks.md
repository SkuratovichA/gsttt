# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the deltas of the `typescript` and `queries` specs

## 2. TypeScript

- [x] 2.1 A declaration that reads `this.<field>.<member>` uses `<Type>.<member>`, and any other read of the field uses `<Type>`, with the scenarios of `Members read through a field of a declared type`
- [x] 2.2 The fact format one up, because the uses of a cached file change

## 3. The walk by name

- [x] 3.1 A use of `Type.member` is a use of that member, a use of `Type` of every member, and a class is used by the declarations that use any member of it, with the first two scenarios of `Members called through a field`
- [x] 3.2 A reached member goes on to the files that import its class, through barrels and a namespace import, with the other scenarios of `Members called through a field`
- [x] 3.3 The properties that hold the walk by name against the walk over files write uses of `Local.member` too

## 4. End to end

- [x] 4.1 An end-to-end test of `gsttt impact --name` with depth 2 over a resolver whose two methods call two methods of one injected service and serve a mutation each, and the sentences on what a declaration uses in `docs/architecture.md` and on `Runner.start` in the skill
