# Spec Delta

## MODIFIED Requirements

### Requirement: Exports
The connector SHALL record the names a file exports itself, sorted: its exported declarations, the names of its `export { … }` lists, `default`, and the names its named re-exports and its `export * as` give, but not the names an `export *` brings. A named re-export SHALL record the name it exports each imported name as. The connector SHALL record the exports of a file as unknown when the file has `export =` or was read past syntax errors.

#### Scenario: Every export form is named
- **WHEN** a file has `export const a = 1`, `export const { b, c: d } = obj`, `export declare const g: number`, `export function f() {}`, `export class C {}`, `export enum E {}`, `export type T = string`, `export interface I {}`, `export namespace N {}`, `export default 1`, `export { x as y }`, `export { z as w } from './z'`, `export * as ns from './ns'` and `export * from './all'`
- **THEN** its exports are `C`, `E`, `I`, `N`, `T`, `a`, `b`, `d`, `default`, `f`, `g`, `ns`, `w` and `y`
- **AND** its re-export of `./z` records `z` exported as `w`, matching the reviewed snapshot

#### Scenario: CommonJS exports are unknown
- **WHEN** one file assigns `module.exports = { ...a }` and another assigns `exports[k] = 1`
- **THEN** the exports of both are unknown

#### Scenario: Exports of a file read past errors are unknown
- **WHEN** a file with `export const a = 1` is read past a syntax error
- **THEN** its exports are unknown

## ADDED Requirements

### Requirement: CommonJS exports
The connector SHALL record the names a file exports where its CommonJS form lists them: `exports.a =`, `module.exports.a =`, the keys of `module.exports = { … }`, and for a local object its keys too; and `default` for a function, a class, a call, a local or another value. It SHALL leave them unknown for an object with a spread or a computed key, a local object that is empty or has one, `require(…)`, an indexed assignment, and `exports` handed to a call other than the `__esModule` marker.

#### Scenario: The names of a CommonJS file are listed
- **WHEN** one file has `exports.d = 1` and `module.exports.e = f`, and another has `module.exports = { a, b: x, c() {} }`
- **THEN** the exports of the first are `d` and `e`, and those of the second are `a`, `b` and `c`

#### Scenario: A CommonJS file that exports one value exports default
- **WHEN** a file has `module.exports = class {}`, `module.exports = function () {}`, `module.exports = main()` or `module.exports = new Foo()`, or `module.exports = run` for a function `run`, a class `Foo` or a constant `make` that holds an arrow function
- **THEN** its exports are `default`

#### Scenario: CommonJS exports that cannot be listed are unknown
- **WHEN** a file has `module.exports = { ...a }`, `module.exports = { [k]: 1 }`, `module.exports = lib` for a constant that holds an object with a spread or none, `module.exports = require('./a')`, `exports[k] = 1`, `module.exports[k] = 1`, `Object.assign(module.exports, x)`, `factory(exports)` or `Object.defineProperty(exports, 'x', …)`, or has `export =`
- **THEN** its exports are unknown
- **AND** a module with export declarations keeps the names of them

#### Scenario: `module.exports` of a local object lists the keys of the object
- **WHEN** a file has `const codes = { A: make(), B: make() }`, `module.exports = codes` and `module.exports.append = append`
- **THEN** its exports are `A`, `B`, `append` and `default`

#### Scenario: An empty `module.exports` lists what is added to it
- **WHEN** a file has `module.exports = {}`, `module.exports.x = 1` and `exports.y = 2`
- **THEN** its exports are `x` and `y`

#### Scenario: The compiled module marker leaves the exports known
- **WHEN** a file has `Object.defineProperty(exports, '__esModule', { value: true })` and `exports.a = 1`
- **THEN** its exports are `a`

### Requirement: A require binds its names
A `require` at the top level of a file, whose result a declarator binds to a plain identifier or to an object pattern of plain names without a rest, a default or a computed key, SHALL bind locals as an import does: it records the names it takes and the locals it binds them to, declares no name, and a use of `local.x` is a use of `x` of that local. Any other `require` SHALL record no locals.

#### Scenario: A require at the top level binds locals
- **WHEN** a file has `const lib = require('./a')` and `const { b, c: d } = require('./a')` at its top level, and `run` uses `lib.x`
- **THEN** the first import takes `default` as `lib` and the second takes `b` and `c` as `b` and `d`
- **AND** the file declares neither `lib`, `b` nor `d`, and `run` uses `lib.x`

#### Scenario: A require that cannot bind stays as it was
- **WHEN** a file has `const { a, ...rest } = require('./a')`, `const { a: first = 1 } = require('./a')`, `const { a: { b } } = require('./a')`, and `const lib = require('./a')` inside a function
- **THEN** none of the four records a local
