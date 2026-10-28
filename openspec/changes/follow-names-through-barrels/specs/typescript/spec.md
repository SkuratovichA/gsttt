# Spec Delta

## ADDED Requirements

### Requirement: Exports
The connector SHALL record the names a file exports itself, sorted: its exported declarations, the names of its `export { … }` lists, `default`, and the names its named re-exports and its `export * as` give, but not the names an `export *` brings. A named re-export SHALL record the name it exports each imported name as. The connector SHALL record the exports of a file as unknown when the file assigns `module.exports` or `exports`, has `export =`, or was read past syntax errors.

#### Scenario: Every export form is named
- **WHEN** a file has `export const a = 1`, `export const { b, c: d } = obj`, `export declare const g: number`, `export function f() {}`, `export class C {}`, `export enum E {}`, `export type T = string`, `export interface I {}`, `export namespace N {}`, `export default 1`, `export { x as y }`, `export { z as w } from './z'`, `export * as ns from './ns'` and `export * from './all'`
- **THEN** its exports are `C`, `E`, `I`, `N`, `T`, `a`, `b`, `d`, `default`, `f`, `g`, `ns`, `w` and `y`
- **AND** its re-export of `./z` records `z` exported as `w`, matching the reviewed snapshot

#### Scenario: CommonJS exports are unknown
- **WHEN** one file assigns `module.exports = { a }` and another assigns `exports.b = 1`
- **THEN** the exports of both are unknown

#### Scenario: Exports of a file read past errors are unknown
- **WHEN** a file with `export const a = 1` is read past a syntax error
- **THEN** its exports are unknown

### Requirement: Mounts through a star
The link phase SHALL find the router a mount names behind a file that re-exports another with `export * from`, and SHALL NOT find a router imported as `default` there, because a star forwards every name but `default`. The impact walk makes the same distinction between the names a barrel exports and those it does not.

#### Scenario: A star does not forward default to a mount
- **WHEN** `routes.ts` has only `export * from './users'`, `users.ts` has a router `users` exported by name and a router `fallback` exported as `default` with one route each, `by-name.ts` mounts `users` imported by name from `./routes` under `/users`, and `by-default.ts` mounts `fallback` imported as `default` from `./routes` under `/old`
- **THEN** `users.ts` serves the route of `users` under `/users` and the route of `fallback` without the prefix `/old`
- **AND** a change to `users.ts` reaches `routes.ts` as a re-export of `users`, `by-name.ts` as an importer of `users` and `by-default.ts` as an importer of `default`, which `routes.ts` does not export
