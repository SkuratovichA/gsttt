# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The comparison

- [x] 2.1 `changed_declarations` returns the changed and the removed names, and compares the order of the shared declarations only
- [x] 2.2 `Change` and `ChangedPart` carry `removed`

## 3. The walk

- [x] 3.1 `seed_of` seeds a file that only removed names, and `walk_from` starts the first layer from the importers of the removed names
- [x] 3.2 `exports` and the start from a removed member share one function
- [x] 3.3 The text header names the removed names

## 4. The scenarios

- [x] 4.1 TypeScript: a test for each of the nine scenarios
- [x] 4.2 One test each for Python, Go and Rust
