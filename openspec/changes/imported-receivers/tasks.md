# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The parse

- [x] 2.1 `crates/lang-ts/src/analyze/constants.rs`: the names an import binds keep the specifier they are imported from, and the exported locals are found in one helper
- [x] 2.2 `crates/lang-ts/src/analyze/makers.rs`: `export default <call>` is a maker named `default`, and the makers a call rule takes a receiver from and the file exports are listed
- [x] 2.3 `crates/lang-ts/src/analyze.rs`: a call on a name imported from a file of the project is deferred to the link, for the rules with a receiver and as a fallback for the rules without one
- [x] 2.4 `crates/lang-ts/src/extra.rs`: `ExportedMaker`, `DeferredTarget::Receiver` and `DeferredTarget::Fallback`
- [x] 2.5 `crates/lang-ts/src/project.rs`: `READER_VERSION` 5 to 6
- [x] 2.6 `crates/lang-ts/src/analyze/makers.rs`: a maker declared at the top level of the module wins over one of the same name declared in a function, a block or a class, and only a top-level maker is exported
- [x] 2.7 `crates/lang-ts/src/analyze.rs`: the guard that a route has a handler after its path, and the test of whether a specifier is a library, are one function each

## 3. The link

- [x] 3.1 `crates/lang-ts/src/link/deferred.rs`: the export a name reaches is found in one place for constants and receivers, a receiver is read on what the file that makes it made, and the fallback is dropped when the name is such a receiver
- [x] 3.2 `crates/lang-ts/src/link.rs`: a route registered on a router another file makes is served under the mounts of that router
- [x] 3.3 `crates/lang-ts/src/link.rs` and `link/deferred.rs`: the step from an import to the export it reaches is one function for `define` and for the names a deferred site is written on, and `apply` reads a call rule through `call_rule`

## 4. The scenarios

- [x] 4.1 TypeScript: a test and a fixture for "A client made in another file is called"
- [x] 4.2 TypeScript: a test and a fixture for "A router made in another file serves under its mounts"
- [x] 4.3 TypeScript: a test and a fixture for "A call on an imported name that no rule makes is silent"
- [x] 4.4 TypeScript: a test and a fixture for "A call on the default import of a file that makes a client is the client's"
- [x] 4.5 TypeScript: tests for an imported constant URL on an imported client, a base URL imported into the file that makes the client, a router exported by default without a name, a call on a package or on Node, and a member of an imported client
- [x] 4.6 CLI: a changed `baseURL` of an exported client moves the key of an importer that is reused
- [x] 4.7 TypeScript: a test and a fixture for "A client made inside a function does not replace the one a file exports", and tests for a variable another call makes inside a function and for a client made inside a function in a file whose export is not a client
- [x] 4.8 TypeScript: a test and a fixture for "A call on an imported name is read by the rule of the call that made it"
- [x] 4.9 TypeScript: tests for an imported router that nothing mounts, a client behind a barrel, a client taken through a namespace import, a client exported by default through its name and an unreadable URL on an imported client; unit tests of what the parse defers and of which specifiers are libraries

## 5. The documents

- [x] 5.1 `crates/lang-ts/src/provides.rs`, `README.md` and `docs/architecture.md`: what is read and what is not, what a router that no read mount reaches and a name inside a function are read as, and the snapshots that list the blind spots
