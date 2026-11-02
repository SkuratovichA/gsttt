# Proposal: Read the raw SQL that Prisma runs

## Why

Raw SQL is where a project writes its real queries, and the `prisma` pack did not read it. In a large Prisma codebase `$executeRawUnsafe(` is called about eighty times, `$queryRaw` over twenty, and `` Prisma.sql` `` sits in about ten files; a small open-source project has a handful of `` $queryRaw` ``. Every table those queries touch was missing from the graph, and the impact of a change to a table missed them without a word about it.

## What Changes

- TypeScript: the `prisma` pack gets two `[[embeds]]` rules of `lang = "sql"`, one for `$queryRaw` and `$executeRaw` and one for `$queryRawUnsafe` and `$executeRawUnsafe`, on a receiver of one, two or three segments (`prisma.$queryRaw`, `this.prisma.$queryRaw`, `this.db.client.$queryRaw`), and `Prisma.sql` in the first. A template is the document itself, a literal or a constant given to an `Unsafe` form is the document, and the existing SQL connector reads it.
- The second rule sets `text = "$arg1"`, so a text built at run time, a template with a computed part or a variable that holds no constant, is an unread site. Without `text` a rule skips a call whose first argument is no text, which is what `` $queryRaw(Prisma.sql`…`) `` needs and what would leave `$executeRawUnsafe(query)` unsaid.
- Core: `EmbedRule::computed_part` says what a computed part of a tagged template is written as in the document: `?` for SQL, nothing for any other language. The analyzer passes it to `template_text`, because a tagged template used to write a computed part as the empty string, and `` SELECT * FROM a WHERE id = ${id} `` reached the SQL parser as `… WHERE id = ` and failed. Documents of other languages keep their text byte for byte.
- SQL: the placeholder `?` is checked in the default, `postgresql`, `mysql` and `sqlite` dialects for `= ?`, `IN (?, ?)`, `LIMIT ?` and `?::uuid`, and every shape finds its table in each. No change to the connector was needed.
- Pinned, not built: `` $queryRaw(Prisma.sql`…`) `` skips the outer call, whose argument is not text, and reads the inner template by its tag, and so does a `Prisma.sql` template held in a name; a `Prisma.sql` fragment that is no statement fails the SQL parse and names no table, which is accepted.
- Keys agree across sources: `"User"` and `public."User"` in a query, `prisma.user.findMany()` and `model User` in the schema are one contract key `table user`, checked through the whole pipeline.
- The blind spot of the provides and the README no longer say that raw SQL is not read; they name what is still not read. The provides credit `prisma` for the SQL it finds.

## Impact

- Capability `typescript`.
- `crates/lang-ts/packs/prisma.toml`, `crates/core/src/rules.rs` (one additive method), the call of `template_text` in `crates/lang-ts/src/analyze.rs`, the blind spot in `crates/lang-ts/src/provides.rs` and `README.md`.
- `READER_VERSION` goes from 11 to 12. No built-in TypeScript pack had `lang = "sql"` before, but a project pack may, and for the same source and packs `computed_part` changes the text of its documents, so its cached facts must not stay. Documents of other languages keep their text.
- A tree with none of these calls gives the same index as before. A Prisma project gains `table` calls and a few blind sites, and the snapshots that hold the blind spot or the rules of the `prisma` pack change in their text only.
