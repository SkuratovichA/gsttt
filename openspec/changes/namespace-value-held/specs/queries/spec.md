# Spec Delta

## ADDED Requirements

### Requirement: A load names what it takes
Asked about names, a dynamic import or a `require` that names what it takes SHALL take those names and no others, where it is written. A `require` that binds `default` to a local holds the module: the file uses a name where it uses `local.name`, and a bare use of the local names nothing. One below the top level takes a name only where the file reads it. The answer counts the files left out in a note. A load that takes the module itself SHALL take every name.

#### Scenario: A load takes the names read from it
- **WHEN** `a.ts` declares `foo` and `bar`, `b.ts` has `const { bar } = await import('./a')` in `load`, `c.ts` has `(await import('./a')).foo()` at its top level, `d.ts` has `const lib = await import('./a')` in `start`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists `c.ts` reached at load and `d.ts` with the caller `start`
- **AND** it does not list `b.ts`

#### Scenario: A require held by a local takes a name where the file uses it
- **WHEN** `lib.js` declares `helper` and `other` and lists them as its exports, `user.js` has `const lib = require('./lib')` at its top level and declares `run`, which uses `lib.helper`, and `stop`, which uses `lib.other`, and impact is asked about `helper` in `lib.js`
- **THEN** impact lists `user.js` with the caller `run` and not `stop`

#### Scenario: A local a require binds names nothing by itself when passed away
- **WHEN** the files are those of the previous scenario, and `user.js` declares `pass`, which uses `lib`, and `idle`, which uses nothing
- **THEN** impact lists no file
- **AND** a note says that 1 file loads this module and reads none of the names asked about

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

### Requirement: Members reached through a field
A use of `Type.member` SHALL refer to that member only, where a use of `Type` alone refers to every member. Asked about a member, `impact` SHALL list the declarations that use it, and those that use its class or an heir of it alone only where their file reads the member by name on some value. Asked about a class, it SHALL list those that use the class or any member of it. A member SHALL go on to the files that import its class, and a namespace import SHALL go on to the member of its class.

#### Scenario: A call through a field names the member
- **WHEN** `service.ts` declares `InvoiceService` with the methods `issue` and `cancel`, `resolver.ts` imports it and declares `InvoiceResolver.issueInvoice`, which uses `InvoiceService.issue`, `InvoiceResolver.cancelInvoice`, which uses `InvoiceService.cancel`, and `InvoiceResolver.all`, which uses `InvoiceService` and reads no `issue`, and impact is asked about `InvoiceService.issue` in `service.ts`
- **THEN** impact lists `resolver.ts` with the caller `InvoiceResolver.issueInvoice`
- **AND** it does not list `InvoiceResolver.all` or `InvoiceResolver.cancelInvoice`

#### Scenario: Asked about the class, a call of any member is listed
- **WHEN** the files are those of the previous scenario and impact is asked about `InvoiceService` in `service.ts`
- **THEN** impact lists `resolver.ts` with the callers `InvoiceResolver.all`, `InvoiceResolver.cancelInvoice` and `InvoiceResolver.issueInvoice`

#### Scenario: A member goes on to the files that import its class
- **WHEN** `page.ts` imports `InvoiceResolver` from `resolver.ts` and declares `Page.open`, which uses `InvoiceResolver.issueInvoice`, `Page.close`, which uses `InvoiceResolver.cancelInvoice`, and `Page.all`, which uses `InvoiceResolver` and reads no `issue`, and impact is asked about `InvoiceService.issue` in `service.ts` with depth 2
- **THEN** impact lists `page.ts` at distance 2 with the caller `Page.open`, and not `Page.all` or `Page.close`

#### Scenario: A member goes on under the name a barrel exports its class by
- **WHEN** `index.ts` re-exports `InvoiceResolver` as `Res` from `resolver.ts`, `page.ts` imports `Res` from `index.ts` and its `Page.open` uses `Res.issueInvoice`, `Page.close` uses `Res.cancelInvoice` and `Page.all` uses `Res` and reads no `issue`, and impact is asked about `InvoiceService.issue` in `service.ts` with depth 2
- **THEN** impact lists `page.ts` at distance 2 with the caller `Page.open`
- **AND** it does not list `index.ts`

#### Scenario: A namespace import goes on to the member of its class
- **WHEN** `page.ts` has `import * as res from './resolver'` and declares `Page.open`, which uses `res.InvoiceResolver.issueInvoice`, `Page.close`, which uses `res.InvoiceResolver.cancelInvoice`, and `Page.all`, which uses `res`, and impact is asked about `InvoiceService.issue` in `service.ts` with depth 2
- **THEN** impact lists `page.ts` at distance 2, imports `resolver.ts`, with the caller `Page.open`
- **AND** it does not list `Page.close` or `Page.all`

#### Scenario: A member a subclass inherits reaches the callers of the subclass
- **WHEN** `audit.ts` imports `InvoiceService` from `service.ts` and declares `AuditedService`, which uses `InvoiceService` and whose file reads `issue`, `page.ts` imports `AuditedService` and declares `Page.open`, which uses `AuditedService.issue`, and `Page.idle`, which uses nothing, and impact is asked about `InvoiceService.issue` in `service.ts` with depth 2
- **THEN** impact lists `audit.ts` at distance 1 with the caller `AuditedService`, and `page.ts` at distance 2 with the caller `Page.open` and not `Page.idle`

