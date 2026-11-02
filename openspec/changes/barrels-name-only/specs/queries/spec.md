## ADDED Requirements

### Requirement: A star barrel names nothing
Asked about a name, a file that passes it on by `export *` SHALL get no row, because nothing in it names the name; the walk SHALL go through it with no hop and reach its importers as before. A file that names what it passes on keeps its row.

#### Scenario: A star barrel gets no row when a name is asked about
- **WHEN** `index.ts` has `export * from './a'`, `page.ts` imports `helper` from `index.ts` and uses it in `open`, and impact is asked about `helper` in `a.ts`
- **THEN** impact lists `page.ts` at distance 1 with the reason `imports helper from index.ts`, and no other file

#### Scenario: A chain of star barrels gets no row
- **WHEN** `index.ts` has `export * from './a'`, `outer.ts` has `export * from './index'`, a test imports `helper` from `outer.ts`, and impact is asked about `helper` in `a.ts`
- **THEN** impact lists the test at distance 1, and neither barrel

#### Scenario: A named re-export keeps its row when a name is asked about
- **WHEN** `index.ts` has `export { helper } from './a'`, `page.ts` imports `helper` from `index.ts` and uses it in `open`, and impact is asked about `helper` in `a.ts`
- **THEN** impact lists `index.ts` at distance 1 with the reason `re-exports helper from a.ts`, and `page.ts` at distance 1
