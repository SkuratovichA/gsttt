# Proposal: Read the constants a rule reads text from

## Why

Code names its routes, URLs and environment variables through constants: `@Get(LOGIN_PATH)`, `this.configService.get(Env.API_URL)`, `@Get(ROUTES.users.byId)`, `fetch(USERS_URL)`, `` fetch(`${BASE}/orders`) ``. The TypeScript connector read only literals and templates, so each of these was an unread site, and the contract behind it (a route a client calls, a variable a service needs) was missing from the index. A change to the handler of such a route looked as if nothing called it. The constant is declared once, in the same file or in a file the site imports, and its value is plain text, so the answer is there to be read.

## What Changes

- A top-level `const` string, a template or concatenation of such constants, a string `enum` member, a `static` text property of a class, and a property at any depth of a `const` object literal are read as the text they stand for, wherever a rule reads text: call arguments, decorator arguments, the options of an object literal, an assigned value, the global prefix and the mount paths of a router. A `let`, a function-local name, a destructured name and an import that leads nowhere stay unread sites.
- A name that an import binds is read by the link phase. The parse keeps the site a rule could not read, and the link finds the constant in the file the import leads to, directly, through re-exports and `export *`, or by namespace, applies the same rule again, and serves the result like a site read in the parse: an Express route under every prefix its router is mounted at, a NestJS controller route behind the global prefix, and a route of a base class through the controllers that extend it.
- A connector's `link` returns `Linked { facts, unread }`. The sites the link could not read count in coverage and in the list of unread sites as the sites of the parse do. The JSON of the index has an `unread` list only when the link returned some.
- A computed path on a NestJS route, base class or `ConfigService` read stays an unread site; the scenarios that named a constant now name a call or a parameter. The requirement `Computed route paths` moves to `Route paths that are computed`, because its scenario changes its name and `openspec archive` refuses a modified requirement that drops a scenario.
- The version of the reader goes from 4 to 5, because the data kept for the link phase changes.

## Impact

- Capabilities `typescript` and `indexing`.
- `crates/core/` (the link contract and the build), the signature of `link` in `crates/lang-python`, `crates/lang-rust` and `crates/contracts`, and `crates/lang-ts/` (the analyzer, `TsExtra`, the link phase, fixtures and tests).
- Indexes built by an earlier version are parsed again once. A tree with no site that waits for the link gives the same index as before, byte for byte, except for the version.
