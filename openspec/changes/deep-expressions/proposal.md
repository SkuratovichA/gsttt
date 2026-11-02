# Proposal: A file nested deeper than a stack does not end the run

## Why

A stack overflow is no panic, so the `catch_unwind` around every connector call cannot catch it: the process aborts. The parsers of TypeScript, Python and Rust recurse once per level of nesting, and a worker of the pool has a stack of 2 MiB, so a file of 4 KB of open parentheses ended the whole index with SIGABRT, and so did the second run, which read the same file back. Three more costs hid behind the same files: the node cache of the Rust syntax tree hashes the whole subtree of a node, so a tree `n` deep cost `n` squared; the thread that drops a Rust syntax tree has a stack of 2 MiB that nothing here sizes; and a chain of `n` members cost `n` squared in the TypeScript analyzer and a sum of `n` operands in a URL cost `n` squared in the part that holds it.

## What Changes

- Core: every connector call that reads the text of a file (`parse` and `parse_embedded`) runs on a stack sized by the length of the text, `1 MiB` and `6 KiB` for each byte in a debug build or `3 KiB` in a release build, through `stacker::maybe_grow`. The stack grows only when the one at hand is smaller, and only the pages a parse touches cost memory. One helper serves both calls. A machine that refuses to map that much memory gets one failed file: `stacker` panics, and the `catch_unwind` around the call turns that into the failure of the file.
- Core: a refresh runs in a pool of its own whose workers have stacks of 256 MiB, because a file of a few KB already needs more than the default 2 MiB by that formula, and growing a stack for each of them cost 11% more instructions on a repository of 3,400 TypeScript files. Only the touched pages cost memory. A machine that refuses such stacks runs the refresh in the global pool, and its parses grow their own.
- Rust: a file whose syntax tree would nest deeper than 2000 levels is one failed file, `nested deeper than 2000 levels`, at the line where it passes the limit. The brackets and the runs of prefix operators of the lexer refuse a file before the parser runs, and the levels are counted on the steps of the parser that the tree is built from, so a file is lexed and parsed once and no tree is built past the limit.
- TypeScript: a chain of members is matched against roots of at most 64 segments, so a chain of `n` members costs `n` and not `n` squared. A sum of text pieces is put together by moving the pieces into the first, not by copying the text of the whole at every level, and an operand of a sum that holds a name weighs only itself, not the whole sum again. The reader goes one up (`READER_VERSION` 15), because a rule whose root has more than 64 segments no longer matches.
- No other version changes: nothing a reader read before is read differently, but for the Rust files nested past the limit, which were never indexed because they aborted the run.

## Impact

- Specs: `indexing` (a requirement and three scenarios), `rust` (a requirement and a scenario), `typescript` (a requirement and a scenario).
- Code: `crates/core/src/engine/pipeline.rs`, `crates/core/src/engine/refresh.rs`, `crates/lang-ts/src/shape.rs`, `crates/lang-rust/src/depth.rs`, `crates/lang-rust/src/project.rs`, `crates/lang-ts/src/analyze.rs`, `crates/lang-ts/src/parts.rs`.
- Dependencies: `stacker` 0.1.25 in `gsttt-core`, `ra_ap_parser` `=0.0.360` in `gsttt-lang-rust` (already in the tree through `ra_ap_syntax`).
- Versions: `READER_VERSION` of `gsttt-lang-ts` 14 to 15. The `version` of the Rust connector 4 to 5, because a file nested past the limit is now a failed file.
