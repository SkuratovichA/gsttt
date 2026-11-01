# Proposal: URLs built with the URL class

## Why

A field test on a public microservice codebase found calls like `httpClient.post(new URL("build-runner/code-generation-success", this.options.buildManagerUrl).href, body)`. The TypeScript connector reads an argument as text only when it is a string, a template or a concatenation, so it read none of them, and the services they call looked unreached.

## What Changes

- Wherever a rule reads an argument as text, `new URL(path, base)` reads as the URL it builds, and so do `new URL(path, base).href`, `.pathname` and `.toString()`. `.origin`, `.search` and the other members are not read.
- A path that starts with `/`, starts with a computed part (a base URL) or holds its own origin is the URL. A relative path joins the directory of a base that is a literal or partly computed text, and follows a dropped base URL when the base is not text. A path with a `..` segment, or one that is not text, is not read, so the call is an unread site.
- The version of the reader goes from 3 to 4, because the facts change.

## Impact

- Capability `typescript`.
- `crates/lang-ts/` only: `shape.rs`, `project.rs`, a fixture per scenario and tests.
- Indexes built by an earlier version are parsed again once.
- Out of scope: a URL held in a local variable, and a receiver made by `axios.create()` in another file.
