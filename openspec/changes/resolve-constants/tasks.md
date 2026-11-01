# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `design.md`, `tasks.md` and the spec deltas

## 2. The core

- [x] 2.1 `crates/core/src/connector.rs`: `Linked { facts, unread }`, and `link` returns it
- [x] 2.2 `crates/core/src/engine/build.rs`, `pipeline.rs`: the unread sites of the link count in coverage and show in the output
- [x] 2.3 `crates/core/src/engine/testing.rs`: the test connector reads `unread` lines
- [x] 2.4 `crates/lang-python`, `crates/lang-rust`, `crates/contracts`: the signature of `link`

## 3. The analyzer

- [x] 3.1 `crates/lang-ts/src/analyze/constants.rs`: the constants of a file and the names it imports
- [x] 3.2 `crates/lang-ts/src/shape.rs`: a name read against the constants of the file, a template or a concatenation of them, and the names kept for the link
- [x] 3.3 `crates/lang-ts/src/analyze.rs`, `analyze/makers.rs`, `analyze/symbols.rs`: constants read wherever the parse reads text, and the sites deferred to the link
- [x] 3.4 `crates/lang-ts/src/extra.rs`: the exported constants and the deferred sites

## 4. The link

- [x] 4.1 `crates/lang-ts/src/link/deferred.rs`: the constants an import leads to, the rule applied again, and the sites still unread
- [x] 4.2 `crates/lang-ts/src/link.rs`: deferred routes served under the mounts of Express hosts and NestJS classes, and the unread sites returned
- [x] 4.3 `crates/lang-ts/src/project.rs`: `READER_VERSION` 4 to 5

## 5. The scenarios

- [x] 5.1 Indexing: a test for "An unread site from the link is counted"
- [x] 5.2 TypeScript: a test and a fixture for "A constant in the same file is read"
- [x] 5.3 TypeScript: a test and a fixture for "An imported enum member names an environment variable"
- [x] 5.4 TypeScript: a test and a fixture for "A static property and a nested object property are read"
- [x] 5.5 TypeScript: a test and a fixture for "A constant imported through re-exports or a namespace is read"
- [x] 5.6 TypeScript: a test and a fixture for "An imported constant path of an Express router is served under its mount"
- [x] 5.7 TypeScript: a test and a fixture for "An imported constant path of a controller is served behind the global prefix"
- [x] 5.8 TypeScript: a test and a fixture for "An imported constant path on a base class is served by its controllers"
- [x] 5.9 TypeScript: a test and a fixture for "A name that is no constant leaves the site unread"
- [x] 5.10 TypeScript: "A route whose path is computed is a blind spot", "A computed configuration name is a blind spot" and "A computed path on a base class is an unread site" on fixtures that name a call or a parameter

## 6. The documents

- [x] 6.1 `docs/architecture.md`: the link returns the sites it could not read
