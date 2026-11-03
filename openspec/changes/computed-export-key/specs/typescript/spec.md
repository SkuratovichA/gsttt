# Spec Delta

## MODIFIED Requirements

### Requirement: Exports
The connector SHALL record the names a file exports itself, sorted: its exported declarations, the names of its `export { … }` lists, `default`, and the names its named re-exports and its `export * as` give, but not the names an `export *` brings. A named re-export SHALL record the name it exports each imported name as. The connector SHALL record the exports of a file as unknown when the file has `export =` or was read past syntax errors.

#### Scenario: Every export form is named
- **WHEN** a file has `export const a = 1`, `export const { b, c: d } = obj`, `export declare const g: number`, `export function f() {}`, `export class C {}`, `export enum E {}`, `export type T = string`, `export interface I {}`, `export namespace N {}`, `export default 1`, `export { x as y }`, `export { z as w } from './z'`, `export * as ns from './ns'` and `export * from './all'`
- **THEN** its exports are `C`, `E`, `I`, `N`, `T`, `a`, `b`, `d`, `default`, `f`, `g`, `ns`, `w` and `y`
- **AND** its re-export of `./z` records `z` exported as `w`, matching the reviewed snapshot

#### Scenario: CommonJS exports are unknown
- **WHEN** one file assigns `module.exports = { ...a }` and another assigns `exports[k] = 1` in a loop
- **THEN** the exports of both are unknown

#### Scenario: Exports of a file read past errors are unknown
- **WHEN** a file with `export const a = 1` is read past a syntax error
- **THEN** its exports are unknown

### Requirement: CommonJS exports
The connector SHALL record the names a file exports where its CommonJS form lists them: `exports.a =`, `module.exports.a =`, the keys of `module.exports = { … }`, and for a local object its keys too; and `default` for any other value. It SHALL leave them unknown for an object with a spread or a computed key, a local object that is empty or has one, `require(…)`, an indexed assignment outside the top level, and `exports` handed to a call other than the `__esModule` marker.

#### Scenario: The names of a CommonJS file are listed
- **WHEN** one file has `exports.d = 1` and `module.exports.e = f`, and another has `module.exports = { a, b: x, c() {} }`
- **THEN** the exports of the first are `d` and `e`, and those of the second are `a`, `b` and `c`

#### Scenario: A CommonJS file that exports one value exports default
- **WHEN** a file has `module.exports = class {}`, `module.exports = function () {}`, `module.exports = main()` or `module.exports = new Foo()`, or `module.exports = run` for a function `run`, a class `Foo` or a constant `make` that holds an arrow function
- **THEN** its exports are `default`

#### Scenario: CommonJS exports that cannot be listed are unknown
- **WHEN** a file has `module.exports = { ...a }`, `module.exports = { [k]: 1 }`, `module.exports = lib` for a constant that holds an object with a spread or none, `module.exports = require('./a')`, `for (const k of keys) exports[k] = 1`, `function add(k) { module.exports[k] = 1; }`, `Object.assign(module.exports, x)`, `factory(exports)` or `Object.defineProperty(exports, 'x', …)`, or has `export =`
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

## ADDED Requirements

### Requirement: Computed export keys
An assignment to a computed key of `exports` or `module.exports` that is a whole statement of the top level SHALL leave the exports of the file known. A literal key SHALL export that name; any other key SHALL export what the value hands out under its own names, the keys of an object literal or the variable of an identifier, and never `default`. The same assignment in a function, a loop, a condition or a callback SHALL leave them unknown.

#### Scenario: A computed key as a statement of the top level keeps the exports known
- **WHEN** a file has `exports[key] = 1`, and another has `exports[kKey] = make()`
- **THEN** both list no export and neither is unknown

#### Scenario: A literal computed key exports its name
- **WHEN** a file has `exports['a'] = x` and `module.exports['b'] = 2`
- **THEN** its exports are `a` and `b`

#### Scenario: A computed key exports the names its value hands out
- **WHEN** a file has `module.exports = run` and `module.exports[Symbol.for('x')] = { helper, other, plain: 3 }`, and another has `module.exports[kInternals] = internals` for a constant `internals`
- **THEN** the exports of the first are `default`, `helper`, `other` and `plain`, and those of the second are `internals`

#### Scenario: A computed key inside a loop or a function leaves the exports unknown
- **WHEN** a file has `for (const k of keys) exports[k] = 1`, another has `function add(k) { module.exports[k] = 1; }` and a third has `names.forEach((n) => { exports[n] = 1; })`
- **THEN** the exports of the three are unknown
