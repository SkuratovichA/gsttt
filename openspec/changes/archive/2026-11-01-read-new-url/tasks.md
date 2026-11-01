# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec

## 2. The reader

- [x] 2.1 `crates/lang-ts/src/shape.rs`: `text_of` reads `new URL(…)` and its `.href`, `.pathname` and `.toString()`, with `url_join` for the path and the base
- [x] 2.2 `crates/lang-ts/src/project.rs`: `READER_VERSION` 3 to 4

## 3. The scenarios

- [x] 3.1 TypeScript: a test and a fixture for "A URL built from a path and a base is called"
- [x] 3.2 TypeScript: a test and a fixture for "A relative path joins the directory of a literal base"
- [x] 3.3 TypeScript: a test and a fixture for "A URL built from a computed path is an unread site"
- [x] 3.4 TypeScript: a test and a fixture for "A URL built from a template that starts with a base URL keeps its path", and `url_text` and `url_join` keep a path that starts with a computed part
