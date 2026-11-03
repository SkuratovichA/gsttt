## ADDED Requirements

### Requirement: Overrides are listed
Asked about a member, `impact` SHALL list, in the file that declares it, the same member of each class that extends or implements its class, at any depth, with the reason `overrides`, and a head it was reached through, with the reason `declares`. It SHALL list no sibling of a head, and no constructor as an override.

#### Scenario: A class that overrides the member is listed
- **WHEN** `Sub` extends `Base` and declares `place`, `a.ts` uses `Sub.place`, and impact is asked about `Base.place`
- **THEN** impact lists `sub.ts` with the caller `Sub.place` and the reason `overrides`, and not `a.ts`

#### Scenario: A class that implements the member of an interface is listed
- **WHEN** `Aws` implements `GitProvider` and declares `groups`, and impact is asked about `GitProvider.groups`
- **THEN** impact lists the file of `Aws` with the caller `Aws.groups` and the reason `overrides`

#### Scenario: An override through a chain of classes is listed
- **WHEN** `Sub2` extends `Sub`, which extends `Base` and does not declare `place`, `Sub2` declares `place`, and impact is asked about `Base.place`
- **THEN** impact lists the file of `Sub2` with the reason `overrides`

#### Scenario: The member an override overrides is listed
- **WHEN** `Aws` implements `GitProvider`, and impact is asked about `Aws.getGitGroups`
- **THEN** impact lists the file of `GitProvider` with the reason `declares GitProvider.getGitGroups`, and the note about the head stays

#### Scenario: A sibling implementation is not listed
- **WHEN** `Github` also implements `GitProvider` and declares `getGitGroups`, and impact is asked about `Aws.getGitGroups`
- **THEN** impact does not list the file of `Github`

#### Scenario: A constructor is no override
- **WHEN** `Sub` extends `Base`, both declare `constructor`, and impact is asked about `Base.constructor`
- **THEN** impact does not list `sub.ts`

## MODIFIED Requirements

### Requirement: Members that pass on
Asked about a member, `impact` SHALL go on to each name that takes the members of its class and does not declare that member, through the heirs of the heirs, in the same hop. An importer of such a name SHALL be a caller when it uses `Heir.member` or the heir alone. A private member SHALL not pass on. Asked about a class as a whole, `impact` SHALL list what it listed before.

#### Scenario: A caller through a subclass is a caller of the member
- **WHEN** `base.ts` declares `Base` with `place` and `cancel`, `sub.ts` declares `Sub`, which extends `Base`, `a.ts` declares `run`, which uses `Sub.place`, `b.ts` declares `stop`, which uses `Sub.cancel`, and impact is asked about `Base.place` in `base.ts`
- **THEN** impact lists `a.ts` at distance 1 with the caller `run`
- **AND** it does not list `b.ts`

#### Scenario: A subclass that declares the member keeps its own callers
- **WHEN** `Sub` extends `Base` and declares `place`, `a.ts` uses `Sub.place`, and impact is asked about `Base.place`
- **THEN** impact lists `sub.ts` with the caller `Sub.place`
- **AND** it does not list `a.ts`

#### Scenario: A member goes through a chain of subclasses
- **WHEN** `Sub2` extends `Sub`, which extends `Base`, in a third file and within one file, a declaration of another file uses `Sub2.place`, and impact is asked about `Base.place`
- **THEN** impact lists that file at distance 1 with that declaration as its caller

#### Scenario: A top-level value of a class is a caller's way to the member
- **WHEN** `store.ts` has `orders` as an heir of `OrderService`, `page.ts` declares `render`, which uses `orders.place`, `other.ts` declares `clear`, which uses `orders.cancel`, and impact is asked about `OrderService.place`
- **THEN** impact lists `page.ts` at distance 1 with the caller `render`
- **AND** it does not list `other.ts`

#### Scenario: A subclass in the file of its base
- **WHEN** `filemap.ts` declares `FileMap` with `set`, `ModuleMap`, which extends it, and `build`, which uses `ModuleMap.set`, `user.ts` imports `ModuleMap` and declares `load`, which uses `ModuleMap.set`, and impact is asked about `FileMap.set`
- **THEN** impact lists `user.ts` at distance 1 with the caller `load`
- **AND** it lists `build` among the callers in `filemap.ts`

#### Scenario: A private member does not pass on
- **WHEN** `Base` declares `#secret`, `Sub` extends it, `a.ts` uses `Sub.#secret`, and impact is asked about `Base.#secret`
- **THEN** impact lists no file

#### Scenario: A member goes on through a namespace
- **WHEN** `sub.ts` has `import * as lib from './lib'` and `Sub` extends `lib.Base`, `a.ts` uses `Sub.place`, `b.ts` uses `Sub.cancel`, and impact is asked about `Base.place` in `lib.ts`
- **THEN** impact lists `a.ts` and not `b.ts`
