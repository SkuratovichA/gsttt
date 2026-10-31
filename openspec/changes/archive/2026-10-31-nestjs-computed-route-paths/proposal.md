# Proposal: a NestJS route whose path is a constant is a blind spot

## Why

A NestJS route decorator whose path is a constant, such as `@Get(LOGIN_PATH)`, was read as if it had no argument, so it served the controller's prefix: a controller decorated `@Controller('/')` made `http GET /` and `http POST /`, routes that do not exist, while the real routes were never counted as unread sites. A route keyed at the wrong path is a wrong answer that looks complete.

## What Changes

- A literal alternative in a value expression, such as the empty text of `$arg1|`, stands in for an argument that is absent, never for one that is present but not text. `@Get()` still serves the prefix alone, and `@Get(LOGIN_PATH)` is a blind spot at its line. The `$member`, `$callee`, `$argN.prop` and `$keyword` alternatives after an unreadable argument still apply.
- The `Site` trait gains `has_arg`, which each connector answers from the argument slots it already keeps.
- `FACT_FORMAT` goes from 7 to 8, because cached facts of an unchanged file would keep the wrong route.

## Impact

- Capability `typescript`.
- `crates/core/src/rules.rs`, `crates/core/src/engine/keys.rs`, the `Site` impls of `lang-ts`, `lang-python` and `lang-rust`, and a fixture and test of `lang-ts`.
- Indexes built by an earlier version are parsed again once.
