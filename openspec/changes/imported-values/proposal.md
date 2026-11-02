# Proposal: Follow imported names into the text a rule reads

## Why

A name that another file declares was followed only when it stood alone: `fetch(API_URL)`. Code builds its URLs from such names in three more ways, and each gave a wrong fact or an unread site with nothing to show for it. A template or a concatenation with an imported name (``fetch(`${BASE}/users`)``) dropped the name and called `/users`, so the call was filed under a path the service does not serve. A constant built from an imported one (`` export const USERS = `${BASE}/users` ``) was not a constant, so every `fetch(USERS)` was an unread site. A client made with an imported base (`axios.create({ baseURL: API_URL })`) lost its base, so `api.get('/users')` called `/users`. A change to the handler of `/api/v1/users` looked as if nothing called it.

## What Changes

- A template or a concatenation that a rule reads is read with each name in it that the file imports from a file of its project, as the text of the constant that file declares. A name that reads as no text is a computed part, as it was, so a tree whose names cannot be read gives the same facts as before; they now come from the link and not from the parse.
- A top-level constant built from imported names (a template, a concatenation, or a name that stands alone) is kept with its parts, and the link puts it together in the context of the file that exports it, through the imports, re-exports and namespaces it passes. A constant is text only when every part of it reads, as before, and a cycle of constants is cut as soon as a constant comes back to itself, so it stays unread.
- A client or a router made in the file of the call, by a call whose prefix names an imported constant, is read at the link on the receiver the link finds, the way a client another file makes is. The file keeps the shape of such a maker once, beside the makers it exports.
- A call or a decorator waits for the link when a name is among the values its rule reads its key from, or, for a decorator rule, the prefix it reads at the decorator, or, when the rule found a contract, among the values it reads its method from. `fetch(compute(), { method: M.post })` no longer waits for a link that cannot make it a contract, and `fetch('/orders', { method: M.post })` is `POST /orders`.
- A name that the file imports and also binds as a parameter, a variable, a function or a class is no import for the value a rule reads anywhere in that file, because a name is looked up with no scope resolution. The file keeps the reading it has without the import, as it had before the change.
- Every import of a file is noted before the first constant is read, so a constant written above the `import` or the `require` that binds a name it uses is a constant of that name too.
- A call on a router or a client made here with a name in its prefix waits for the link before the parse asks what the router gives as a prefix. A rule that serves routes reads a prefix that is no text as a blind spot, and it was recorded as one before the call could wait.
- A constant is read once for the link, not once for every way to it: a diamond of files that each name the one before twice took 2^n reads.
- The version of the reader goes from 8 to 10, because what the parse keeps in `TsExtra` changes: the parts of a value, the constants as parts, and the makers a call waits on. Version 9 is taken by another change that is already in main. `FACT_FORMAT` is not bumped, because `crates/core` is untouched and the meaning of a fact does not change, and `LAYOUT` does not apply, because the snapshot keeps the extra of a file as opaque bytes.

## Impact

- Capability `typescript`.
- `crates/lang-ts/` (`shape.rs`, the new `parts.rs`, `analyze.rs`, `analyze/constants.rs`, `analyze/makers.rs`, `analyze/roots.rs`, `extra.rs`, `link/deferred.rs`, `project.rs`, fixtures and tests).
- Indexes built by an earlier version are parsed again once. A site whose template or method names an import now reaches the graph through the link, with the key the parse would have given it when the name cannot be read.
