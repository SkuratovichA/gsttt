# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `design.md`, `tasks.md` and the spec delta

## 2. The parts of a value

- [x] 2.1 `crates/lang-ts/src/parts.rs`: `Part` and `Reads`, the text of a part read with and without resolved names, and unit tests
- [x] 2.2 `crates/lang-ts/src/shape.rs`: a value keeps its parts when a name is in it, `Shape::holds_name` replaces `Shape::has_names`, and `fill` reads every value that holds a name again
- [x] 2.3 `crates/lang-ts/src/analyze/constants.rs`: a constant built from imported names is kept as parts, a template that names a same-file constant splices its parts, and the strict and lax readers stay pinned against each other

## 3. The parse

- [x] 3.1 `crates/lang-ts/src/analyze.rs`: `waits_for_link` takes the key and the method of the rule, and a call on a client or a router made here with a name in its prefix waits as `DeferredTarget::Made`
- [x] 3.2 `crates/lang-ts/src/analyze/makers.rs` and `extra.rs`: `LinkMaker` replaces `ExportedMaker` and is kept for the makers a deferred call waits on
- [x] 3.3 `crates/lang-ts/src/project.rs`: `READER_VERSION` 10 to 11

## 4. The link

- [x] 4.1 `crates/lang-ts/src/link/deferred.rs`: `constant()` puts a constant together in the context of the file that exports it, and a call on a maker of the file is read as a call on one another file makes

## 5. The scenarios

- [x] 5.1 TypeScript: a test and a fixture for "A template with an imported constant is read"
- [x] 5.2 TypeScript: a test and a fixture for "An exported constant built from an imported one is read"
- [x] 5.3 TypeScript: a test and a fixture for "A constant of the same file built from an imported one is read"
- [x] 5.4 TypeScript: a test and a fixture for "A client made with an imported base URL is called"
- [x] 5.5 TypeScript: a test and a fixture for "An imported name the link cannot read keeps its reading"
- [x] 5.6 TypeScript: a test and a fixture for "A cycle of constants stays unread"
- [x] 5.7 TypeScript: tests for "A method that is an imported name defers nothing", with a unit test of the deferred list, and for "A method that is an imported name is read by the link"

## 6. Review round

- [x] 6.1 `crates/lang-ts/src/shape.rs` and `analyze/constants.rs`: one `part_of(expr, constants, Mode)` reads a declaration and a use, a template allocates once until a hole holds a name, and the proptest pins a declared constant against its use with a name and an unresolved import
- [x] 6.2 `crates/lang-ts/src/analyze/constants.rs`: every import of the file is noted before a constant is read, for `import`, `import = require`, `require` and `import()`, with a test that pins the names against the ones the walk binds
- [x] 6.3 `crates/lang-ts/src/analyze/roots.rs` and `constants.rs`: a name that the file binds again is no import for the value a rule reads, with a test and a fixture for "A name that a local binds again is no import"
- [x] 6.4 `crates/lang-ts/src/shape.rs`: `Shape::holds` and `Shape::holds_name` share `Shape::any_value`
- [x] 6.5 `crates/lang-ts/src/analyze.rs`: a router made with a name in its prefix is deferred before the parse asks for the prefix
- [x] 6.6 `crates/lang-ts/src/analyze.rs`: `waits_for_link` looks at the prefix of a decorator rule, with a test and a fixture for "A decorator prefix that is an imported name is read by the link"
- [x] 6.7 `crates/lang-ts/src/link/deferred.rs`: each constant is read once, with tests of a diamond of 24 files and of a cycle read from every end, and the stack of a read is `Inside`
- [x] 6.8 The scenario "An imported name the link cannot read keeps its reading" has a constant that names an import which reads as no text, and "A method that is an imported name defers nothing" is proven by the unit test of the deferred list
- [x] 6.9 The requirement `Constants` is modified with its scenarios, and the new behaviours have requirements and scenarios of their own

## 7. Fix round

- [x] 7.1 `crates/lang-ts/src/link/deferred.rs`: `Reader::constant` reads a chain with a stack of frames of its own and no recursion, marking as inside only the constants on the path, with unit tests of a diamond, of a cycle read from either end, of a name that is no constant, and of a chain of 5000 constants on a thread of 256 KiB
- [x] 7.2 `crates/lang-ts/src/link/deferred.rs`: the text the reader keeps for the constants it has read is at most `MAX_KEPT` (32 MiB) in all, with a unit test of constants that each read as a text of the longest a constant may be
- [x] 7.3 `crates/lang-ts/src/parts.rs`: `Part::cat` refuses a part nested deeper than `MAX_DEPTH` (32) levels, so the data of a file stays within what `serde_json` reads back, with a test that writes a part at the limit in the deepest place of a `TsExtra` and reads it back
- [x] 7.4 `crates/lang-ts/src/parts.rs` and `shape.rs`: `Part::cat`, `Part::read` and `template_part` refuse more than `MAX_COMPOSED` (one mebibyte) of text, counting what a name adds and no literal of a template that adds none
- [x] 7.5 `crates/lang-ts/src/shape.rs`: `part_of` reads the left side of a sum in a loop and keeps a long sum in one level, with a test of a sum of 5000 operands on a thread of 256 KiB in both modes
- [x] 7.6 TypeScript: a test for "A chain of constants across two files is read to its end", with a tree of 4000 constants made in the test
- [x] 7.7 TypeScript: a test for "A constant built deeper than the limit is no constant and the project still links"
- [x] 7.8 TypeScript: a test for "A constant that doubles past the limit is no constant and the index completes"
- [x] 7.9 The proposal states the limits, and the requirements for the three scenarios are in the delta of `typescript`
