# Spec Delta

## MODIFIED Requirements

### Requirement: A caller is told apart by the member it names
Asked about a member, `impact` SHALL NOT list a declaration whose only uses of the class name other members, whether it reaches them through a new instance, a local value, a parameter or the class itself. A declaration that takes the members of the class, as a subclass or a value made by `new` or by a factory does, SHALL NOT be listed for naming it.

#### Scenario: A caller of another member through a new instance is not listed
- **WHEN** `service.ts` declares `OrderService` with `place` and `cancel`, `refunds.ts` has `new OrderService().cancel(id)` in `refund`, `checkout.ts` has `new OrderService().place(id)` in `pay`, and impact is asked about `OrderService.place` in `service.ts`
- **THEN** impact lists `checkout.ts` with the caller `pay`
- **AND** it does not list `refunds.ts`

#### Scenario: Asked about the class, every caller of a member is listed
- **WHEN** the files are those of the previous scenario, with also `keep.ts` declaring `function keep(service: OrderService) {}`, and impact is asked about `OrderService`
- **THEN** impact lists `checkout.ts`, `refunds.ts` and `keep.ts`

#### Scenario: A subclass that names the member only on another value is not a caller
- **WHEN** `sub.ts` has `Sub extends Base`, `run(socket)` calls `socket.destroy()`, and impact is asked about `Base.destroy`
- **THEN** impact does not list `sub.ts`
- **AND** the note counts it as a file that holds `Base` and names no `destroy`

#### Scenario: A subclass that reads the member through this is a caller
- **WHEN** `Sub extends Base` has a `run` that calls `this.destroy()`, and impact is asked about `Base.destroy`
- **THEN** impact lists the file of `Sub` with the caller `Sub.run`

#### Scenario: A top-level value made by new is a caller at its reader and not at its declarator
- **WHEN** `store.ts` has `const shared = new Base()` and `stop` calls `shared.destroy()`, and impact is asked about `Base.destroy`
- **THEN** impact lists `store.ts` with the caller `stop` and not `shared`

### Requirement: A star barrel names nothing
Asked about a name, a file that passes it on by `export *` SHALL get no row, because nothing in it names the name; the walk SHALL go through it with no hop and reach its importers as before. A file that re-exports a class, `export * as ns` or hands a class on by `module.exports.A = A` SHALL get none either for a member of the class or a name of the namespace. A file that names what it passes on keeps its row.

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

#### Scenario: A file that hands a class on gets no row for a member of it
- **WHEN** `mid.js` exports a class of `base.js` under another name by `module.exports.Agent = Base`, `page.js` imports it and calls `Agent.destroy`, and impact is asked about `Base.destroy`
- **THEN** impact lists `page.js` and not `mid.js`
- **AND** asked about `Base`, impact lists both
