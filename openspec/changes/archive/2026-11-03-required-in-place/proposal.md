# Proposal: Read a package required where it is used

## Why

A package required where it is used reads as nothing. An Express application written in the old style, `const router = require('express').Router()` or `const app = require('express')()`, serves no route, and `require('axios').get('/c')` calls nothing, while the same code with a named local reads fine.

## What Changes

- TypeScript: a call whose callee starts at `require('pkg')` with a literal argument is read under the name a pack writes: `axios.get` for `require('axios').get`, `express` for `require('express')`. The same name is used for the callee of a maker, so `require('express').Router()` and `require('express')()` make a router and an application.
- `const get = require('axios').get` stands for `axios.get`, so `get('/x')` is read, and `const client = require('./named').client` is bound to the member `client` of the module, as `const { client } = require('./named')` is.
- A package no pack reads, a relative path, a built-in module, a path inside a package and a computed member give nothing, under the rule that already holds for a named require.
- Versions: `READER_VERSION` of the TypeScript connector goes from 23 to 24, because the facts of the same text change.

## Impact

- Capabilities `typescript` and `queries`.
- `required_callee` in `crates/lang-ts/src/analyze/constants.rs` beside `Constants::canonical`, `visit_call_expression` and `load_declared` in `crates/lang-ts/src/analyze.rs`, and the maker collector in `crates/lang-ts/src/analyze/makers.rs`.
- A tree that names each required package as before gives the same answers.
