# Proposal: NestJS decorators called through a namespace import

## Why

Generated NestJS services write `import * as graphql from "@nestjs/graphql"` and `import * as common from "@nestjs/common"`, and decorate with `@graphql.Query(...)` and `@common.Get(...)`. The TypeScript connector names such a decorator by its dotted chain, `graphql.Query`, and a rule matches segment by segment, so the `nestjs` pack, which names `Query` and `Get`, read none of them. On Amplication v3.15.0 gsttt found 0 served GraphQL keys in `packages/amplication-plugin-api` where a TypeScript-checker oracle finds 16.

## What Changes

- The `nestjs` pack also matches the decorators called through any namespace (`*.Get`, `*.Query` and the rest) and the `Controller` and `Resolver` gates likewise. `$callee` is the last segment of the name, so the method and the root type are unchanged.
- No connector change.

## Impact

- `crates/lang-ts/packs/nestjs.toml`.
- Specs: `typescript` (NestJS routes, GraphQL).
