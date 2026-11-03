# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. Core

- [x] 2.1 `Linked.overrides`, `LinkedOverride`, and their column in the symbols table
- [x] 2.2 `overridden` and `overriding` follow the linked heads
- [x] 2.3 `FACT_FORMAT` 19 to 20 and `LAYOUT` 12 to 13

## 3. The Go connector

- [x] 3.1 The parse keeps methods, interfaces, embedded types and generic types, with the normal form of a signature
- [x] 3.2 The link of method sets, in `Project::link`
- [x] 3.3 `READER_VERSION` 4 to 5
- [x] 3.4 A function type is read by its types, a generic embedded interface is a head, and a type that lends a method takes the head too

## 4. The scenarios

- [x] 4.1 Core: a test for "A head found across files is followed"
- [x] 4.2 Go: a test for each scenario of the requirements on interfaces, and one through the CLI
- [x] 4.4 Go: a test for each of the three scenarios on function types, generic embeds and lenders
- [x] 4.3 A proptest for the normal form of a type

## 5. Documents

- [x] 5.1 The README and `docs/architecture.md`
