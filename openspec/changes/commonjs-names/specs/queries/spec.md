# Spec Delta

## MODIFIED Requirements

### Requirement: Loads that name what they take
Asked about names, a dynamic import or a `require` that names what it takes SHALL take those names and no others, where it is written. A `require` that binds `default` to a local holds the module: the file uses a name where it uses `local.name`, or the local alone unless the file exports a default. One below the top level takes a name only where the file reads it. The answer counts the files left out in a note. A load that takes the module itself SHALL take every name.

#### Scenario: A load takes the names read from it
- **WHEN** `a.ts` declares `foo` and `bar`, `b.ts` has `const { bar } = await import('./a')` in `load`, `c.ts` has `(await import('./a')).foo()` at its top level, `d.ts` has `const lib = await import('./a')` in `start`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists `c.ts` reached at load and `d.ts` with the caller `start`
- **AND** it does not list `b.ts`

#### Scenario: A require held by a local takes a name where the file uses it
- **WHEN** `lib.js` declares `helper` and `other` and lists them as its exports, `user.js` has `const lib = require('./lib')` at its top level and declares `run`, which uses `lib.helper`, and `stop`, which uses `lib.other`, and impact is asked about `helper` in `lib.js`
- **THEN** impact lists `user.js` with the caller `run` and not `stop`

#### Scenario: A local a require binds is a use of every name when passed away
- **WHEN** the files are those of the previous scenario, and `user.js` declares `pass`, which uses `lib`, and `idle`, which uses nothing
- **THEN** impact lists `user.js` with the caller `pass` and not `idle`

#### Scenario: A destructured require takes its names
- **WHEN** `user.js` has `const { helper, other: d } = require('./lib')` at its top level and declares `run`, which uses `helper`, and `stop`, which uses `d`, and impact is asked about `helper` and then about `other` in `lib.js`
- **THEN** impact lists `user.js` with the caller `run` for `helper`, and with the caller `stop` for `other`

#### Scenario: A require of a file that exports one value reads as that value
- **WHEN** `foo.js` declares `Foo` with the methods `start` and `stop` and exports it as `default`, `user.js` has `const Foo = require('./foo')` at its top level and declares `run`, which uses `Foo.start`, and `halt`, which uses `Foo.stop`, and impact is asked about `Foo.start` in `foo.js`
- **THEN** impact lists `user.js` with the caller `run` and not `halt`

#### Scenario: A default import of a file that exports no default holds the module
- **WHEN** `lib.js` lists `helper` and `other` and no default, `user.js` has `import lib from './lib'` and declares `run`, which uses `lib.helper`, and `stop`, which uses `lib.other`, and impact is asked about `helper` in `lib.js`
- **THEN** impact lists `user.js` with the caller `run` and not `stop`

#### Scenario: A local below the top level takes a name where the file reads it
- **WHEN** `reads.js` and `holds.js` each have `const lib = require('./lib')` inside a function, `reads.js` reads `helper` on a value and `holds.js` reads only `other`, and impact is asked about `helper` in `lib.js`
- **THEN** impact lists `reads.js` and not `holds.js`
- **AND** a note says that 1 file loads this module and reads none of the names asked about

#### Scenario: A require used as it stands takes every name
- **WHEN** `user.js` has `require('./lib')` in a function `load` and uses nothing of it, and impact is asked about `helper` in `lib.js`
- **THEN** impact lists `user.js` with the caller `load`

#### Scenario: A bare use of a required module that has a default is no use of its other names
- **WHEN** `reply.js` lists `default` and `buildReply`, and three files have `const Reply = require('./reply')`: `bare.js` uses `Reply` alone, `member.js` uses `Reply.buildReply`, and `named.js` uses `Reply` and reads `buildReply` on a value, and impact is asked about `buildReply`
- **THEN** impact lists `member.js` and `named.js` and not `bare.js`
- **AND** a note says that 1 file loads this module and reads none of the names asked about
