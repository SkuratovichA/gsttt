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
