# Design

## Context

A rule reads the text of an argument through `Site`, which the TypeScript connector implements with `Shape`, an owned copy of a call or a decorator. The parse of a file is pure and cached, so it cannot look at another file; the link sees all files of a project but not their syntax trees. Until now a value kept the name it was written as only when the whole value was that one name.

## Decisions

- **One tree of parts, with the policy of its join.** `Part` (`crates/lang-ts/src/parts.rs`) is text, a computed part, a name, or pieces written one after the other with a `Reads`: `Always` for a template, which is text whatever its pieces read, `Any` for `a + b`, which is text when one operand is, and `All` for a constant, which is text only when everything it is built from is. The parse reads a tree with every name as nothing, and the link reads it again with the names resolved, so the parse-time text is the one it always was, including `${}` for a part that is no text.
- **A value holds parts only where a name is inside.** `Part::cat` folds a whole with no name into text, so the common value stays a string and the hot path does not allocate a tree.
- **The sources of the rule's key decide whether a site waits.** `Shape::holds_name` takes a `ValueExpr` and answers for the values that expression reads, so a name in an argument the rule never reads defers nothing. A rule that found a contract defers on its method as well, because a method that is a name changes the key. A site that is blind and has a name only in its method stays the unread site it was.
- **Constants keep their parts.** `Constants` stores a constant with a name in it as a `Part::Cat(All, …)`, and a template that names a same-file constant splices that constant's parts. The exported ones are kept in `TsExtra::constants`, and the link's `constant()` fills them in the context of the file that exports them, following each name with `imported()` as a name that stands alone is followed.
- **A cycle is cut when a constant comes back to itself.** `constant()` carries the constants it is inside of, as `(file, key)`, and gives no text for one that is among them, so the value stays unread. A depth cap alone would visit the constants of a cycle once for every way round, which is exponential in the number of times they name each other. The read keeps this path on a stack of frames of its own and not on the call stack, so the length of a chain is no limit; the nesting of one stored constant (32 levels) and the text it puts together (one mebibyte) are limited where `Part::cat` builds it.
- **A maker the file calls on is read at the link like one another file makes.** `LinkMaker` (was `ExportedMaker`) is a maker with its callee and shape; it is kept for a name the file exports and for a name a deferred call of the file waits on (`DeferredTarget::Made`). The link reads the call with `on_maker`, which `on_made` shares: it fills the shape of the maker, reads the prefix the rule takes from it and applies the rule to the call.

- **Imports are noted before constants.** `Constants::collect` takes the names of every `import`, `import = require`, `require` and `import()` of the top level before it reads the first constant, so the order of the statements does not decide whether a constant that names an import is stored with its part. The walk of the file binds the same names (`Analyzer::bind`), and a test pins the two lists against each other.
- **A router made with a name waits before its prefix is asked.** `CallRule::behind` gives no prefix for one that is not text under a rule that serves routes, and that is a blind spot. The call is deferred as `DeferredTarget::Made` first, and the link reads the prefix against the file that made the router.
- **A constant is read once.** The link keeps what each `(file, key)` read as. A stored constant is built from pieces that must all read, so what it reads as does not depend on the constants it is read inside of: a read that is cut because it came back to itself is cut from either end of the cycle.

## Known limitations

- A name is looked up in the imports of the file with no scope resolution. A name that the file binds again anywhere (`Constants::rebound`, from the same walk that `roots` uses to find the names a file binds more than once) is no import in the whole file, so a template keeps the computed part it had and a bare name stays unread. A top-level constant of the file that a function parameter hides is still read as the constant, as it was before this change.
- A name inside an array, a `URL` or a `.href`, `.pathname` or `.toString()` is not followed: those read as text only when everything they hold is known to the parse.
- Constants of another package are not followed, because the link is per project.