#### Scenario: A file that holds a class and never names the member is no caller
- **WHEN** `page.ts` declares `hold`, which uses `Service`, reads no `place`, and impact is asked about `Service.place` in `service.ts`
- **THEN** impact lists no file
- **AND** a note says that 1 file holds `Service` and names no `place`, and that asking about `Service` lists it

#### Scenario: A file that holds a class and reads the member on any value is a caller
- **WHEN** the files are those of the previous scenario and `page.ts` reads `place` on some value
- **THEN** impact lists `page.ts` at distance 1 with the caller `hold`
- **AND** the answer has no note

#### Scenario: Asked about the class, a holder is listed
- **WHEN** the files are those of the first holder scenario and impact is asked about `Service` in `service.ts`
- **THEN** impact lists `page.ts` at distance 1 with the caller `hold`

#### Scenario: A value of a class passed away is no caller of its members
- **WHEN** `service.ts` declares `Service` with `place` and `shared`, an heir of `Service`, `page.ts` imports `shared` and declares `go`, which uses `shared` and reads no `place`, and impact is asked about `Service.place` in `service.ts`
- **THEN** impact lists no file

#### Scenario: A holder in the file of the class needs the member's name too
- **WHEN** `service.ts` declares `Service` with `place` and `make`, which uses `Service` and reads no `place`, and impact is asked about `Service.place`
- **THEN** impact lists no file

### Requirement: A namespace import names what it reads
In the walk over names, a namespace import SHALL be a caller only of the names it uses, as `lib.helper`, in a declaration and in a load, also through a barrel that exports the namespace. A use of the namespace alone names nothing: the file SHALL be a caller only where it reads the name on some value, and otherwise it is counted in a note and not listed.

#### Scenario: A namespace import is a caller only of the names it uses
- **WHEN** `svc.ts` exports `helper` and `other`, `a.ts` has `import * as lib from './svc'` with `viaHelper` using `lib.helper` and `viaOther` using `lib.other`, `b.ts` has the same import and `onlyOther` using `lib.other`, and impact is asked about `helper` in `svc.ts`
- **THEN** impact lists `a.ts` with the caller `viaHelper` and not `viaOther`, and does not list `b.ts`

#### Scenario: A namespace used as a value is held
- **WHEN** `c.ts` and `d.ts` have the same import, `pass` in `c.ts` uses `lib` alone as `consume(lib)` does, `pick` in `d.ts` uses `lib` alone as `lib[key]()` does, and impact is asked about `helper` in `svc.ts`
- **THEN** impact lists no file
- **AND** a note says that 2 files load this module and read none of the names asked about

#### Scenario: A namespace passed away names nothing by itself
- **WHEN** `page.ts` has `import * as lib from './lib'` and declares `go`, which uses `lib` alone, and impact is asked about `helper` in `lib.ts`
- **THEN** impact lists no file

#### Scenario: A file that reads the member and holds the namespace as a value is a caller
- **WHEN** the files are those of the previous scenario and `page.ts` reads `helper` on some value
- **THEN** impact lists `page.ts` with the caller `go`
- **AND** the answer has no note

#### Scenario: A file that holds a namespace as a value is named in a note and not listed
- **WHEN** `page.ts` has `import * as lib from './lib'` and declares `go`, which uses `lib` alone and reads no `helper`, and impact is asked about `helper` in `lib.ts`
- **THEN** impact lists no file
- **AND** a note says that 1 file loads this module and reads none of the names asked about, and that asking about `lib.ts` lists it

#### Scenario: A load reads through a namespace by the names it uses
- **WHEN** the code of `boot.ts` that runs as it loads uses `lib.helper` of the same import, and impact is asked about `helper` and about `other` in `svc.ts`
- **THEN** impact lists `boot.ts` with a load for `helper` and does not list it for `other`

#### Scenario: A name goes on through a namespace a barrel exports
- **WHEN** `barrel.ts` has `export * as ns from './svc'`, `page.ts` imports `ns` from it with `open` using `ns.helper` and `close` using `ns.other`, and impact is asked about `helper` in `svc.ts` with depth 2
- **THEN** impact lists `page.ts` with the caller `open` and not `close`

## REMOVED Requirements

### Requirement: Loads that name what they take
**Reason**: A bare use of a local a require binds is no longer a use of every name, so the requirement and its scenarios move to `A load names what it takes`.
**Migration**: Ask about the file, with no name, to list every file that loads it; asked about a name, a file that only hands the module on is counted in a note.

### Requirement: Members called through a field
**Reason**: A namespace used alone no longer takes every name, so the requirement and its scenarios move to `Members reached through a field`.
**Migration**: Ask about the file, with no name, to list every file that loads it; asked about a name, a file that only hands the module on is counted in a note.

### Requirement: A namespace import names what it uses
**Reason**: A namespace used as a value no longer uses every name, so the requirement and its scenarios move to `A namespace import names what it reads`.
**Migration**: Ask about the file, with no name, to list every file that loads it; asked about a name, a file that only hands the module on is counted in a note.
