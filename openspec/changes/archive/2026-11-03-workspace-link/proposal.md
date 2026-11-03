# Proposal: Let a link read the packages its project imports

## Why

The TypeScript link reads one project at a time. In a monorepo the two ends of a route then get the wrong key and never meet: an application mounts a router of another package, a URL is built from a prefix declared in a shared package, and a NestJS module is imported from a library package. Each end keeps the path it is written with, so a route is served at `/x` and called at `/ext/x`.

## What Changes

- Core: a project may ask to link across projects. Its link then also receives the files of other projects of the same connector that its files import, directly or through the files they import, as neighbours. A project that does not ask sees only its own files, and a file of another connector is never a neighbour.
- Core: a neighbour whose link data does not read back is left out, and the link of the project stands. A linked fact that replaces the key of another linked fact on the same line and role takes that fact away unless it carries the key itself, and the same linked fact from two projects counts once.
- TypeScript: the project asks to link across projects. A URL and a mount path read a constant that a file of another package declares. An app that mounts a router of another package serves its routes under the mount, with the router's own mounts chained below it, and a router no app mounts keeps its path. A NestJS app serves a controller of another package behind its global prefix when the module that holds it is reached.
- TypeScript: a mount whose path is written with names declared in other files keeps the path as written, so the link can read it. A mount path that cannot be read is served as before.
- Not changed: tRPC routers merged from another package, and the rules a pack applies to a call, are read as before, because a rule means something only with the packs of the project that parsed the file.
- Versions: `READER_VERSION` of the TypeScript connector goes from 27 to 28, because the data stored for a mount changes. `FACT_FORMAT` and `LAYOUT` stay: no other connector asks to link across projects, so the outcome for the same text only changes where `READER_VERSION` already invalidates it.

## Impact

- Capabilities `indexing` and `typescript`.
- `LinkInput` and `Project::links_across_projects` in `crates/core/src/connector.rs`, `Reach` in `crates/core/src/engine/pipeline.rs`, the linked facts of `crates/core/src/engine/build.rs`.
- `parents`, `express` and `nestjs` in `crates/lang-ts/src/link.rs`, `Mount` in `crates/lang-ts/src/extra.rs`, `read_mount` in `crates/lang-ts/src/analyze.rs`, and the blind spots in `crates/lang-ts/src/provides.rs`.
- A tree of one project gives the same answers.
