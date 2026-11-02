# Proposal: Read a controller prefix from a constant of another file

## Why

Real NestJS code keeps its controller prefixes in a constant of another file: `@Controller(USERS)`, `@Controller(ROUTES.orders)`, `@Controller(Paths.ITEMS)` and `@Controller({ path: USERS })`, all imported. The parse could not read them, so the class prefix was an unread site and the whole controller served nothing, and `@Controller([USERS, 'literal'])` served only `/literal/...`. One name the link could read took every route of a controller out of the graph, although the link already reads the imported constant of a method's `@Get(LOGIN)`.

## What Changes

- TypeScript parse: a class prefix that names a constant of another file, on a named class under a rule that serves HTTP routes, is kept for the link as a waiting prefix instead of an unread site, together with the plain name the class extends. The method decorators of that class are deferred to the link, whatever text the parse could read for the other elements of an array of paths, so that no route is served twice.
- TypeScript link: the waiting prefix is read against the constants of the other files, as the prefix of the parse is read. Each method decorator is applied under each text it gives, the class serves the routes it inherits under each of them, and a prefix that still is not text is one unread site at the decorator. The routes then compose with the global prefix and the mounts of the module as any other controller's.
- `ClassPrefix` moves next to `Shape`, so that the parse and the link read a class prefix with the same code, and says whether the prefix waits for the link.
- The blind spot that says a controller whose prefix is an imported constant serves no route is narrowed to a call, a constant of another package and a constant that is not text.
- An imported constant that holds an array (`export const PATHS = ['a', 'b']`) is read as `@Get(PATHS)` is read: it is not text, so `@Controller(PATHS)` is one unread site.
- The requirement "Controller prefixes that are not read" gets a prefix that is a call in the other file, in place of one that is a text, and a requirement "Controller prefixes from constants of other files" is added.
- Versions: `READER_VERSION` of the TypeScript connector goes from 13 to 14, because the extra of a file entry stores the waiting prefixes and a deferred site of a new kind, and a file cached without them would keep the prefix as an unread site.

## Impact

- Capability `typescript`.
- `crates/lang-ts/src/analyze.rs`, `shape.rs`, `extra.rs`, `link.rs`, `link/deferred.rs`, `provides.rs` and `project.rs`.
- A tree whose controllers take their prefix from a constant of another file gives more facts and fewer unread sites. A tree with no such prefix gives the same index as before, except for the narrowed blind-spot sentence.
