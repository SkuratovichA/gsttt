# Spec Delta

## ADDED Requirements

### Requirement: Impact through a computed export key
Impact SHALL NOT pass a name a file only imports on to the files that require it because the file assigns a computed key of its exports at the top level, and SHALL still list the file that takes a name that key hands out.

#### Scenario: A computed export key does not pass an imported name on
- **WHEN** `a.js` imports a class, calls it in `run`, has `module.exports = run` and `module.exports[Symbol.for('x')] = { helper }`, `b.js` requires `./a`, and `c.js` has `require('./a')[Symbol.for('x')]`
- **THEN** impact of the class lists `a.js` and not `b.js`
- **AND** impact of `helper` in `a.js` lists `c.js`
