# Spec Delta

## MODIFIED Requirements

### Requirement: Members called through a field
A use of `Type.member` SHALL refer to that member only, where a use of `Type` alone refers to every member. Asked about a member, `impact` SHALL list the declarations that use it, and those that use its class or an heir of it alone only where their file reads the member by name on some value. Asked about a class, it SHALL list those that use the class or any member of it. A member SHALL go on to the files that import its class, and a namespace import used alone SHALL take every name.

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

#### Scenario: A namespace import takes every member of its class
- **WHEN** `page.ts` has `import * as res from './resolver'` and declares `Page.open`, which uses `res.InvoiceResolver.issueInvoice`, `Page.close`, which uses `res.InvoiceResolver.cancelInvoice`, and `Page.all`, which uses `res`, and impact is asked about `InvoiceService.issue` in `service.ts` with depth 2
- **THEN** impact lists `page.ts` at distance 2, imports `resolver.ts`, with the callers `Page.all` and `Page.open`
- **AND** it does not list `Page.close`

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

#### Scenario: A namespace passed away stays a use of every name
- **WHEN** `page.ts` has `import * as lib from './lib'` and declares `go`, which uses `lib` alone, and impact is asked about `helper` in `lib.ts`
- **THEN** impact lists `page.ts` with the caller `go`, whether or not `page.ts` reads `helper`

### Requirement: Members of a class
A member of a class SHALL be a declaration of its own, `Class.member`, so that a name used in one method affects that method and not its class. Asked about a member, `impact` SHALL list the members that use it through `this` and the declarations that use the class where their file reads the member by name, and SHALL count the files that only hold the class in one note.

#### Scenario: A name used in a method affects that method
- **WHEN** `runner.ts` declares the class `Runner` with the method `start`, which calls `foo` imported from `a.ts`, and the method `stop`, `app.ts` imports `Runner` and uses it in `main`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists `runner.ts` with the caller `Runner.start`
- **AND** it does not list `Runner`, `Runner.stop` or `app.ts`

#### Scenario: Callers of a member are found through this and through its class
- **WHEN** `Runner.stop` calls `this.start()`, `app.ts` imports `Runner` and uses it in `main`, which reads `start`, and impact is asked about `Runner.start` in `runner.ts`
- **THEN** impact lists `runner.ts` with the caller `Runner.stop` and `app.ts` with the caller `main`
- **AND** the answer has no note

#### Scenario: Members of several classes are one note
- **WHEN** `runner.ts` declares the class `Runner` with the methods `start` and `stop` and the class `ModuleRunner` with the method `import`, `app.ts` imports `Runner` and uses it in `main`, which reads none of them, and impact is asked about `Runner.start`, `Runner.stop` and `ModuleRunner.import` in `runner.ts`
- **THEN** impact lists no file
- **AND** the answer has one note, that 1 file holds the classes of `ModuleRunner.import`, `Runner.start` and `Runner.stop` and names none of them, and that asking about those classes lists it

### Requirement: A star barrel names nothing
Asked about a name, a file that passes it on by `export *` SHALL get no row, because nothing in it names the name; the walk SHALL go through it with no hop and reach its importers as before. A file that re-exports a class, or `export * as ns`, SHALL get none either for a member of the class or a name of the namespace. A file that names what it passes on keeps its row.

#### Scenario: A star barrel gets no row when a name is asked about
- **WHEN** `index.ts` has `export * from './a'`, `page.ts` imports `helper` from `index.ts` and uses it in `open`, and impact is asked about `helper` in `a.ts`
- **THEN** impact lists `page.ts` at distance 1 with the reason `imports helper from index.ts`, and no other file

#### Scenario: A chain of star barrels gets no row
- **WHEN** `index.ts` has `export * from './a'`, `outer.ts` has `export * from './index'`, a test imports `helper` from `outer.ts`, and impact is asked about `helper` in `a.ts`
- **THEN** impact lists the test at distance 1, and neither barrel

#### Scenario: A named re-export keeps its row when a name is asked about
- **WHEN** `index.ts` has `export { helper } from './a'`, `page.ts` imports `helper` from `index.ts` and uses it in `open`, and impact is asked about `helper` in `a.ts`
- **THEN** impact lists `index.ts` at distance 1 with the reason `re-exports helper from a.ts`, and `page.ts` at distance 1

#### Scenario: A re-export of a class gets no row for a member of it
- **WHEN** `index.ts` has `export { Service } from './service'`, `page.ts` imports `Service` from `index.ts` and its `go` uses `Service.place`, and impact is asked about `Service.place` in `service.ts`
- **THEN** impact lists `page.ts` and not `index.ts`
- **AND** asked about `Service`, impact lists both

#### Scenario: A namespace re-export gets no row for a name it holds
- **WHEN** `index.ts` has `export * as lib from './a'`, `page.ts` imports `lib` from `index.ts` and its `go` uses `lib.helper`, and impact is asked about `helper` in `a.ts`
- **THEN** impact lists `page.ts` and not `index.ts`
