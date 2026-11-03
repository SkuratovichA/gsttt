# Spec Delta

## ADDED Requirements

### Requirement: Callers through a head
Asked about a member of a class, or run with `--changed` from one, `impact` SHALL also list the callers of the same member of each class or interface the class extends or implements, and of their heads in turn, found through imports and barrels. A holder SHALL count only when its file names the member. A head outside the repository, or one that does not declare the member, SHALL add nothing. A note SHALL name the member and the head.

#### Scenario: A caller through an interface is a caller of the method that implements it
- **WHEN** `GitProvider` declares `getGitGroups`, `Aws` implements it and declares it, `app.ts` holds a `GitProvider` and calls `getGitGroups`, and impact is asked about `Aws.getGitGroups`
- **THEN** impact lists `app.ts` with the caller that calls it
- **AND** it says that `Aws.getGitGroups` may run through `GitProvider.getGitGroups`

#### Scenario: A caller through a base class is a caller of the method that overrides it
- **WHEN** `Base` declares `run`, `Sub` extends it and overrides `run`, `app.ts` holds a `Base` and calls `run`, and impact is asked about `Sub.run`
- **THEN** impact lists `app.ts`

#### Scenario: A sibling implementation is no caller
- **WHEN** `Github` also implements `GitProvider`, a file holds only `Github` and names `getGitGroups`, and impact is asked about `Aws.getGitGroups`
- **THEN** impact does not list that file

#### Scenario: A method the interface does not declare gets nothing from it
- **WHEN** `Aws` implements `GitProvider`, declares `extra`, which `GitProvider` does not declare, a file holds a `GitProvider` and names `extra`, and impact is asked about `Aws.extra`
- **THEN** impact lists no file and says nothing about a head

#### Scenario: A head is followed through a barrel
- **WHEN** `Aws` implements a `GitProvider` it imports with `import type` from a barrel that re-exports it by name, or by `export *`, and a file holds the `GitProvider` and names the method
- **THEN** impact asked about `Aws.getGitGroups` lists that file

#### Scenario: A changed method is followed through what it implements
- **WHEN** `Aws.getGitGroups` changed in the working tree and `app.ts` holds a `GitProvider` and calls `getGitGroups`
- **THEN** `impact --changed` lists `app.ts`

### Requirement: Members of an interface can be asked about
Asked about `Interface.member`, `impact` SHALL list the files that hold the interface and name the member.

#### Scenario: Callers of a member of an interface are the holders that name it
- **WHEN** `GitProvider` declares `getGitGroups` and `other`, `app.ts` holds a `GitProvider` and names `getGitGroups`, `rest.ts` holds one and names `other`, and impact is asked about `GitProvider.getGitGroups`
- **THEN** impact lists `app.ts`
- **AND** it does not list `rest.ts`
