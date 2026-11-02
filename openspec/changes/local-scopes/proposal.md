# Proposal: Read names by the scope they stand in

## Why

A constant was a constant of the top level of a file and nothing else. Code keeps its paths and URLs in the function that uses them, and each of three defects followed from the file having no scopes.

A `const path = '/a'` inside a function, read by `fetch(path)` in the same function, stayed unread: the site was an unread site with nothing to show for it, and so were a local built from another local, a local object read by its property, and the base of a client made in a function. A parameter or a local that shares its name with a constant of the file made the site read the constant, so `function param(BASE: string) { return fetch(BASE) }` called the path of a constant it never used, and the fact was wrong with nothing to show for it. And a name that a file imports and also binds anywhere, such as the parameter of one function, was dropped as an import for the whole file, so every other site that read the import fell back to a computed part or an unread site.

## What Changes

- A `const` with a plain name that a function, a block, a loop, a catch clause or a class body declares is read as a constant of that scope, the way a constant of the file is: a string, a template or a concatenation of constants, a property of an object. A site reads the innermost scope around it that binds the name, wherever in the scope the constant is declared, so a closure above the declaration and a function inside the scope read it too. A local built from another local and from a constant of the file is read.
- A parameter, a `let`, a `var`, a destructured name, a function, a class or a catch clause hides a constant of the file and an import of the same name in the scope that binds it. A `var` binds in its whole function and any other name in its block. A name that only a type mentions, such as the parameter of a function type, binds nothing. The site stays as it is without the constant: unread when the name stands alone, a computed part in a template or a concatenation.
- A client or a router made in a function by a call whose prefix names a constant reads the name as the scope of the call binds it, because the makers read their prefix through the same constants.
- The requirement "Imported names that a local binds again" is modified, because its text says the name is no import anywhere in the file. It is no import in the scope that binds it; outside that scope the name is the import, and the link reads the sites there. The scenario keeps its name and its fixture gains a function that does not bind the name. The requirement "Constants" is modified because it said a local name leaves the site unread, which is now true of a parameter and not of a local `const`.
- A call or a `.use` on a name that the file imports is read as a call on that client or a mount on that app or router only where no scope around it binds the name again, so `function list(api) { return api.get('/users') }` beside an imported `api` gives no fact from inside `list`, and `function setup(app) { app.use('/api', router) }` mounts nothing. Outside that function the name stays the import.
- The scopes are found once, by position, when the constants of the file are collected, and a reader asks for a name at the offset of its site. No walker keeps a scope of its own.
- The scopes come from one walk of their own and not from `oxc_semantic`, which builds the tables of every symbol and reference of a file, where this needs only where a name is bound.
- The version of the reader goes from 15 to 16, because what the parse keeps for a file changes: a site that read a constant of the file or an import may now read a local, so the facts and the sites that wait for the link differ. `FACT_FORMAT` is not bumped, because `crates/core` is untouched and the meaning of a fact does not change, and `LAYOUT` does not apply, because the snapshot keeps the extra of a file as opaque bytes.
- The blind spots that `provides.rs` and the README state for a route path and for a URL no longer say a function-local constant is not read, and the two that say a parameter or a variable named like a router, an app or a client is read as that one now say it of a name the file makes. A name that the file imports is no longer read as the import where a scope binds it again.

## Limitations

- Only a `const` with a plain name and an initializer that can hold text is a constant of a scope. A `let`, a `var`, a destructured name, a parameter and a name that a loop declares hide the name and read as nothing.
- A name bound twice in one scope, such as a `var` declared twice or a `const` and a function of the same name, reads as nothing in the whole scope, because the scope does not say which of the two a site means.
- A scope is found by position, so a use above the `const` of its own block, which fails at run time, reads the constant all the same. A closure above the declaration is the case that is meant.
- A top-level declaration is no scope, so a name that a file imports and also declares at its top level is not hidden. That is a duplicate declaration, which TypeScript rejects.
- A local built by a call (`const url = build()`) reads as nothing, as it does at the top level.
- The initializer of a local constant is read when the walk reaches it, so a constant that names a `const` written below it in a scope around reads as nothing: in `const p = '/top'; function f() { const g = () => { const u = p + '/x'; fetch(u); }; const p = '/late'; }` the call is not read at all.
- A name that a `require` binds inside a function becomes an import when the walk reaches the call, after the scopes were collected, so a parameter of the same name in another function is not told apart from it.
- A name that the file makes (`const router = express.Router()`, `const api = axios.create(...)`) is still found in the whole file, because the makers keep no scope: a parameter or a variable of the same name inside a function is read as the one the file makes.

## Impact

- Capability `typescript`.
- `crates/lang-ts/` (`analyze/constants.rs`, the new `analyze/constants/scopes.rs`, `analyze/roots.rs` where `rebound` is removed, `analyze.rs` where an imported receiver asks for the scope of the call, `provides.rs`, `project.rs`, fixtures and tests).
- `README.md`: the known gaps for a router, an app and a client named like a variable of a function.
- Indexes built by an earlier version are parsed again once. A site that reads a function-local constant now carries a fact where it carried an unread site, and a site that reads a parameter or a local named like a constant of the file no longer carries the fact of that constant.
