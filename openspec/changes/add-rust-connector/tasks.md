# Tasks

## 1. Reading files

- [x] 1.1 `Connector` for `rust`: detection, projects per `Cargo.toml`, crate targets, workspace inheritance, options with a JSON schema
- [x] 1.2 Parsing with `ra_ap_syntax`: module declarations, use trees, `extern crate`, qualified paths in code and macro tokens, includes, cfg flags, test tags, recovered status
- [x] 1.3 A `Site` over calls, method calls, macro calls and attributes, and matching of every rule kind

## 2. Resolution

- [x] 2.1 The module tree per crate target in `prepare`, with `#[path]` and inline modules
- [x] 2.2 Path resolution: `crate`, `self`, `super`, `use` aliases, child modules, crates, root modules; workspace crates through their trees
- [x] 2.3 Dependencies by crate name and renamed key, builtins, unknown packages, includes and generated includes

## 3. Packs and link

- [x] 3.1 Packs: `axum`, `actix`, `reqwest`, `sqlx`, `sea_orm`, `env`
- [x] 3.2 `link`: axum `nest` and `merge`, actix `scope`, across files
- [x] 3.3 `provides` with blind spots per project

## 4. Proof

- [x] 4.1 A test annotated `#[scenario("rust", …)]` for every scenario of the `rust` spec, on fixture trees with reviewed `insta` snapshots
- [x] 4.2 gsttt indexes its own repository with no unresolved import other than those its blind spots name
- [x] 4.3 `cargo nextest run -p gsttt-lang-rust`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass

## 5. Review fixes

- [x] 5.1 Editions: 2015 when the manifest names none, `edition.workspace`, a leading `::`, `use` paths from the crate root in 2015, and why the resolution order ends at a module of the crate root
- [x] 5.2 A path dependency that leads to the repository root resolves to the root package
- [x] 5.3 A visit budget for each router that starts a walk, cuts logged with the router and the cap, the caps named among the blind spots, and a module cap for each crate target
- [x] 5.4 The alias scenario uses a name that only alias expansion can resolve
- [x] 5.5 Snapshots printed with sorted keys, the same with and without `preserve_order`
- [x] 5.6 `macro_call` reads the arguments of the macros that need them only
- [x] 5.7 A module declared twice under different `cfg`s gives both files a module, and a file no declaration reaches has none
- [x] 5.8 Dependency visibility by manifest table, and `[lib] name` in the names of dependencies
- [x] 5.9 Receivers traced from a scope built once for each file
- [x] 5.10 The dogfood test excuses an unknown package only after a glob of a crate outside the repository
- [x] 5.11 `path` as a raw string or inside `cfg_attr`, `derive` inside `cfg_attr`, `cfg_if!` modules, and blind spots for `macro_rules!` bodies and `Type::router()`
- [x] 5.12 Pack gaps: an axum `route` only on a router, `ClientBuilder::new()` as a client, functions brought in by `use` found by their full path
- [x] 5.13 `RepoPath::parent` in the module tree in place of a copy of it
- [x] 5.14 Requirements and scenarios for what the fixes add or change
- [x] 5.15 The known gaps in the proposal

## 6. Integration fixes

- [x] 6.1 A `use` of a macro declared in the same module reads no import, so gsttt reads its own repository, where `crates/core/src/query/render.rs` re-exports `display_as_text` that way, with no unexplained unresolved import
- [x] 6.2 A router is served once under each mount and under no other path: `report` keys each route of a mounted router again with `LinkedFact::rekeyed`, at the root mount too, so the engine drops the bare site and the graph holds the router under its mounts only; scenarios for `nest`, `merge`, both and a cut
- [x] 6.3 `TRACE` is an HTTP method: core lists it, so actix `#[route(.., method = "TRACE")]` serves `http TRACE <path>` and the connector no longer rewrites the key of an axum `trace(..)` route after the rules ran
- [x] 6.4 `cfg_flags` reads only the leading attributes of each ancestor, so a file of 10001 `mod` declarations is indexed in 0.12 s instead of 16 s in a debug build, and a doc comment between two attributes, an inner attribute of a file or of a module body, and a shebang before one are still read
- [x] 6.5 The project salt is made with `salt_of` of the core from the edition and the packs, in place of a hash of its own, so the crate no longer needs `xxhash-rust`
