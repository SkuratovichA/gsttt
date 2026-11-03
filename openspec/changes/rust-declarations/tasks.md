# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The core

- [x] 2.1 A `.member` use matches a read by member name
- [x] 2.2 An import of a module is asked a name and gives nothing
- [x] 2.3 A private glob import passes a name on
- [x] 2.4 Through a head more than 32 declarations override, a file counts only by naming the member of the head, and a class that takes the members of the head is no caller
- [x] 2.5 A member declared on an imported class reaches the holders of the class
- [x] 2.6 A re-export by name beats a star only for what its file hands out
- [x] 2.7 A private glob passes a name to a glob below it

## 3. The connector

- [x] 3.1 Imports list the item they take and the name they bind, and a glob is a `Static` import of `*`
- [x] 3.2 Declarations, members, uses, reads, exports, heirs and overrides of a Rust file
- [x] 3.3 A file read past syntax errors records none of them
- [x] 3.4 The code of a Rust file for `--changed`
- [x] 3.5 The `rust` connector goes from 5 to 6
- [x] 3.6 The provides name what a Rust walk cannot see
- [x] 3.7 A type parameter with a bound is read as the trait
- [x] 3.8 An import nothing names is used as the file loads, and only with one in scope a value of a written type reads a member by name
- [x] 3.9 A path goes on through what a crate re-exports, with that crate's own dependencies

## 4. The scenarios

- [x] 4.1 Each scenario has a test that fails without the code
- [x] 4.2 The `cli` scenarios: a changed Rust function follows its callers, and a new file is walked whole

## 5. The notes

- [x] 5.1 The blind spots of the connector, `SKILL.md` and `docs/architecture.md` say what a Rust walk reads now
