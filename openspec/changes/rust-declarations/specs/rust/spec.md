# Spec Delta

## ADDED Requirements

### Requirement: Declarations of a Rust file
A Rust file SHALL record its functions, structs, enums, unions, traits, type aliases, constants, statics and `macro_rules!`, with inline modules flattened into the file, and the members of each as `Type.member`: the items of `impl Type` and of `impl Trait for Type`, the items of a trait, the variants of an enum and the named fields of a struct. It SHALL export every name it declares at its top level and every name a `pub use` binds.

#### Scenario: A name gets its callers and not every importer
- **WHEN** `a.rs` declares `foo` and `bar`, `b.rs` calls `foo` and `c.rs` imports both and calls `bar`, and impact is asked about `foo` in `a.rs`
- **THEN** impact lists `b.rs` and not `c.rs`

#### Scenario: An impl in another file reaches the callers of its type
- **WHEN** `model.rs` declares `Foo`, `ext.rs` declares `impl Foo { fn extra }` and `app.rs` calls `foo.extra()` on a `&Foo`, and impact is asked about `Foo` in `model.rs`
- **THEN** impact lists `app.rs` among the callers

### Requirement: A path in code is a use
A path in the code of a declaration SHALL be a use of its last name. `Type::member`, `Self::member` and `self.member` SHALL be a use of `Type.member`. A name that a pattern or a parameter of the declaration binds SHALL be no use, except as the callee of a call.

#### Scenario: A path in code is a use of the member it names
- **WHEN** a file calls `Foo::bar()`, `Self::baz()` and `self.baz_ref()` in the methods of `impl Foo`
- **THEN** each declaration uses `Foo.bar`, `Foo.baz` and `Foo.baz_ref` respectively

#### Scenario: A name bound by a pattern is no use
- **WHEN** `b.rs` imports `foo` from `a.rs`, one function of `b.rs` takes a parameter named `foo` and another calls `foo()`
- **THEN** the first function uses nothing and impact about `foo` in `a.rs` lists `b.rs`

### Requirement: A method called on a value of unknown type
A method call or field read on a value whose type the code does not write SHALL record `.member` in the declaration and `member` in the reads of the file, so that a file which imports the type is a caller of `Type.member`. A receiver of a written type SHALL be a use of `Type.member`, also through a type parameter with a bound, and reads a member by name only when a trait in scope is named by nothing.

#### Scenario: A method called on a value of an unknown type counts by its name
- **WHEN** `b.rs` imports `Runner` and calls `value.start()` on a generic `value`, and impact is asked about `Runner.start` and `Runner.stop` in `a.rs`
- **THEN** impact lists `b.rs` for `Runner.start` and no file for `Runner.stop`

#### Scenario: A receiver with a written type names its type
- **WHEN** a method calls `self.cache.get()` where the field `cache` is declared as `Cache` in the file, and a function calls `cache.get()` on a parameter of type `&Cache`
- **THEN** both declarations use `Cache.get` and the file reads no member of an unknown type

#### Scenario: A type parameter is read as the trait that bounds it
- **WHEN** a function calls `x.m()` on `x: T` with `T: Tr` or `where T: Tr`, or calls `T::m(&x)`
- **THEN** it uses `Tr.m`, and a parameter without a bound reads `.m`

#### Scenario: A value of a written type reads a member only with a nameless trait in scope
- **WHEN** `caller.rs` holds `use crate::tr::Tr`, which nothing names, and calls `foo.m()` on a `&Foo`, and `bystander.rs` holds no such import and does the same
- **THEN** `caller.rs` reads `m` and impact about `Tr.m` lists it, and `bystander.rs` reads nothing

#### Scenario: A local rebound to another type reaches both types
- **WHEN** a function binds `x` to an `A` and then to a `B` and calls `x.run()` after each
- **THEN** impact about `A.run` and about `B.run` each list that file

### Requirement: Imports name what they take
A `use` SHALL list the item it takes in `names` and the name it binds in `exported`. A glob SHALL be a static import of `*`, and a `pub use` glob SHALL also re-export every name. A name that a file declares itself SHALL win over one a glob brings in. A variant imported by its path SHALL take its enum, a trait imported `as _` SHALL answer by its method names, and an import nothing names SHALL be used as the file loads.

#### Scenario: A glob import passes the names on
- **WHEN** `b.rs` and `c.rs` hold `use crate::a::*`, `b.rs` calls `foo` and `c.rs` calls `bar`
- **THEN** impact about `foo` in `a.rs` lists `b.rs` and not `c.rs`

