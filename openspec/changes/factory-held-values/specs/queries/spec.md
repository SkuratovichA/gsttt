# Spec Delta

## ADDED Requirements

### Requirement: Callers through what a factory gives
Asked about a member, `impact` SHALL list the file that gets a value of the class from a static call on it or from a function that gives it, and reads that member on the value, and a file that reads the member on a value that a third file passes on. A file that calls the factory and reads no such member SHALL NOT be listed.

#### Scenario: A caller through a static factory and through a function factory is listed
- **WHEN** `service.ts` declares `Service` with `place`, a static `make` and `makeService(): Service`, `by-static.ts` has `const s = Service.make(); s.place()` and `by-function.ts` has `const s = makeService(); s.place()`, and impact is asked about `Service.place`
- **THEN** impact lists `by-static.ts` and `by-function.ts`

#### Scenario: A file that calls a factory and reads no such member is not listed
- **WHEN** a file has `Service.make().other()` and `makeService().other()`, and another only `makeService()`
- **THEN** impact asked about `Service.place` lists neither

#### Scenario: A value made by a factory and passed across a third file is a caller
- **WHEN** `service.ts` has `export const made = makeService()`, `third.ts` has `export { made } from './service'` and `shared.ts` has `made.place()`
- **THEN** impact asked about `Service.place` lists `shared.ts`
