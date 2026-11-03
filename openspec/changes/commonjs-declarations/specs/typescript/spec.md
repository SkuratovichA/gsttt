# Spec Delta

## ADDED Requirements

### Requirement: CommonJS declarations
The connector SHALL declare the names a top-level statement writes in place on `exports`: `exports.a = value` and `module.exports.a = value` declare `a` over the statement, a class with its members. `module.exports = function f() {}` and `= class F {}` SHALL declare `f` or `F` and export `default` from it, and `= { … }` SHALL declare each key written in place with its own lines. A local, a `require`, a computed key and a spread declare nothing.

#### Scenario: A function written in place on exports is declared
- **WHEN** a file has `exports.run = function () { return dep() }` over three lines, `exports.a = exports.b = () => dep()`, `module.exports.limit = 5`, `exports.local = dep` and `exports.loaded = require('./other')`
- **THEN** it declares `run` over its three lines, `a` and `b` over their statement, and `limit`, and not `local` or `loaded`
- **AND** `run` uses `dep` and what the body of `run` uses is not read as the file loads, matching the reviewed snapshot

#### Scenario: An object assigned to module.exports declares each property written in place
- **WHEN** a file has `module.exports = { run() { return dep() }, stop: function () { return other() }, limit: 5, Bar: class { go() { return dep() } } }`
- **THEN** it declares `run`, `stop`, `limit`, `Bar` and `Bar.go`, each over the lines of its own property
- **AND** `run` uses `dep`, `stop` uses `other` and `Bar.go` uses `dep`, matching the reviewed snapshot

#### Scenario: A property that holds a local or a require declares nothing
- **WHEN** a file has `module.exports = { a, b: local, c, d: require('./d') }` for a function `a`, a `require` of `local` and a name `c` that a `require` binds
- **THEN** it declares none of `b`, `c` and `d`
- **AND** the names the properties hold are read as the file loads

#### Scenario: A computed key declares nothing
- **WHEN** a file has `module.exports = { ['computed']() {}, run() {} }`
- **THEN** it declares `run` and not `computed`

#### Scenario: A class written in place on exports is declared under its exported name
- **WHEN** a file has `exports.Foo = class Inner { run() { return dep() } }`
- **THEN** it declares `Foo` and `Foo.run`, and `Foo.run` uses `dep`

#### Scenario: A class assigned to module.exports is declared and exported as default
- **WHEN** a file has `module.exports = class Foo { go() {} }`, and another has `module.exports = function run() {}`
- **THEN** the first declares `Foo` and `Foo.go` and exports `default` from `Foo`
- **AND** the second declares `run` and exports `default` from `run`, matching the reviewed snapshot

#### Scenario: An anonymous function assigned to module.exports declares nothing
- **WHEN** a file has `module.exports = function () { return dep() }`
- **THEN** it declares no name for the function

#### Scenario: A call on exports is a use of the name the file declares
- **WHEN** a file has `exports.run = function () {}` and `exports.again = function () { return exports.run() }`
- **THEN** `again` uses `run`

#### Scenario: A void placeholder on exports is no declaration
- **WHEN** a file has `exports.a = void 0;`, then `function a() {}` over three lines, then `exports.a = a;`
- **THEN** it declares `a` over the lines of the function only
