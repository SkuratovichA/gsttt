# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The connector

- [x] 2.1 `symbols.rs`: `through_class` pulled out of `field_use`, and `chain` names the member of an imported name or of a class of the file
- [x] 2.2 `constants.rs` and `scopes.rs`: a scope table that records the class a `const` or a parameter holds
- [x] 2.3 `analyze.rs`: a member, a value, a type and a `new` of a held class are read through `through_class`
- [x] 2.4 `project.rs`: `READER_VERSION` from 17 to 18
- [x] 2.5 `docs/architecture.md`: the uses of a member and of a type alone

## 3. The scenarios

- [x] 3.1 TypeScript: the eight scenarios of "Members read through a value of a known class" on the fixture `member-uses`
- [x] 3.2 Queries: the two scenarios of "A caller is told apart by the member it names"