#### Scenario: A name a file declares beats the one a glob brings in
- **WHEN** `b.rs` holds `use crate::a::*`, declares its own `foo` and calls it, and impact is asked about `foo` in `a.rs`
- **THEN** impact lists no file

#### Scenario: A pub use glob hands out every name of the module
- **WHEN** `hub.rs` holds `pub use crate::a::*` and `user.rs` takes `foo` from `hub` and calls it
- **THEN** impact about `foo` in `a.rs` lists `user.rs` at distance 1

#### Scenario: A pub use hands out the name it takes
- **WHEN** `hub.rs` holds `pub use crate::a::foo as renamed;` and `use crate::a::foo as private;`
- **THEN** its exports are `renamed` and not `private`

#### Scenario: A variant imported by its path names its enum and itself
- **WHEN** `b.rs` holds `use crate::a::Color::Red` and uses `Red`
- **THEN** impact about `Color.Red` in `a.rs` lists `b.rs` and about `Color.Blue` lists no file

#### Scenario: An import nothing names is used as the file loads
- **WHEN** `nameless.rs` holds `use crate::tr::Tr`, `under.rs` holds `use crate::tr::Tr as _`, `standard.rs` holds `use std::fmt::Write`, and none of them names it
- **THEN** the first two use it as they load and impact about `Tr` lists them, and `standard.rs` uses nothing as it loads

#### Scenario: A glob of a parent passes on what a glob of it brings in
- **WHEN** `assets.rs` holds `use crate::error::*` and its child `assets/theme.rs` holds `use super::*` and uses `Failure` of `error.rs`
- **THEN** impact about `Failure` in `error.rs` lists both

#### Scenario: A path goes on through what a crate re-exports
- **WHEN** a path has more segments after one that is no module where it landed, as `front::tools::Spade` with `pub extern crate tool as tools` in `front`, `front::builder::Factory` with `pub use inner::*`, or `counting::Cell` with `counting` from a glob of the same file
- **THEN** the import lands on the module of the crate that holds it, and impact about the name lists the importer

#### Scenario: Modules that glob each other end a path
- **WHEN** twelve modules each glob the other eleven and a file holds `use crate::m0::missing::Item`
- **THEN** the index ends and the import lands on the file of `m0`

#### Scenario: A re-export of what its crate does not hand out leaves the glob its name
- **WHEN** `front` holds `pub use derived::Thing`, where `derived` exports no `Thing`, and `pub use inner::*`, and `app` holds `use front::Thing`
- **THEN** impact about `Thing` in `inner` lists `app`

#### Scenario: A trait imported as underscore still answers by method name
- **WHEN** `b.rs` holds `use crate::a::Shout as _` and calls `value.shout()` on a generic `value`
- **THEN** impact about `Shout.shout` in `a.rs` lists `b.rs`

### Requirement: Implementations and derives
An `impl Trait for Type` SHALL make `Type` an heir of `Trait` that overrides it. A `derive(Trait)`, a supertrait, a type alias `type A = Foo` and a static or constant of type `Foo` SHALL be heirs without overriding.

#### Scenario: An implementation is reached through its trait
- **WHEN** `tr.rs` declares `Tr` and `imp.rs` holds `impl Tr for Bash`
- **THEN** the heirs and overrides of `imp.rs` hold `Bash Tr` and impact about `Tr` in `tr.rs` lists `imp.rs`

#### Scenario: A derive names the trait it derives
- **WHEN** a struct carries `#[derive(Debug, Clone, serde::Serialize)]`
- **THEN** the struct uses `Debug`, `Clone` and `Serialize` and is an heir of each

### Requirement: The arguments of a macro are read
The tokens of a macro call SHALL be read for uses: an identifier after `.` is a member read, a `::` path is a path, and a name after `$` is skipped.

#### Scenario: The arguments of a macro are read
- **WHEN** a function holds `println!("{}", foo())`, `assert!(Runner::start(&Runner) == ())` and `dbg!(value.start())`
- **THEN** it uses `foo`, `Runner.start` and `.start`

### Requirement: A Rust file with syntax errors records no declarations
A Rust file with syntax errors from which the parser recovered SHALL record no declarations and no exports and keep its imports, and a walk by name SHALL follow it whole.

#### Scenario: A file with syntax errors records no declarations
- **WHEN** `b.rs` imports `foo` from `a.rs`, has a syntax error, and `c.rs` imports `run` from `b.rs`
- **THEN** impact about `foo` in `a.rs` with depth 2 lists `b.rs` at distance 1 and `c.rs` at distance 2
