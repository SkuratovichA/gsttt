## ADDED Requirements

### Requirement: A class taken by a require below the top level
A class taken by a `require` below the top level of a file, as in a callback of a test, SHALL be used where the file uses the member, `Class.member`, or reads the member on some value. A use of the class alone SHALL NOT make the file a caller of a member the file never reads.

#### Scenario: A class taken by a require below the top level is used where the member is named
- **WHEN** `page.test.ts` takes `Service` with a `require` in a callback, and `check` uses `Service.place`, or uses `Service` and the file reads `place` on some value, and impact is asked about `Service.place` in `service.ts`
- **THEN** impact lists `page.test.ts`
- **AND** it lists no file when `check` uses `Service.other` or `Service` alone and the file never reads `place`
