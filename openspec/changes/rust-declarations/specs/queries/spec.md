# Spec Delta

## REMOVED Requirements

### Requirement: Names in files that record no declarations
**Reason**: Rust records declarations now, so no language is named as one that does not; a file read past syntax errors is the case that remains.
**Migration**: `Names in files read past syntax errors` holds the same two behaviours, and the Rust scenario is replaced by one about a file with syntax errors.

## ADDED Requirements

### Requirement: Names in files read past syntax errors
A file whose connector recorded no declarations, as when it was read past syntax errors, SHALL be followed whole: asked about a name in such a file, `impact` SHALL walk from it as it does without names and say so in a note, and an importer without declarations, like an edge into a file whose exports are unknown, SHALL be followed whole from there.

#### Scenario: A file read past syntax errors asked about by name is followed whole
- **WHEN** `user.rs` imports `parse` from `lib.rs`, `app.rs` imports `user.rs`, `lib.rs` was read past syntax errors, and impact is asked about `parse` in `lib.rs`
- **THEN** impact lists `user.rs` at distance 1 and not `app.rs`
- **AND** a note says that `lib.rs` is followed whole because its connector records no declarations

#### Scenario: An importer without declarations is followed whole
- **WHEN** `b.ts` imports `foo` from `a.ts` and was read past syntax errors, `c.ts` imports `bar` from `b.ts` and uses it in `main`, and impact is asked about `foo` in `a.ts` with depth 2
- **THEN** impact lists `b.ts` at distance 1 without callers and `c.ts` at distance 2 with the caller `main`

### Requirement: A member read on a value of unknown type
A use of `.member` in a declaration SHALL be a use of every member of that name, so that a file which holds the class or the trait is a caller of `Class.member` when it reads `member` on a value whose class its connector did not know.

#### Scenario: A member read on an unknown value reaches the members of that name
- **WHEN** `a.rs` declares `Runner.start` and `Runner.stop`, `b.rs` imports `Runner` and uses `.start`, and impact is asked about `Runner.start` and `Runner.stop`
- **THEN** `b.rs` is a caller of `Runner.start` and not of `Runner.stop`

### Requirement: A glob import binds every name
An import of `*` that is not a re-export SHALL bind each name the walk asks about as itself and pass it to a glob of that file, unless the importing file declares that name, and an import of a module declared by `mod` SHALL take no name.

#### Scenario: A glob import binds the name asked about
- **WHEN** `b.rs` holds `use a::*` and uses `foo`, `c.rs` holds `use a::*` and does not, and impact is asked about `foo` in `a.rs`
- **THEN** impact lists `b.rs` and not `c.rs`

#### Scenario: A glob import passes the name to a glob below it
- **WHEN** `b.rs` holds `use a::*`, `c.rs` holds `use b::*` and uses `foo`, and impact is asked about `foo` in `a.rs`
- **THEN** impact lists `c.rs`

#### Scenario: A module declaration takes no name
- **WHEN** `lib.rs` declares `mod a;` and impact is asked about `foo` in `a.rs`
- **THEN** impact does not list `lib.rs`

### Requirement: A crowded head says nothing about a read on an unknown value
Asked about a member of a class, a walk through a head whose member more than 32 declarations override SHALL count a file only when the file names the member of the head. A read of the member on a value of unknown type SHALL only put the file in the note about files that hold the class, and a class that takes the members of that head SHALL be no caller. Through a head fewer declarations override the read counts, and so it does when the walk is asked about the member of the head.

#### Scenario: A read on an unknown value does not count through a crowded head
- **WHEN** 33 declarations override `GitProvider.groups`, one of them `Aws.groups`, `typed.ts` uses `GitProvider.groups`, `unknown.ts` holds `GitProvider` and reads `.groups`, and impact is asked about `Aws.groups`
- **THEN** impact lists `typed.ts` and not `unknown.ts`, a note counts `unknown.ts` and another says that 33 declarations override the head
- **AND** impact about `GitProvider.groups` lists both

#### Scenario: A head asked about too is no crowded head
- **WHEN** 33 declarations override `GitProvider.groups`, `unknown.ts` holds `GitProvider` and reads `.groups`, and impact is asked about `Aws.groups` and `GitProvider.groups` together
- **THEN** impact lists `unknown.ts`, whichever file comes first

#### Scenario: A read on an unknown value counts through a head few declarations override
- **WHEN** 32 declarations override `GitProvider.groups`, one of them `Aws.groups`, `unknown.ts` holds `GitProvider` and reads `.groups`, and impact is asked about `Aws.groups`
- **THEN** impact lists `unknown.ts`

#### Scenario: A class that takes the members of a crowded head is no caller through it
- **WHEN** 33 declarations override `GitProvider.groups`, one of them `Aws.groups`, `Gcp` takes the members of `GitProvider` and declares no `groups`, `user.ts` uses `Gcp.groups`, and impact is asked about `Aws.groups`
- **THEN** impact does not list `user.ts`
- **AND** impact about `GitProvider.groups` lists it

### Requirement: A member declared on a class of another file
A member that a file declares on a class it imports SHALL reach the files that hold the class through the file that declares the class, because whoever holds the class holds the member.

#### Scenario: A member declared on an imported class reaches the holders of the class
- **WHEN** `b.rs` imports `Foo` from `a.rs` and declares `Foo.m`, `c.rs` imports `Foo` from `a.rs` and uses `Foo.m`, `d.rs` imports `Foo` and uses `Foo.n`, and impact is asked about `Foo.m` in `b.rs`
- **THEN** impact lists `c.rs` and not `d.rs`

### Requirement: A re-export by name beats a star only for what its file hands out
A re-export by name SHALL beat the same name of a star of that file only when the file it comes from lists the name among its exports, lists no exports, or re-exports with a star of its own, and when it re-exports a whole file under that name. Any other name is of another namespace, as a derive macro beside a trait of that name.

#### Scenario: A re-export of a name its file does not hand out leaves the star its name
- **WHEN** `root.rs` re-exports everything of `builder.rs` and `Parser` by name from `derive.rs`, `user.rs` takes `Parser` of `root.rs`, and impact is asked about `Parser` in `builder.rs`
- **THEN** impact lists `user.rs` when `derive.rs` does not export `Parser`, and does not when it does
- **AND** impact does not list `user.rs` when `root.rs` re-exports `derive.rs` whole under the name `Parser`
