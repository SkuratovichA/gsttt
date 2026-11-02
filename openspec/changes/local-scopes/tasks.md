# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The scopes

- [x] 2.1 `crates/lang-ts/src/analyze/constants/scopes.rs`: the walk that finds what each function, block, loop, catch clause, class body and namespace binds, and reads the `const` of a scope as a constant
- [x] 2.2 `crates/lang-ts/src/analyze/constants.rs`: `locals` replaces `rebound`, `Constants::local` finds the innermost scope that binds a name at an offset, and the readers of a constant and of an import ask it first
- [x] 2.3 `crates/lang-ts/src/analyze/roots.rs`: `rebound` is removed, with its tests ported to the scopes in `constants.rs`
- [x] 2.4 `crates/lang-ts/src/project.rs`: `READER_VERSION` 13 to 14
- [x] 2.5 `crates/lang-ts/src/provides.rs`: the blind spots of a route path and of a URL
- [x] 2.6 `crates/lang-ts/src/analyze/constants.rs` and `analyze.rs`: `spec_of` takes the position and answers none where a scope binds the name again, and `imported_receiver` passes the position of the call
- [x] 2.7 `provides.rs` and `README.md`: the blind spots for a router, an app and a client named like a variable of a function say the file makes it

## 3. The scenarios

- [x] 3.1 TypeScript: a test and a fixture for "A function-local constant is read"
- [x] 3.2 TypeScript: a test and a fixture for "A local built from another local and a constant of the file is read"
- [x] 3.3 TypeScript: a test and a fixture for "Two blocks that declare the same name read their own"
- [x] 3.4 TypeScript: a test and a fixture for "A closure reads a constant declared after it in the same scope"
- [x] 3.5 TypeScript: a test and a fixture for "An inner function reads a constant of the function around it"
- [x] 3.6 TypeScript: a test and a fixture for "A local object constant is read by its property"
- [x] 3.7 TypeScript: a test and a fixture for "A parameter hides a constant of the file of the same name"
- [x] 3.8 TypeScript: a test and a fixture for "A local constant hides a constant of the file of the same name"
- [x] 3.9 TypeScript: a test and a fixture for "A let and a destructured name hide a constant of the file"
- [x] 3.10 TypeScript: a test and a fixture for "A var in a block hides a constant of the file in the whole function"
- [x] 3.11 TypeScript: a test and a fixture for "A type that names a parameter hides nothing"
- [x] 3.12 TypeScript: the test and the fixture of "A name that a local binds again is no import" follow the modified requirement
- [x] 3.13 TypeScript: a test and a fixture for "A client made in a function reads the base it is given there"
- [x] 3.14 TypeScript: a test and a fixture for "A parameter named like an imported client is no client"
- [x] 3.15 TypeScript: a test and a fixture for "A parameter named like an imported router is no mount"
- [x] 3.16 TypeScript: a test and a fixture for "An element of an array of paths reads a constant of its scope"
