# Spec Delta

## ADDED Requirements

### Requirement: Impact through a destructured name
Impact of a name SHALL list a file that takes the name out of a module by destructuring, and SHALL leave out a file that destructures only other names of the module.

#### Scenario: A destructured name lists the files that take it
- **WHEN** `lib.js` exports `a` and `other`, and `taker.js` has `const lib = require('./lib')` and `const { a } = lib`
- **THEN** impact of `lib.js` for `a` lists `taker.js`, and for `other` does not
