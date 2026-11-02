# Proposal: Serve every path of an array of paths

## Why

`@Post(['webhook', 'webhook/:webhookToken'])` serves two paths, and gsttt recorded only the first. A route decorator and an Express verb take an array of paths, and the TypeScript shape of an argument read an array as the first of its elements that has text, so every other path of the array was missing from the index, and a caller of it was found to call nothing. The same holds for `@Controller(['a', 'b'])`, which serves every route of its class under each prefix.

## What Changes

- Core: a new value form `$argN[]`, in `ValueSource::ArgEach`. It reads like `$argN`, but a connector that can tell the elements of an array literal apart gives one site for each element. `$argN[].x` and `$argN.x[]` are malformed, and `$arg0[]` is a position error, as `$arg0` is.
- Python and Rust: `$argN[]` is read exactly as `$argN`, since neither connector reads an array of paths as one argument with elements.
- TypeScript: a shape keeps the elements of an argument that is an array literal. `Shape::each` gives one shape for each element of the first `$argN[]` source of a value expression whose argument is an array literal with elements, and the site itself for an empty array, a value that is not an array and a value expression with no such source. An empty array is therefore an unread site, as before.
- TypeScript: a call rule serves one contract for each shape of its key, at the line of the call. An element that is not read is an unread site at that line while the others serve, and an element that is an imported constant waits for the link as an imported constant does, with the shape stored without its elements.
- TypeScript: a `@Controller` whose argument is an array of paths serves the routes of its class under each element that is read, and the routes of the class it extends under each of them too. An element that is not read is an unread site at the decorator while the others serve.
- Packs: the key of the NestJS route decorators becomes `$arg1[]|` and the key of a class decorator `$arg1[]|$arg1.path`, and the key of the Express verbs becomes `$arg1[]`. A project pack that wrote `$arg1` is read as before.
- Blind spot: `@Controller({ path: [...] })` is still read for the first path of the array that is text, by the `path` property of an options object. The provides of an HTTP server and the Known gaps of the README say so, and a test pins it.
- Not changed: the mounts of a router (`.use`) read an array of prefixes by the code of `read_mount()` in `analyze.rs`, which was left alone, and `part_of` still takes the first element of an array that has text, for every value that is not read with `$argN[]`.
- The requirements "Arrays of route paths" and "Controller prefixes in an array" (`typescript`) get their scenarios.
- Versions: none. `module_state` hashes the serialised packs of a module, so the change of `nestjs.toml` and `express.toml` gives every TypeScript file another key and the file is read again. The fingerprint of the setup hashes the built-in packs too. A project pack cannot hold `$arg1[]` before this change, since the parser refused it, so no cached fact was read with a pack that now means something else. Python and Rust packs and facts are untouched. `READER_VERSION` and `FACT_FORMAT` stay as they are, and a stale-cache case would be the reason to bump one; none was found.

## Impact

- Capability `typescript`.
- `crates/core/src/rules.rs` (the value form), `site.rs` of the Python connector and `sites/candidate.rs` of the Rust connector (the exhaustive match), and `shape.rs`, `analyze.rs`, `provides.rs` and the packs `nestjs.toml` and `express.toml` of the TypeScript connector.
- A tree with no array of paths gives the same facts as before. A tree with one gives a contract for each element of it, where it gave a contract for the first.
- The provides of an HTTP server in TypeScript lists one more blind spot, so the snapshots that list the blind spots of the connector change by one line.
- The JSON Schema of a pack names the new form in its description of a value expression.
