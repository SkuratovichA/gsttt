# Design

## Context

A rule reads the text of an argument through `Site`, which the TypeScript connector implements with `Shape`, an owned copy of a call or a decorator. The parse of a file is pure and cached, so it cannot look at another file; the link sees all files of a project but not their syntax trees.

## Decisions

- **A pre-pass collects the constants of a file.** `Constants` (`crates/lang-ts/src/analyze/constants.rs`) walks the top-level statements once, in order, so a constant may use one declared above it. It holds the dotted name of each value (`BASE`, `Env.API_URL`, `ROUTES.users.byId`), the first segment of each name so that a name which starts with anything else costs no string, and the names the file imports. `as const`, `satisfies` and parentheses are looked through; `export` declarations count; a `let`, a `var`, a name in a function and a destructured name do not.
- **The same evaluator serves the whole parse.** `Shape` reads a name through `Constants` wherever it read a literal or a template, and so do the maker shapes, the assigned values, the mount paths and `setGlobalPrefix`. `Constants::literal` is the strict form, which gives text only when every part is known; `setGlobalPrefix('a' + 'b')` reads as a result.
- **An imported name is deferred to the link, and only when a rule needs it.** A `Value` keeps the name it is written as only when it has no text and its first segment is a name the file imports. When a call or a decorator rule returns a blind spot and such a name is among its unread values, the parse stores a `Deferred` (the shape, the rule by the index of its pack and of itself, and where its fact goes) in `TsExtra`, together with the constants the file exports. The packs are in the cache key, so the indexes of the rules stay valid. A file with no such site stores nothing, and its cached data does not change.
- **The link applies the rule again.** For each `Deferred` the link follows the first segment of the name through the imports of the file with `module_file` and `export_of`, finds the value in the exporting file, and fills the shape. The outcome of the rule is routed by the target the parse kept: a fact of the file as it is, a route of an Express host under every prefix its mounts give, or a route of a NestJS class behind every prefix of the modules that reach it, and, for a base class, through the existing inheritance. A class route found this way is always a found fact and never one that keys a parsed fact again, since no parsed fact stands in its place; a controller that no app prefixes serves it as written.
- **`link` returns `Linked { facts, unread }`.** A site that the rule still cannot read after the constants are known must show in coverage like any other, and only the link knows it. The build appends these sites after the parse's own in the file's unresolved sites and counts them with the same call. They are sorted and deduplicated by file, line, detail and reason. `From<Vec<LinkedFact>>` keeps the connectors that find no unread site as they were.

## Known limitations

- A local that shadows a top-level constant of the same name is not seen as a local: the constant's value is read.
- A constant built from another file's constants, and an imported name inside a template or a concatenation, are unread sites: only a name that stands alone is followed through an import.
- `@Controller(importedConst)` still reads as an empty prefix.
- A receiver made in another file is not recognised, as before.
- The exported constants of a file are kept in its cached data, so a large exported object literal makes the cache of that file larger.
