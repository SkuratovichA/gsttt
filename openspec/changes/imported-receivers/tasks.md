# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The parse

- [x] 2.1 `crates/lang-ts/src/analyze/constants.rs`: the names an import binds keep the specifier they are imported from, and the exported locals are found in one helper
- [x] 2.2 `crates/lang-ts/src/analyze/makers.rs`: `export default <call>` is a maker named `default`, and the makers a call rule takes a receiver from and the file exports are listed
- [x] 2.3 `crates/lang-ts/src/analyze.rs`: a call on a name imported from a file of the project is deferred to the link, for the rules with a receiver and as a fallback for the rules without one
- [x] 2.4 `crates/lang-ts/src/extra.rs`: `ExportedMaker`, `DeferredTarget::Receiver` and `DeferredTarget::Fallback`
- [x] 2.5 `crates/lang-ts/src/project.rs`: `READER_VERSION` 5 to 6

## 3. The link

- [x] 3.1 `crates/lang-ts/src/link/deferred.rs`: the export a name reaches is found in one place for constants and receivers, a receiver is read on what the file that makes it made, and the fallback is dropped when the name is such a receiver
- [x] 3.2 `crates/lang-ts/src/link.rs`: a route registered on a router another file makes is served under the mounts of that router

## 4. The scenarios

- [x] 4.1 TypeScript: a test and a fixture for "A client made in another file is called"
- [x] 4.2 TypeScript: a test and a fixture for "A router made in another file serves under its mounts"
- [x] 4.3 TypeScript: a test and a fixture for "A call on an imported name that no rule makes is silent"
- [x] 4.4 TypeScript: a test and a fixture for "A call on the default import of a file that makes a client is the client's"
- [x] 4.5 TypeScript: tests for an imported constant URL on an imported client, a base URL imported into the file that makes the client, a router exported by default without a name, a call on a package or on Node, and a member of an imported client
- [x] 4.6 CLI: a changed `baseURL` of an exported client moves the key of an importer that is reused

## 5. The documents

- [x] 5.1 `crates/lang-ts/src/provides.rs`, `README.md` and `docs/architecture.md`: what is read and what is not, and the snapshots that list the blind spots
