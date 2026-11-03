# Spec Delta

## ADDED Requirements

### Requirement: Destructured members of a module
A destructuring of a name with an object pattern of static keys and no rest element SHALL use `name.key` for each key, as the member chains do, and not the name whole. A rest element or a computed key SHALL keep the use of the name whole. A default value in the pattern SHALL be read. A destructuring of a local object SHALL read as before.

#### Scenario: A destructured namespace uses the members it names
- **WHEN** a function has `const { a, b: c } = lib` and `lib` is `import * as lib from './lib'`
- **THEN** it uses `lib.a` and `lib.b`, and not `lib`

#### Scenario: A destructured require uses the members it names
- **WHEN** a function has `const { a } = lib` and `lib` is `const lib = require('./lib')`
- **THEN** it uses `lib.a`

#### Scenario: A destructured default import uses the members it names
- **WHEN** a function has `const { a } = utils` and `utils` is `import utils from './utils.cjs'`
- **THEN** it uses `utils.a`

#### Scenario: A destructuring that may take any name uses the module whole
- **WHEN** a function has `const { a, ...rest } = lib`, and another has `const { [key]: a } = lib`
- **THEN** each uses `lib`

#### Scenario: A destructuring inside a function counts for that function
- **WHEN** one function has `const { a } = lib` and another does not
- **THEN** the first uses `lib.a` and the second uses nothing

#### Scenario: A default value in a destructuring is still read
- **WHEN** a function has `const { a = helper() } = lib`
- **THEN** it uses `lib.a` and `helper`

#### Scenario: A destructuring of a local object changes nothing
- **WHEN** a function has `const { a } = config` and `config` is a `const` object of the same file
- **THEN** it uses `config`
