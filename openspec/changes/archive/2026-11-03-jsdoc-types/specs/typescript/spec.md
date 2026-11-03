## ADDED Requirements

### Requirement: JSDoc types in JavaScript
In a `.js`, `.cjs`, `.mjs` or `.jsx` file an `@import` tag and an `import('x')` in a JSDoc type SHALL be type-only imports, and a parameter or declaration documented with one plain class SHALL hold that class. A type that is no plain class, a TypeScript file and a tag that holds no type SHALL give no fact.

#### Scenario: An @import in a JavaScript file is a type-only import
- **WHEN** a `.js` file has a comment with `@import { A, B as C } from "./x.js"`, `@import * as ns from "./y.js"` or a default `@import`
- **THEN** the file has a type-only import of each module, with the names it takes and the locals it binds

#### Scenario: An import type in JSDoc is a type-only import
- **WHEN** a `.js` file has `@param {import ('./client')} c` and `@param {import('./client.js').Thing} t`
- **THEN** it has one type-only import of each module, at the line of the first, with the name `default` or `Thing`

#### Scenario: An import type with no name takes what the module exports as its value
- **WHEN** a `.js` file has `@typedef {import('./x')} Z`, or two functions that each document `@param {import('./x')} c`
- **THEN** the typedef is a type-only import of `default` that binds `Z`
- **AND** each function has an import of `default` at its own line, with no local

#### Scenario: A parameter documented with a class holds that class
- **WHEN** `/** @param {Sel} a */ function f(a) { a.compare(b) }` follows an `@import` of `Sel`
- **THEN** `f` uses `Sel.compare`
- **AND** a function without the comment, or whose inner function shadows `a`, uses none

#### Scenario: A destructured parameter documented by a dotted name holds that class
- **WHEN** `@param {Linter} config.linter` documents `function run({ linter }) { linter.verifyAndFix() }`
- **THEN** `run` uses `Linter.verifyAndFix`
- **AND** `config.linter` types no name in `function f(config)`

#### Scenario: A parameter of a method holds the class of its documentation
- **WHEN** a method of a class, or a method of an object, has a `@param {Visitor} v` comment above it and calls `v.callSync()`
- **THEN** it uses `Visitor.callSync`

#### Scenario: A type that is no plain class gives no class
- **WHEN** a parameter is documented `{A|B}`, `{Foo<Bar>}`, `{sel}`, `{Object}` or `{A[]}`
- **THEN** a member read from it is a use of no class

#### Scenario: JSDoc in a TypeScript file gives no fact
- **WHEN** a `.ts` file has `@import` and `@param {Sel}` comments
- **THEN** it has no import and no class from them

#### Scenario: A code sample in JSDoc gives no fact
- **WHEN** a JSDoc comment has `import('./x.js')` under `@example` or `@description`
- **THEN** the file has no import from it
