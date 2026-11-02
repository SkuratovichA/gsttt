# Spec Delta

## ADDED Requirements

### Requirement: A namespace import names what it uses
In the walk over names, a namespace import SHALL be a caller only of the names it uses, as `lib.helper`, in a declaration and in a load, also through a barrel that exports the namespace. A use of the namespace alone SHALL remain a use of every name.

#### Scenario: A namespace import is a caller only of the names it uses
- **WHEN** `svc.ts` exports `helper` and `other`, `a.ts` has `import * as lib from './svc'` with `viaHelper` using `lib.helper` and `viaOther` using `lib.other`, `b.ts` has the same import and `onlyOther` using `lib.other`, and impact is asked about `helper` in `svc.ts`
- **THEN** impact lists `a.ts` with the caller `viaHelper` and not `viaOther`, and does not list `b.ts`

#### Scenario: A namespace used as a value uses every name
- **WHEN** `c.ts` and `d.ts` have the same import, `pass` in `c.ts` uses `lib` alone as `consume(lib)` does, `pick` in `d.ts` uses `lib` alone as `lib[key]()` does, and impact is asked about `helper` in `svc.ts`
- **THEN** impact lists `c.ts` with the caller `pass` and `d.ts` with the caller `pick`

#### Scenario: A load reads through a namespace by the names it uses
- **WHEN** the code of `boot.ts` that runs as it loads uses `lib.helper` of the same import, and impact is asked about `helper` and about `other` in `svc.ts`
- **THEN** impact lists `boot.ts` with a load for `helper` and does not list it for `other`

#### Scenario: A name goes on through a namespace a barrel exports
- **WHEN** `barrel.ts` has `export * as ns from './svc'`, `page.ts` imports `ns` from it with `open` using `ns.helper` and `close` using `ns.other`, and impact is asked about `helper` in `svc.ts` with depth 2
- **THEN** impact lists `page.ts` with the caller `open` and not `close`
