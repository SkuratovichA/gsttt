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
- [ ] 4.3 `cargo nextest run -p gsttt-lang-rust`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass
