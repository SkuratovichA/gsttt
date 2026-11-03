# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The TypeScript connector

- [x] 2.1 `re_export_name` and `re_export_all` shared by the export-from visitors and the CommonJS forms
- [x] 2.2 A value that is `require('x')`, `require('x').y` or `local.y` is a re-export, and its `require` is no load
- [x] 2.3 `module.exports = require('x')`, `...require('x')` and a call that hands on `exports` with `require('x')` re-export everything
- [x] 2.4 `READER_VERSION` 33 to 34

## 3. Tests

- [x] 3.1 The scenarios of the delta, on the trees `commonjs-barrel` and `commonjs-barrel-export`
