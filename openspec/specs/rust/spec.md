# rust Specification

## Purpose
The Rust connector reads Rust into imports and contract sites. It parses with `ra_ap_syntax`, builds each crate's module tree, resolves `use` paths and qualified paths through it and through the workspace's crates, and finds HTTP routes and requests, tables, embedded SQL and environment variables through rule packs.

## Requirements

### Requirement: Projects
The connector SHALL open one project per `Cargo.toml` in the file list. A project SHALL claim the `rs` files under its root and its `Cargo.toml`, which it tags as a manifest.

#### Scenario: Every manifest is a project
- **WHEN** a repository has a virtual `Cargo.toml` at the root, `crates/core/Cargo.toml` and `crates/cli/Cargo.toml`
- **THEN** the connector opens projects rooted at the repository root, `crates/core` and `crates/cli`
- **AND** `crates/core/src/lib.rs` belongs to `crates/core`

### Requirement: Crate targets
A project with a `[package]` table SHALL know its crate targets: `src/lib.rs`, `src/main.rs`, `src/bin/*.rs`, `src/bin/*/main.rs`, `examples/*.rs`, `tests/*.rs`, `benches/*.rs` and `build.rs` when they exist, and the targets its manifest names with a `path`.

#### Scenario: Explicit targets are crate roots
- **WHEN** `crates/cli/Cargo.toml` has `[[bin]] name = "tool" path = "src/tool/entry.rs"` and `src/tool/entry.rs` has `mod args;`
- **THEN** the module import resolves to `crates/cli/src/tool/args.rs`

### Requirement: Workspace dependencies
A dependency declared with `workspace = true` SHALL take its package from `[workspace.dependencies]` of the workspace root.

#### Scenario: A workspace dependency takes its package from the root
- **WHEN** the root `Cargo.toml` has `[workspace.dependencies]` with `engine = { package = "gsttt-core", path = "crates/core" }`, `crates/web/Cargo.toml` has `engine = { workspace = true }`, and `crates/web/src/main.rs` imports `engine::graph::Graph`
- **THEN** the import resolves to `crates/core/src/graph.rs`

### Requirement: Dependency visibility
A target SHALL see the dependencies of `[dependencies]`, `[dev-dependencies]` and their `[target.*]` forms, except the build script, which SHALL see only `[build-dependencies]`. Dev-dependencies are seen by every target but the build script, because `cfg` is not evaluated and `#[cfg(test)]` code in `src/` uses them. A path to a crate that no visible dependency names SHALL be an unknown package.

#### Scenario: A build script sees only build dependencies
- **WHEN** a package declares `serde` in `[dependencies]`, `proptest` in `[dev-dependencies]` and `cc` in `[build-dependencies]`, `build.rs` imports `cc::Build`, `serde::Serialize` and `proptest::collection::vec`, and `src/lib.rs` imports `serde::Serialize` and `cc::Build`
- **THEN** in `build.rs` the first import resolves to the package `cc` and the other two are unknown packages
- **AND** in `src/lib.rs` the first resolves to the package `serde` and the second is an unknown package

#### Scenario: A dev-dependency is seen by every other target
- **WHEN** the same package has `src/lib.rs` import `proptest::prelude::Strategy` and `tests/it.rs` import `proptest::collection::vec` and `cc::Build`
- **THEN** the two `proptest` imports resolve to the package `proptest`
- **AND** `cc::Build` in `tests/it.rs` is an unknown package

### Requirement: Library names
A dependency SHALL be named by its key in the manifest, with `_` where the key has `-`. A path dependency that is not renamed SHALL be named by the `[lib] name` of its own manifest when that sets one, and not by its key.

#### Scenario: A library name replaces the package name
- **WHEN** the package `my-util` sets `[lib] name = "helpers"`, `app` depends on it as `my-util = { path = "../util" }`, and `app/src/lib.rs` imports `helpers::text::trim` and `my_util::text::trim`
- **THEN** the first resolves to `util/src/text.rs` and the second is an unknown package

### Requirement: Module tree
`mod <name>;` SHALL be an import of kind module that resolves, from a crate root or a `mod.rs`, to `<name>.rs` or `<name>/mod.rs` in the same directory, and from any other file `<dir>/<file>.rs` to `<dir>/<file>/<name>.rs` or `<dir>/<file>/<name>/mod.rs`; inline modules SHALL nest the directory the same way, and a `#[path = "…"]` attribute SHALL name the file relative to the declaring file's directory. Each project SHALL build the module tree of each of its crate targets from these imports.

#### Scenario: Module declarations find their files
- **WHEN** `src/lib.rs` has `mod a; mod b;`, `src/a.rs` has `mod c;`, and the files `src/a.rs`, `src/b/mod.rs` and `src/a/c.rs` exist
- **THEN** the three module imports resolve to `src/a.rs`, `src/b/mod.rs` and `src/a/c.rs`

#### Scenario: A path attribute names the file
- **WHEN** `src/lib.rs` has `#[path = "generated/schema.rs"] mod schema;`
- **THEN** the module import resolves to `src/generated/schema.rs`

#### Scenario: A missing module file is not found
- **WHEN** `src/lib.rs` has `mod gone;` and neither `src/gone.rs` nor `src/gone/mod.rs` exists
- **THEN** the import is unresolved as not found

### Requirement: Module declarations
A `#[path = …]` attribute SHALL be read when its string is raw and when it sits inside `#[cfg_attr(…)]`, and a `mod x;` inside `cfg_if!` SHALL be a module declaration. A module declared again in another file, as one `mod` is under different `cfg`s, SHALL give that file a module of its own, while a path into the module reaches the first declaration. A file that no declaration reaches SHALL have no module.

#### Scenario: A path attribute may be a raw string or sit in cfg_attr
- **WHEN** a file has each of `#[path = r"raw.rs"] mod a;`, `#[path = r#"hash.rs"#] mod a;` and `#[cfg_attr(unix, path = "unix.rs")] mod a;`
- **THEN** the module imports name the files `raw.rs`, `hash.rs` and `unix.rs`

#### Scenario: A module declared inside cfg_if is read
- **WHEN** a file has `cfg_if::cfg_if! { if #[cfg(unix)] { mod unix; } else { mod win; } }`
- **THEN** it holds module imports for `unix` and `win`

#### Scenario: A module declared twice gives both files a module
- **WHEN** `src/lib.rs` has `#[cfg(unix)] #[path = "unix.rs"] mod sys;`, `#[cfg(windows)] #[path = "win.rs"] mod sys;` and `pub use sys::run;`, and `src/unix.rs` and `src/win.rs` each import `crate::util::shared`
- **THEN** `sys::run` resolves to `src/unix.rs`, and `crate::util::shared` resolves to `src/util.rs` from both files
- **AND** `super::util::shared` in `src/win.rs` resolves to `src/util.rs`
- **AND** the project names a module declared twice among its blind spots

#### Scenario: A file no declaration reaches has no module
- **WHEN** `src/orphan.rs` is declared by no `mod` and imports `crate::util::shared` and `serde::Serialize`, and the package declares `serde`
- **THEN** the first import is unresolved as not found and the second resolves to the package `serde`

### Requirement: Imports
The connector SHALL read `use` declarations, one import per leaf of a use tree, with `pub use` as a re-export and a glob as a re-export of everything when it is public; `extern crate`; qualified paths of two or more segments in code that start with `crate`, `self`, `super` or the name of a crate, including paths inside macro invocations, read from their tokens; and `include!`, `include_str!` and `include_bytes!` with a literal path as includes.

#### Scenario: Every import form is read
- **WHEN** a file holds one import of each form listed above
- **THEN** its facts hold each import with its kind, line and names, matching the reviewed snapshot

#### Scenario: Paths in macros are read
- **WHEN** a file has `assert_eq!(crate::a::helper(), 1)` and no `use` of `crate::a`
- **THEN** its facts hold an import of `crate::a::helper` at that line

### Requirement: Macros re-exported by name
A one-segment `use name;`, public or not, SHALL read no import when the module that holds it declares `macro_rules! name` before it, because the line makes the macro addressable by a path and reaches no other file. The same `use` in a module that declares no such macro, or declares it later, SHALL be read as any other import.

#### Scenario: A use of a macro declared in the file reads no import
- **WHEN** `src/render.rs` has `macro_rules! shout { () => {}; }` followed by `pub(crate) use shout;`
- **THEN** its facts hold no import for that `use`
- **AND** a `use shout;` in a module that declares no such macro before it is read as an import of `shout`

