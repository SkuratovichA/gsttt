# Tasks

## 1. Reading files

- [x] 1.1 `Connector` for `ts`: detection, projects per `package.json`, manifests, options with a JSON schema
- [x] 1.2 Parsing with oxc: imports of every form, flags, names, lines, test tags, recovered and failed status
- [x] 1.3 A `Site` over oxc call expressions and decorators, and matching of call, decorator, chain, embed and assignment rules

## 2. Resolution

- [x] 2.1 tsconfig chains (`extends`, `paths`, `baseUrl`, `outDir`, `rootDir`), read with comments allowed
- [x] 2.2 `oxc_resolver` over the file list, with workspace packages visible by name
- [x] 2.3 Packages, builtins, unknown packages and not-found paths

## 3. Packs and link

- [x] 3.1 Packs: `express`, `nestjs`, `fetch`, `axios`, `graphql`, `trpc`, `prisma`, `typeorm`, `drizzle`, `env`
- [x] 3.2 `link`: Express mounts across files, tRPC router composition across files
- [x] 3.3 `provides` with blind spots per project

## 4. Proof

- [x] 4.1 A test annotated `#[scenario("typescript", …)]` for every scenario of the `typescript` spec, on fixture trees with reviewed `insta` snapshots
- [x] 4.2 `cargo nextest run -p gsttt-lang-ts`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass
