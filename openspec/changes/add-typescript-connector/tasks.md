# Tasks

## 1. Reading files

- [x] 1.1 `Connector` for `ts`: detection, projects per `package.json`, manifests, options with a JSON schema
- [x] 1.2 Parsing with oxc: imports of every form, flags, names, lines, test tags, recovered and failed status
- [x] 1.3 A `Site` over oxc call expressions and decorators, and matching of call, decorator, chain, embed and assignment rules

## 2. Resolution

- [x] 2.1 tsconfig chains (`extends`, `paths`, `baseUrl`, `outDir`, `rootDir`), read with comments allowed
- [x] 2.2 Resolution written by hand over the file list, with workspace packages visible by name
- [x] 2.3 Packages, builtins, unknown packages and not-found paths

## 3. Packs and link

- [x] 3.1 Packs: `express`, `nestjs`, `fetch`, `axios`, `graphql`, `trpc`, `prisma`, `typeorm`, `drizzle`, `env`
- [x] 3.2 `link`: Express mounts across files, tRPC router composition across files
- [x] 3.3 `provides` with blind spots per project

## 4. Proof

- [x] 4.1 A test annotated `#[scenario("typescript", …)]` for every scenario of the `typescript` spec, on fixture trees with reviewed `insta` snapshots
- [x] 4.2 `cargo nextest run -p gsttt-lang-ts`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass

## 5. Review fixes

- [x] 5.1 A `package.json` opens a project only at the root or when it names a package, declares dependencies or lists workspaces; bare imports and pack detection read every enclosing manifest, nearest first
- [x] 5.2 A solution `tsconfig.json` hands a file to the first referenced config that lists it, and `jsconfig.json` is read like a tsconfig
- [x] 5.3 A router entry naming a variable that holds a procedure is served, and spread and member entries are unread sites named in the blind spots
- [x] 5.4 The JSON reader stops at the nesting depth of the contracts reader, and its module doc says the two are twins
- [x] 5.5 The workspace `serde_json` no longer keeps object keys in insertion order, which nothing in this crate relied on
- [x] 5.6 Among equal `paths` and `exports` prefixes the first listed wins
- [x] 5.7 An extensionless specifier tries the TypeScript and JavaScript sources only, a built extension maps to the sources it may come from, `.d.ts` is a source, and `.`, `..` and a trailing slash name directories
- [x] 5.8 A `#` import whose target is a Node builtin resolves to a builtin
- [x] 5.9 `exports`: `null` and unmatched subpaths are not found, the first active condition in key order decides, and `import` or `require` is active by the form of the import
- [x] 5.10 `app.use` with an array of paths mounts the router under each literal element
- [x] 5.11 Hot path: candidate paths are built in one buffer, the maker scan runs only when a pack has a rule with a receiver, mount prefixes are memoised and a cycle is cut where it closes
- [x] 5.12 A target missing from the file list but present on disk is unresolved as ignored
- [x] 5.13 `app.get` with a path alone is no route, Prisma clients named `db` and `tx` are recognised, and the blind spots name the receivers and roots not read
- [x] 5.14 Scenario tests that proved less than their scenario now assert the cases the spec names
- [x] 5.15 The proposal, spec and tasks say that resolution is hand-written and that `http_prefixes` belongs to the core setup, split long requirements and list every tRPC client call

## 6. Integration fixes

- [x] 6.1 Scenarios and tests for what the engine's rule on declarations a mount keys again asks of the Express link: a router mounted at the root, at the root and under a prefix, and routes that share a line; the link already did all three
- [x] 6.2 `link` returns the parsed facts that share a line, role and kind with a fact it returns, so that a fact of another rule on the line of a route is not replaced in the graph