### Requirement: Derives
A `#[derive(…)]`, also inside `#[cfg_attr(…)]`, SHALL be read for the crates that its paths name.

#### Scenario: A derive inside cfg_attr names its crate
- **WHEN** a file has `#[cfg_attr(feature = "x", derive(serde::Serialize, Debug))] struct S;`
- **THEN** its facts hold an import of `serde::Serialize`

### Requirement: Conditional imports
Imports inside an item or module under `#[cfg(test)]` SHALL be flagged test-only, and imports under any other `#[cfg(…)]` optional.

#### Scenario: Conditional imports are flagged
- **WHEN** a file has `#[cfg(test)] mod tests;`, `#[cfg(feature = "cli")] use crate::cli::Args;` and `#[cfg(test)] mod inline { use super::Thing; }`
- **THEN** the first and third imports are flagged test-only and the second optional

### Requirement: Parse status
A file with syntax errors SHALL be recorded as recovered with the number of errors and the line of the first, keeping what was read.

#### Scenario: A recoverable error keeps the imports
- **WHEN** a file has `use crate::a::B;` on line 1 and an unclosed brace on its last line
- **THEN** its status is recovered and its facts still hold the import of `crate::a::B`

### Requirement: Test files
Files under a `tests` directory of a package, and files named `tests.rs`, `test.rs`, `*_test.rs` or `*_tests.rs`, SHALL be tagged as tests.

#### Scenario: Test files are tagged
- **WHEN** a package holds `tests/api.rs`, `src/a/tests.rs`, `src/parse_tests.rs` and `src/a.rs`
- **THEN** the first three are tagged as tests and `src/a.rs` is not

### Requirement: Resolution
A path SHALL resolve through the module tree of the crate the file belongs to: `crate` from the crate root, `self` from the file's module, `super` from its parent, and a leading segment that the same module brings in with `use` through that `use`. An include SHALL resolve against the directory of its file, and an `include!` of a `concat!` that starts with `env!("OUT_DIR")` SHALL be an import unresolved as generated.

#### Scenario: Aliases resolve through their use
- **WHEN** a file has `use crate::a::c as cc;` and calls `cc::Thing::new()`
- **THEN** the path in the call resolves to `src/a/c.rs`

#### Scenario: Includes resolve beside the file
- **WHEN** `src/a.rs` has `include_str!("../fixtures/a.json")` and `include!(concat!(env!("OUT_DIR"), "/gen.rs"))`
- **THEN** the first resolves to `fixtures/a.json` in the package and the second is unresolved as generated

### Requirement: Editions
The edition of a package SHALL be the `edition` of its `[package]` table, that of `[workspace.package]` when the table says `edition.workspace = true`, and 2015 when there is none. In 2015 a `use` path SHALL start at the crate root, as one with a leading `::` does there; from 2018 a leading `::` SHALL start at a crate, and any other `use` path at the module.

#### Scenario: A crate without an edition is read as 2015
- **WHEN** two crates have the same files, `src/a.rs`, `src/sub/a.rs` and a `src/sub.rs` with `mod a;` and `use a::b;`, and one has no `edition` key while the other has `edition = "2021"`
- **THEN** the `use` in the first resolves to `src/a.rs` and the one in the second to `src/sub/a.rs`

#### Scenario: A workspace edition is inherited
- **WHEN** a member has `edition.workspace = true`, `[workspace.package]` of the root says `edition = "2015"`, and the member has `src/a.rs` and a `src/sub.rs` with `mod a;` and `use a::b;`
- **THEN** the `use` starts at the crate root and resolves to `src/a.rs`

#### Scenario: A leading double colon starts at the crate root in 2015 and at a crate after
- **WHEN** a 2015 crate and a 2021 crate both declare `serde` and have a module `a` at their roots, and a file of each has `use ::a::b;` and `use ::serde::Serialize;`
- **THEN** in the 2015 crate the first resolves to `src/a.rs` and in the 2021 crate it is an unknown package
- **AND** the second resolves to the package `serde` in both

