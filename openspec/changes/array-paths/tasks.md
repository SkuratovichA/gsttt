# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The core

- [x] 2.1 `crates/core/src/rules.rs`: `ValueSource::ArgEach`, its parser, its `Display`, its reading and its docs, with the forms that are malformed
- [x] 2.2 Python `site.rs` and Rust `sites/candidate.rs`: `$argN[]` is read as `$argN`
- [x] 2.3 The schema snapshot of a pack names the new form

## 3. The TypeScript connector

- [x] 3.1 `shape.rs`: an argument that is an array literal keeps its elements, `Shape::each` gives one shape for each of them, and `Shape::stored` is a shape without them
- [x] 3.2 `analyze.rs`: a call rule serves one contract for each shape, and a deferred site keeps the shape it was stored with
- [x] 3.3 `analyze.rs`: a `@Controller` with an array of prefixes serves the routes of its class and of its base class under each element that is read
- [x] 3.4 `nestjs.toml` and `express.toml` read the path with `$arg1[]`
- [x] 3.5 `provides.rs` and the README name `@Controller({ path: [...] })` as a blind spot
- [x] 3.6 `analyze/makers.rs`: the shape of a maker is stored without the elements of its arrays, as every stored shape is

## 4. The scenarios

- [x] 4.1 Fixtures and tests for "Every path of a method decorator's array serves a route", "A computed element of an array is an unread site and the other elements serve", "An element of an array that is an imported constant waits for the link", "An Express verb with an array of paths serves every path" and "An empty array of paths is an unread site"
- [x] 4.2 Fixtures and tests for "A controller with an array of prefixes serves every route under each", "A controller with a prefix that is read and one that is computed serves under the read one" and "A route of a base class is served under each prefix of a controller with an array"
- [x] 4.3 A test that pins the first-path read of `@Controller({ path: [...] })`
- [x] 4.4 A test that an imported element waits for the link under each prefix of a controller and in an Express verb, and that a name listed twice serves once
- [x] 4.5 A test that pins `read_mount()`, which reads the array of a mount by code of its own, against `Shape::each`, which reads the array of a verb
- [x] 4.6 Core: tests and a property test for the parser and the display of `$argN[]`
