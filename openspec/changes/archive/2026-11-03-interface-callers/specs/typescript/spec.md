# Spec Delta

## MODIFIED Requirements

### Requirement: Declarations
The connector SHALL record the declarations of a file, each with the lines it spans: every top-level function, class, variable, enum, interface, type alias and namespace, each name a destructuring declares, `default` for `export default` of an expression, every member of a class as `Class.member` and every method and property signature of an interface as `Interface.member`. Overloads and merged declarations SHALL be joined by name. A file read past syntax errors SHALL record none.

#### Scenario: Every declaration form is recorded
- **WHEN** a file has `function f() {}`, two overloads and the body of `g`, `const { a, b: c } = obj`, `class C { constructor() {} m() {} static s = 1; #p = 2; get v() { return 1 } set v(x) {} }`, `enum E {}`, `interface I {}` twice, `type T = string`, `namespace N {}` and `export default 1`
- **THEN** its declarations are `C`, `C.#p`, `C.constructor`, `C.m`, `C.s`, `C.v`, `E`, `I`, `I.first`, `I.second`, `N`, `T`, `a`, `c`, `default`, `f` and `g`, each with its lines, matching the reviewed snapshot

#### Scenario: A file read past errors records no declarations
- **WHEN** a file declares `foo`, imports `dep` and has a syntax error
- **THEN** it records no declarations
- **AND** its static imports record no local names

#### Scenario: A member of an interface is a declaration
- **WHEN** a file has `export interface Provider { name: string; list(): string[]; list(id: string): string[]; [key: string]: unknown; ['computed'](): void }`, `export default interface Named { id(): number }` and `type Shape = { size(): number }`
- **THEN** its declarations are `Named`, `Named.id`, `Provider`, `Provider.list`, `Provider.name` and `Shape`
- **AND** `Provider.list` spans both overloads

## ADDED Requirements

### Requirement: Overrides
The connector SHALL record, for a class, the heads a value of which may run its members: the head of its `extends` and each type it `implements`, as the source spells the name without type arguments. An interface, a top-level value and a class with neither SHALL record none.

#### Scenario: A class records what it extends and implements
- **WHEN** a file has `class Aws extends Base implements Provider, lib.Other<string>`, `interface Wide extends Base` and `class Plain {}`
- **THEN** it records `Aws` of `Base`, `Aws` of `Provider` and `Aws` of `lib.Other`
- **AND** it records no override for `Wide` or `Plain`