### Requirement: Resolution order
The first segment of a path not starting with `crate`, `self`, `super`, `::` or a name the file's module brings in with `use` SHALL be tried, in order, as a child module of the file's module, as a crate (the package's own library from its other targets, a visible dependency, or `std`, `core`, `alloc`, `proc_macro` or `test`), and as a module at the crate root, which stands in for the names that `use super::*` and `use crate::…::*` bring in. A path that matches none SHALL be an unknown package.

#### Scenario: Targets of a package see its library
- **WHEN** the package `gsttt-core` has `tests/engine.rs` with `use gsttt_core::index::Index;`
- **THEN** the import resolves to `crates/core/src/index.rs`

#### Scenario: Dependencies, builtins and unknown names are told apart
- **WHEN** a package declares `serde` directly and `tokio = { workspace = true }`, and a file imports `serde::Deserialize`, `tokio::sync::Mutex`, `std::collections::HashMap` and `rand::random`
- **THEN** the imports resolve to the packages `serde` and `tokio`, a builtin and an unknown package

### Requirement: Glob imports
A name that a glob import brings in SHALL not be known, because no item of a dependency is read: a path that starts with one SHALL be an unknown package, and every project SHALL name such names among its blind spots.

#### Scenario: A name a glob import brings in is an unknown package
- **WHEN** a file has `use proptest::prelude::*;` and a path `prop::collection::vec`
- **THEN** the import of `proptest::prelude` resolves to the package `proptest` and `prop::collection::vec` is an unknown package
- **AND** the blind spots of every project name the names that a glob import brings in

### Requirement: Deepest module
Within a crate a path SHALL resolve to the file of the deepest module it names.

#### Scenario: Paths stop at the deepest module
- **WHEN** `src/lib.rs` has `mod a;`, `src/a.rs` has `mod c;` and `pub struct Helper;`, and the files `src/lib.rs`, `src/a.rs` and `src/a/c.rs` hold `use crate::a::c::Thing;`, `use self::c::Thing;` and `use super::Helper;`
- **THEN** the first two resolve to `src/a/c.rs` and the third to `src/a.rs`

### Requirement: Paths into dependencies
A path into a workspace crate SHALL continue through that crate's module tree, wherever its `path` leads, the repository root included, and a path into any other dependency SHALL resolve to that package.

#### Scenario: Workspace crates resolve to their modules
- **WHEN** `crates/cli` depends on `gsttt-core` by path and imports `gsttt_core::index::Index`, and `crates/web` depends on it as `engine = { package = "gsttt-core", path = "../core" }` and imports `engine::graph::Graph`
- **THEN** the imports resolve to `crates/core/src/index.rs` and `crates/core/src/graph.rs`

#### Scenario: A path dependency may be the repository root
- **WHEN** the root package `mycrate` has `pub mod parser;`, `fuzz/Cargo.toml` has `mycrate = { path = ".." }` and `fuzz/src/main.rs` imports `mycrate::parser::run`
- **THEN** the import resolves to `src/parser.rs`

### Requirement: HTTP routes
With the `axum` pack, `route("<path>", <method router>)` on a receiver the same file shows to be a `Router`, by `Router::new()`, `Router::default()` or its type, SHALL serve `http <METHOD> <path>` for each of `get`, `post`, `put`, `patch`, `delete`, `head`, `options` and `trace` in the method router, and `*` for `any`.

#### Scenario: An axum route serves each of its methods
- **WHEN** a file has `Router::new().route("/bookings/{id}", get(show).delete(destroy))`
- **THEN** it serves `http GET /bookings/{}` and `http DELETE /bookings/{}` at the line of the `route` call

#### Scenario: A route on another receiver is no route
- **WHEN** a file has `Router::new().route("/health", get(health))` and `planner.route("/not-a-route", get(health))`, and `planner` is a parameter of type `&Planner`
- **THEN** it serves `http GET /health`
- **AND** the file serves nothing at the second call

### Requirement: Nested routers
With the `axum` pack, `nest("<prefix>", <router>)` SHALL serve the nested router's routes under the prefix and `merge(<router>)` without one, where the router is a local binding or a call to a function returning a router, in this file or another, through any number of levels.

#### Scenario: A nested router in another file serves under the prefix
- **WHEN** `src/bookings.rs` has `pub fn router() -> Router { Router::new().route("/{id}", get(show)) }` and `src/main.rs` has `Router::new().nest("/api/bookings", bookings::router())`
- **THEN** `src/bookings.rs` serves `http GET /api/bookings/{}` at the line of its `route` call

### Requirement: Routes under every mount
A route SHALL be served once under each mount of its router and under no other path. `nest` mounts a router under a prefix and `merge` mounts it at the root, so a merged router serves its routes as written, and so does a router that nothing mounts. A route below a mount that a cap cuts keeps the path it is written with.

#### Scenario: A nested router serves only under its prefix
- **WHEN** `nested.rs` has `fn router() -> Router { Router::new().route("/x", get(x)) }` and `Router::new().nest("/api", router())`
- **THEN** `nested.rs` serves `http GET /api/x` at the line of the `route` call
- **AND** it serves nothing under `http GET /x`

#### Scenario: A merged router serves at the root
- **WHEN** `merged.rs` has the same `router` and `Router::new().merge(router())`
- **THEN** `merged.rs` serves `http GET /x` at the line of the `route` call and nothing else

#### Scenario: A router both merged and nested serves under both
- **WHEN** `both.rs` has the same `router` and `Router::new().nest("/api", router()).merge(router())`
- **THEN** `both.rs` serves `http GET /api/x` and `http GET /x`, both at the line of the `route` call

#### Scenario: A route below a mount cut keeps the path it is written with
- **WHEN** routers are mounted one in the next 64 levels deep or more, and each serves `GET /r`
- **THEN** the routers within the limit serve `/r` under the prefixes they are mounted at
- **AND** the routers below the limit serve `http GET /r`

### Requirement: Walk and module caps
Following nested routers SHALL give every router that nothing mounts a budget of its own of 64 levels and 50000 visits, and a crate target SHALL be read up to 10000 modules. A walk or a tree that a cap cuts SHALL be logged with the cap, and every project SHALL name the caps among its blind spots.

#### Scenario: Each router that starts a walk has a budget of its own
- **WHEN** a router that nothing mounts mounts the next one twice at each of 40 levels, and another router that nothing mounts serves `/ok` and has the prefix `/v2` from its scope
- **THEN** the second serves `http GET /v2/ok`, though the walk from the first used up its visits

#### Scenario: A walk that a cap cuts is reported
- **WHEN** routers are mounted one in the next more than 64 levels deep, or each of 40 levels mounts the next twice
- **THEN** the walk is cut and reported with the router it started from and the cap, depth or visits
- **AND** a walk within both caps is not reported

#### Scenario: A tree that the module cap cuts keeps the modules before it
- **WHEN** `src/lib.rs` declares `mod user;` and then `src/a.rs` under 22 names with `#[path]`, `src/a.rs` declares `src/b.rs` under 22 names, `src/b.rs` declares `src/c.rs` under 22 names, and `src/user.rs` has `use crate::a0::b0::c0::item;` and `use crate::a21::b21::c21::item;`
- **THEN** the first path resolves to `src/c.rs`
- **AND** the second stops at the deepest module before the cut and resolves to `src/lib.rs`

#### Scenario: A cap is named among the blind spots
- **WHEN** the coverage of a Rust project is read, with or without packs
- **THEN** its http serves blind spots name routers nested more than 64 deep and entered more than 50000 times
- **AND** every entry names a crate target with more than 10000 modules

### Requirement: Actix routes
With the `actix` pack, the attributes `#[get]`, `#[post]`, `#[put]`, `#[patch]`, `#[delete]`, `#[head]` and `#[route(…, method = …)]` on a function SHALL serve their path, `web::resource("<path>").route(web::<verb>().to(…))` SHALL serve the path with the verb, and `web::scope("<prefix>").service(<handler>)` SHALL serve the handler's routes under the prefix, across files.

#### Scenario: An actix scope prefixes a handler from another file
- **WHEN** `src/bookings.rs` has `#[get("/{id}")] async fn show()` and `src/main.rs` has `web::scope("/bookings").service(bookings::show)`
- **THEN** `src/bookings.rs` serves `http GET /bookings/{}` at the line of the attribute

#### Scenario: An actix route attribute serves the TRACE method
- **WHEN** `src/main.rs` has `#[route("/x", method = "TRACE")] async fn x()`
- **THEN** `src/main.rs` serves `http TRACE /x` at the line of the attribute

### Requirement: HTTP requests
With the `reqwest` pack, `reqwest::get(<url>)` SHALL call `http GET <url>`, and `get`, `post`, `put`, `patch`, `delete` and `head` with a URL, and `request(Method::<VERB>, <url>)`, SHALL call the URL with the verb on a receiver bound in the same file to `Client::new()`, `Client::builder()` or `ClientBuilder::new()`, with or without a chain, or a `self` field or parameter whose type in the same file is `Client`.

#### Scenario: A client held in a field calls the URL
- **WHEN** a file has `struct Api { http: reqwest::Client, base: String }` and a method calling `self.http.get(format!("{}/bookings/{}", self.base, id))`
- **THEN** it calls `http GET /bookings/{}`

#### Scenario: A client builder makes a client
- **WHEN** a file has `let client = ClientBuilder::new().build().unwrap();` and `client.get("https://api.example.com/items")`
- **THEN** it calls `http GET /items`

#### Scenario: A get on another receiver is no request
- **WHEN** a file has `cache.get("/bookings")` and `cache` is a `HashMap`
- **THEN** the file has no contract site at that line

### Requirement: Request URLs
With the `reqwest` pack, a URL from `format!` SHALL keep its literal parts with each placeholder as a parameter; a URL with no literal path SHALL be an unread site.

#### Scenario: Built URLs keep their literal parts and bare URLs are unread
- **WHEN** a file has `let client = reqwest::Client::new();`, `client.delete(format!("/bookings/{}", id))` on line 12 and `client.get(url)` on line 13
- **THEN** it calls `http DELETE /bookings/{}` at line 12
- **AND** the file has an unread site at line 13

### Requirement: Imported function names
A call whose callee matches no rule of the active packs SHALL be tried again with its first segment replaced by the path that a `use` of the same file gives it, so `get(url)` after `use reqwest::get;` is `reqwest::get(url)`. A name that two imports give different paths SHALL be left alone.

#### Scenario: A function a use brought in is matched by its full path
- **WHEN** a file has `use reqwest::get;`, `use std::env::var;`, `get("https://api.example.com/imported")` and `var("IMPORTED")`
- **THEN** it calls `http GET /imported` and `env IMPORTED`

### Requirement: Tables and SQL
With the `sqlx` pack, the SQL argument of `query!`, `query_as!`, `query_scalar!`, their `_unchecked` forms, and of the functions `query`, `query_as` and `query_scalar`, SHALL be an embedded `sql` document when it is a literal, and `query_file!` and `query_file_as!` SHALL be includes resolving from the package root. With the `sea_orm` pack, `#[sea_orm(table_name = "<name>")]` SHALL declare `table <name>`.

#### Scenario: Query macros embed SQL
- **WHEN** a file has `sqlx::query_as!(Booking, "SELECT id FROM bookings WHERE id = $1", id)` on line 9
- **THEN** it embeds an `sql` document at line 9 with that text

#### Scenario: Query files are includes
- **WHEN** a file in the package `crates/db` has `sqlx::query_file!("queries/find.sql", id)`
- **THEN** it has an include that resolves to `crates/db/queries/find.sql`

#### Scenario: An entity declares its table
- **WHEN** a struct has `#[sea_orm(table_name = "bookings")]`
- **THEN** the file declares `table bookings`

### Requirement: Environment variables
With the `env` pack, `env::var`, `env::var_os`, `std::env::var`, `dotenvy::var`, `env!` and `option_env!` with a literal name, and a clap `env = "<NAME>"` argument in `#[arg(…)]` or `#[clap(…)]`, SHALL call `env <NAME>`.

#### Scenario: Every way of reading the environment is seen
- **WHEN** a file has `env::var("A")`, `std::env::var_os("B")`, `env!("C")`, `option_env!("D")` and a field with `#[arg(long, env = "E")]`
- **THEN** it calls `env A`, `env B`, `env C`, `env D` and `env E`

### Requirement: Pack activation
A pack SHALL be active in a project when its manifest declares one of the pack's `detect` crates in `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]` or a target-specific table, directly or through the workspace, or always when its `detect` is empty. Each project SHALL report what it provides, with its blind spots in words.

#### Scenario: Packs follow the manifest
- **WHEN** `crates/api` declares `axum` and `crates/cli` does not
- **THEN** `route` calls in `crates/api` produce sites and the same calls in `crates/cli` do not

#### Scenario: Blind spots are reported
- **WHEN** the coverage of a Rust project is read
- **THEN** its provides name, among the blind spots, actix `configure`, tonic services, Diesel `table!`, `macro_rules!` bodies, mounts written `Type::router()` and routers passed as values the connector cannot follow
