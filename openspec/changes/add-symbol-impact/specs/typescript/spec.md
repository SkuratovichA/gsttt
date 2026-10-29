# Spec Delta

## ADDED Requirements

### Requirement: Declarations
The connector SHALL record the declarations of a file, each with the lines it spans: every top-level function, class, variable, enum, interface, type alias and namespace, each name a destructuring declares, `default` for `export default` of an expression, and every member of a class as `Class.member`. Overloads and merged declarations SHALL be joined by name. A file read past syntax errors SHALL record none.

#### Scenario: Every declaration form is recorded
- **WHEN** a file has `function f() {}`, two overloads and the body of `g`, `const { a, b: c } = obj`, `class C { constructor() {} m() {} static s = 1; #p = 2; get v() { return 1 } set v(x) {} }`, `enum E {}`, `interface I {}` twice, `type T = string`, `namespace N {}` and `export default 1`
- **THEN** its declarations are `C`, `C.#p`, `C.constructor`, `C.m`, `C.s`, `C.v`, `E`, `I`, `N`, `T`, `a`, `c`, `default`, `f` and `g`, each with its lines, matching the reviewed snapshot

#### Scenario: A file read past errors records no declarations
- **WHEN** a file declares `foo` and has a syntax error
- **THEN** it records no declarations

### Requirement: What a declaration uses
A declaration SHALL record, once each, the names it refers to that the file declares or binds by import: identifiers, JSX tags, type references and `typeof`, `Class.member` for `this.member` inside the class, and both `C.s` and `C` for `C.s`. An export specifier SHALL NOT count, nor a use of the declaration itself. Shadowing SHALL be ignored.

#### Scenario: References, tags, types and this are uses
- **WHEN** a file imports `useX`, `Props` and `Widget`, declares `helper`, `function View(p: Props) { const x = useX(); return <Widget value={helper(x)} /> }`, `class C { a() { this.b() } b() {} static s() {} }` and `function d() { return C.s() }`
- **THEN** `View` uses `Props`, `Widget`, `helper` and `useX`, `C.a` uses `C.b`, and `d` uses `C` and `C.s`, matching the reviewed snapshot

### Requirement: Writes of top-level variables
A top-level variable SHALL record as uses the declarations that assign it, update it, assign or delete a member of it, or call on it one of `set`, `add`, `delete`, `clear`, `push`, `pop`, `shift`, `unshift`, `splice`, `sort`, `reverse` and `fill`, so that a function that fills a cache affects the functions that read it.

#### Scenario: A function that fills a cache is a use of the cache
- **WHEN** a file has `const cache = new Map()`, `function fill(k) { cache.set(k, 1) }`, `function read(k) { return cache.get(k) }`, `let count = 0` and `function bump() { count++ }`
- **THEN** `cache` uses `fill` and not `read`, and `count` uses `bump`

### Requirement: Code at load
The connector SHALL record what a file refers to as it loads: everything in a top-level statement that is not a declaration, callbacks included, and in a declaration what is under a call, `new`, a tagged template or a decorator evaluated at load, in an initializer, a static field or block, a computed key, a decorator or `extends`. Function bodies, parameter defaults and instance fields SHALL NOT count.

#### Scenario: What runs at load is recorded
- **WHEN** a file imports `f`, `g`, `h`, `k`, `n`, `mixin` and `D` from `./x` and `test` from `vitest`, and has `const a = f()`, `const b = g`, `test('x', () => h())` and `class C extends mixin(D) { static s = k(); m() { n() } }`
- **THEN** what it uses at load is `D`, `f`, `h`, `k`, `mixin` and `test`
- **AND** `b` uses `g`, and `C.m` uses `n`

### Requirement: Local names of imports and exports
A static import SHALL record the local name of each name it takes, in the same order: `React` for `import React from 'react'`, `ns` for `import * as ns`, `b` for `import { a as b }`, `a` for `import { a }`, type-only imports included. An export of a local under another name SHALL record the local: `a` for `export { a as b }`, `f` for `export default f`.

#### Scenario: Imports and exports record their local names
- **WHEN** a file has `import React from 'react'`, `import * as ns from './ns'`, `import { a as b, c } from './x'`, `import type { T } from './t'`, `const f = 1`, `export { f as g }` and `export default f`
- **THEN** its imports record the local names `React`, `ns`, `b` and `c`, and `T`, and its exports record `g` from `f` and `default` from `f`, matching the reviewed snapshot
