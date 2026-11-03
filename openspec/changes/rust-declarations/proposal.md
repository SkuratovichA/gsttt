# Proposal: Record the declarations of Rust files

## Why

The Rust connector records no declarations, so `gsttt impact a.rs --name foo` lists every importer of `a.rs` for any name and follows the file whole, and `--changed` walks each changed Rust file whole. TypeScript, Go and Python answer the same questions by name. Rust should answer them the same way, with the core walk that already works from the declarations a connector records.

## What Changes

- The Rust connector records, for a file, its declarations with their lines (functions, structs, enums, unions, traits, type aliases, constants, statics and `macro_rules!`, inline modules flattened into the file), their members as `Type.m` for the items of `impl Type` and `impl Trait for Type`, as `Trait.m` for the items of a trait, as `Enum.Variant` and as `Struct.field`, what each declaration uses, the member names read on a value of unknown type, the names the file exports, and the types whose members a name takes, as heirs and overrides.
- A path in code is a use: `foo`, `Foo`, `Foo::bar` as `Foo.bar`, `self.m` and `Self::m` as `Type.m`, `<Foo as Tr>::m` as `Foo.m` and `Tr.m`, the names of `derive`, and the paths and `.member` reads inside the arguments of a macro. A name bound by a pattern or a parameter of the declaration is no use, except as the callee of a call.
- A method call or a field read on a value whose type the code does not write down records `.m` in the declaration and `m` in the reads of the file, so the walk finds a declaration `T.m` through any importer of its file. A receiver with a written type (`self`, `self.f` with the field type declared in the file, a parameter or `let` with a type, a struct literal) names its type, and `T::m` names `T.m` alone.
- An import lists the name it binds in `exported` and the item it takes in `names`; a glob is a `Static` import of `*`, and a `pub use` glob is both that and the re-export of everything. `use a::Tr as _` binds a name nothing uses and still answers through `.m`. An import of a module is asked a name and gives nothing.
- A file with syntax errors from which the parser recovered records no declarations and no exports and is followed whole.
- A Rust file has a code for `--changed`, with comments and `use` items blanked and the spaces of a string marked, so `--changed` names the changed function and reaches its callers, and not every importer of the file.
- Nothing is left without declarations except a file read past errors, so the requirements that named Rust say a file read past syntax errors.
- A type parameter with a bound is read as the trait: `x: T` with `T: Tr` or `where T: Tr`, and `T::m(&x)`, use `Tr.m`.
- An import nothing names is there for a trait's methods or a macro, because the compiler warns otherwise: the file uses it as it loads, and a member a value of a written type does not declare in the file is then read by name too. Without such an import a value of a written type reads only its own type's member.
- A path with segments past the module it reached goes on through what the file brings in: `pub extern crate a as b`, `pub use a::b`, a glob, in that crate's own dependencies. `grep::cli::X` and `clap::builder::X` land on the module that holds the name.
- The core: asked about an implementation of a head that more than 32 declarations override, a file counts only when it names the member of the head, and a class that takes the members of the head is one more sibling, so one `impl IntoResponse` out of 70 does not list every `.into_response()` in the repository; a member a file declares on a class it imports reaches the holders of that class; a re-export by name beats a star only for what its file hands out, so a derive macro leaves the trait of that name to the star; a private glob passes a name to a glob below it.
- Limits: a value whose type the file does not name, `self.config.style.changes()`, reaches a member only by a file that holds the class. A field of a type parameter, `self.inner.m()` with `inner: T`, is read by name unless the struct itself bounds `T`. A type parameter with several traits is read as the first. A name a parent imports by name does not reach a `use super::*` below it.
- Versions: the `rust` connector goes from 5 to 6, because the facts of every Rust file hold declarations, exports and imports with names. `FACT_FORMAT` stays: the core changes are at query time and the facts of Rust files are read again anyway.

## Impact

- Capability `rust`: declarations, members, uses, receivers, names of an import, exports, heirs and overrides, syntax errors and the code of a Rust file are new requirements.
- Capability `queries`: "Names in files that record no declarations" is replaced by a requirement that names a file read past syntax errors; a member use of unknown receiver and a glob import are new.
- Capability `cli`: "Languages without declarations walk the file whole" is replaced, and a changed Rust function reaching its callers is new.
- `crates/core/src/fact.rs`, `crates/core/src/query/walk/names.rs`, `crates/core/src/query/impact.rs`, `crates/lang-rust/src` (`symbols.rs`, `symbols/`, `code.rs`, `imports.rs`, `project.rs`, `provides.rs`, `resolve.rs`, `tree.rs`), `docs/architecture.md` and `plugin/skills/gsttt/SKILL.md`.
- A Rust repository answers a question about a name with the files that use the name, and `--changed` with the callers of the changed functions. A method called on a value of a type the code does not write counts by its name, which the provides say.
