# Tasks

## 1. Connectors

- [x] 1.1 Six connectors with their projects, claimed files, options with JSON schemas and `provides`
- [x] 1.2 Parse status per format: syntax failures with lines, recovered counts where the parser recovers

## 2. Formats

- [x] 2.1 OpenAPI 3 and Swagger 2 with `saphyr` and `jsonc-parser`: operations, server paths, `$ref` imports, fragments joined in `link`
- [x] 2.2 GraphQL with `apollo-parser`: schema roots and renames, operations, same-document fragments, `#import`, embedded documents
- [x] 2.3 Protobuf with `protox-parse`: services, `google.api.http` bindings, imports through buf roots and ancestors
- [x] 2.4 Prisma: models, `@@map`, datasource env
- [x] 2.5 SQL with `sqlparser`: dialect option, DDL and DML tables, CTEs left out, placeholders, statement-level recovery, embedded documents
- [x] 2.6 Env files

## 3. Proof

- [x] 3.1 A test annotated `#[scenario("contract-formats", …)]` for every scenario of the `contract-formats` spec, with reviewed `insta` snapshots
- [x] 3.2 `cargo nextest run -p gsttt-contracts`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass

## 4. Review fixes

- [x] 4.1 Drop the dependencies nothing uses (`gsttt-config`, `tempfile`, `thiserror`) and keep `serde_json` for the tests only
- [x] 4.2 `SyntaxError` lives in `common.rs` with the other helpers the connectors share
- [x] 4.3 Every project returns salt 0 and `salt_of` is gone, because the options are part of the fact key already
- [x] 4.4 SQL reads the statements around one the tokenizer cannot close, and counts that one as an error
- [x] 4.5 A SQL statement read again with its placeholders replaced keeps its lines and its strings, escaped newlines, doubled quotes and lone carriage returns included
- [x] 4.6 SQL declares the new name of a renamed table, keeps one site per line, key and role, picks its grammar once when the project opens, and lists the scope of common table expressions and table functions among its blind spots
- [x] 4.7 A proto path variable is a parameter wherever it stands, `{id}:cancel` and variables that span segments included, and a binding set as `option (google.api.http).get = "…"` is read
- [x] 4.8 The proto line map borrows the location paths of the descriptor instead of cloning one vector for every location
- [x] 4.9 A brace inside a Prisma string literal closes no block, and the scan for the closing brace shares its string logic with the one for comments
- [x] 4.10 The GraphQL operations fixture also holds a shorthand query and a fragment spread at the root, which the operations scenario now names
- [x] 4.11 OpenAPI reads every server, drops an origin written as `//host` or `{variable}`, and declares each operation, and each fragment reached by a `$ref`, once under each distinct server path
- [x] 4.12 OpenAPI searches a document for its `$ref`s without reading its operations, and logs each document it skips because it is too large, unreadable or invalid
- [x] 4.13 GraphQL lists a root type renamed in another file and the `# import <names> from "<file>"` form as blind spots, and builds its root and fragment maps only for documents that hold a type or a fragment
- [x] 4.14 Env files: a `.env.*` file in a programming language is not claimed, and the lines inside a quoted value that runs over several lines declare nothing
- [x] 4.15 `detect` is tested: `openapi`, `proto`, `prisma` and `env` are detected only for a file they claim, `graphql` and `sql` for any file list
- [x] 4.16 The YAML tree resolves `<<` merge keys, which the loader leaves as entries, so an operation merged into a path item is declared; aliases were already resolved by the loader

## 5. Integration fixes

- [x] 5.1 A path segment that holds a `{…}` is a parameter of the key, so the custom method `{id}:cancel` of OpenAPI meets the protobuf binding of the same method
- [x] 5.2 One counter of syntax errors in the core, `ErrorCount`, replaces the copies of the contract, Python, Rust and TypeScript connectors, and it keeps the smallest line noted, so a parser that reports out of order still names the first error of the file
- [x] 5.3 The TypeScript connector joins directories with `RepoPath::join_dir` of the core and its own `join_dir` is gone, because the two agree for every input: a join that comes to the root is the empty directory, and an absolute path or one that leaves the repository is `None`
