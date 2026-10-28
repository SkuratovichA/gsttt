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
