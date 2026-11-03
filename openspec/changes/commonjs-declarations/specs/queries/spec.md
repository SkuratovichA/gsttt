# Spec Delta

## ADDED Requirements

### Requirement: Names a CommonJS file writes in place
Asked about a name that a CommonJS file writes in place on `exports` or `module.exports`, impact SHALL list the files that read that name from it, and no file that reads only another name of the file.

#### Scenario: A name written in place lists the files that read it
- **WHEN** `lib.js` has `exports.run = function () {}` and `exports.stop = function () {}`, `runs.js` has `const lib = require('./lib')` and `lib.run()` at its top level, `stops.js` reads only `lib.stop()`, and impact is asked about `run` in `lib.js`
- **THEN** impact lists `runs.js` and not `stops.js`
