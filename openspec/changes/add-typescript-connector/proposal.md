# Proposal

## Why

TypeScript is where most cross-service seams live: Express and NestJS servers, fetch and axios clients, GraphQL, tRPC, Prisma, all in one monorepo. Impact is only as good as the import graph under it, so resolution has to agree with what TypeScript itself would pick: aliases, workspace packages, ESM `.js` specifiers and built entries that point at sources.

## What Changes

- The `ts` connector: projects per `package.json`, manifests tagged, parsing with oxc into imports, tags and sites, recovery from syntax errors.
- Resolution written by hand over the repository's file list, with workspace packages, `paths`, `extends`, `exports` conditions and `outDir` to `rootDir` mapping.
- Built-in packs in `crates/lang-ts/packs/`: `express`, `nestjs`, `fetch`, `axios`, `graphql`, `trpc`, `prisma`, `typeorm`, `drizzle`, `env`.
- `link` for Express mounts and tRPC router composition.

## Capabilities

### New Capabilities

- `typescript`: what the TypeScript connector reads, how it resolves, and which contracts it finds.

### Modified Capabilities

None.

## Impact

`crates/lang-ts/` only, plus new entries in the root `[workspace.dependencies]` if needed. Known gaps, reported as blind spots: file-system routes (Next.js, Remix, SvelteKit), NestJS `setGlobalPrefix` (not applied: `http_prefixes` is a field of the core setup, not an option of this connector), routers mounted from another package, URLs and prefixes held in variables.
