# Proposal: Read a package under the name a file imports it as

## Why

A pack writes the name of a package in its rules: `axios.get`, `express`, `express.Router`, `Router`. A file that renames the import is not read, so its contract is missing from the index and nothing says so: `import http from 'axios'` with `http.get('/a')`, `import * as ax from 'axios'` with `ax.post('/b')`, `const axiosLib = require('axios')`, `import ex, { Router as R } from 'express'` with `ex()` and `R()`.

## What Changes

- TypeScript: a call rule reads the first name of a callee under what the file's import says it stands for. A default import, a namespace import and `const x = require('pkg')` stand for the package, and a named import under another name for the name the package exports. This holds only for a package that some pack detects and that Node does not ship, with no subpath, and only when the name is an identifier, so `gql` from `graphql-tag` and `fetch` from `node-fetch` keep their names. A scope that binds the name again, and a name from a file of the project, keep theirs.
- The same name is used for the callee of a call that makes a receiver, so `http.create(...)` makes a client and `ex()` an app, and for the tag of a tagged template.
- The requirement "Imports under another name" gets its scenarios.
- Versions: `READER_VERSION` of the TypeScript connector goes from 21 to 22, because the sites of the same text change.

## Impact

- Capability `typescript`.
- `Constants` in `crates/lang-ts/src/analyze/constants.rs`, `visit_call_expression` and `visit_tagged_template_expression` in `crates/lang-ts/src/analyze.rs`, and the maker collector in `crates/lang-ts/src/analyze/makers.rs`.
- A tree that spells each package as its rules do gives the same answers as before.
