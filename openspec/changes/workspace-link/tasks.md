# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. Core

- [x] 2.1 `LinkInput::neighbors` and `Project::links_across_projects`
- [x] 2.2 The import closure of the same connector across projects, in `Reach`
- [x] 2.3 A neighbour whose link data does not decode is left out
- [x] 2.4 Linked facts replace the linked facts of other projects and count once

## 3. The TypeScript connector

- [x] 3.1 The project links across projects
- [x] 3.2 A mount keeps the path as written when it holds names of other files
- [x] 3.3 `parents` covers neighbours, `express` and `nestjs` serve their routes under the mounts
- [x] 3.4 The blind spots say what is followed
- [x] 3.5 `READER_VERSION` 27 to 28

## 4. The scenarios

- [x] 4.1 Core: a test for each of the six scenarios of "A link across projects"
- [x] 4.2 TypeScript: a fixture and a test for each scenario of "Constants of other packages in the link", "Routers of other packages in the link" and "Controllers of other packages in the link"
- [x] 4.3 CLI: a test that `impact` of a router in one package reaches the client of the mounted path, named in the `queries` delta

## 5. The documents

- [x] 5.1 `docs/architecture.md` says what a link sees
