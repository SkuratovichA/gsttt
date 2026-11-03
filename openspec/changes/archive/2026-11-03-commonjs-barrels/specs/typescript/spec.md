## ADDED Requirements

### Requirement: CommonJS barrels pass names on
A CommonJS export whose value is `require('x')`, `require('x').y`, or `y` of a top-level `const local = require('x')` SHALL be a re-export of `default` or of `y`, as `export { y as name } from 'x'` is, and that `require` SHALL be no load of the module.

#### Scenario: A property whose value is a require passes the module's value on
- **WHEN** a file has `module.exports = { one: require('./lib/one'), many: require('./lib/many') }`
- **THEN** it re-exports `default` of each module as `one` and `many`, and it loads neither
- **AND** a file that takes `one` from it is a caller of the function `lib/one` exports

#### Scenario: An assignment to exports of a require passes the module's value on
- **WHEN** a file has `exports.one = require('./lib/one')`
- **THEN** it re-exports `default` of `./lib/one` as `one`, and a file that takes `one` from it is a caller of that function

#### Scenario: A member of a required module passes that name on and no other
- **WHEN** a file has `const validate = require('./lib/validate')`, `exports.check = validate.check` and `exports.helper = require('./lib/many').helper`
- **THEN** it re-exports `check` of `./lib/validate` and `helper` of `./lib/many`
- **AND** it is no caller of `skip` of `./lib/validate` or of `other` of `./lib/many`

#### Scenario: A require that passes names on is no load
- **WHEN** a file passes a name on with a `require`, and the name asked about is one it does not pass on
- **THEN** the file is not listed for that name

### Requirement: A CommonJS file passes a whole module on
`module.exports = require('x')`, an object with `...require('x')`, and a call that hands `exports` or `module.exports` together with `require('x')` SHALL re-export every name of `x`, as `export * from 'x'` does, and the first also `default`. The exports of the file SHALL stay as they were.

#### Scenario: A required module handed on whole passes every name and the value
- **WHEN** a file has `module.exports = require('./lib/many')`
- **THEN** it re-exports every name of `./lib/many` and its `default`, and its exports are unknown
- **AND** a file that takes `helper` from it is a caller of `helper` of `./lib/many`

#### Scenario: A spread and Object.assign of a require pass every name
- **WHEN** a file has `module.exports = { ...require('./lib/many'), extra: 1 }` or `Object.assign(module.exports, require('./lib/validate'))`
- **THEN** it re-exports every name of the module, its exports are unknown, and a file that takes a name from it is a caller of that name

### Requirement: A spread belongs to no property
A spread in `module.exports = { … }` SHALL give no use to a neighbouring key.

#### Scenario: A spread gives no use to a neighbouring key
- **WHEN** a file has `module.exports = { ...require('./lib/many'), extra: 1 }`
- **THEN** the declaration `extra` uses nothing

### Requirement: A CommonJS barrel is read as an export barrel
An importer of a CommonJS barrel SHALL be listed for the same names, and for the same file, as an importer of the barrel written with `export … from`.

#### Scenario: A file asked about with no name lists its CommonJS barrels
- **WHEN** a module is asked about with no name, and a CommonJS file passes names of it on
- **THEN** that file is listed as a file that re-exports them

#### Scenario: A CommonJS barrel answers as the same barrel written with export
- **WHEN** one tree has its barrels written with `require` and another has the same barrels written with `export … from`
- **THEN** the same questions give the same files in both
