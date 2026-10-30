# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the deltas of the `queries`, `cli` and `mcp` specs

## 2. The walks

- [ ] 2.1 The walk over files counts hops through code: each file reached carries the hops through code of the way it was reached by, a contract keeps them and an import adds one, and names that arrive by a way with fewer are followed as far as it allows
- [ ] 2.2 The walk by name counts them for each declaration, load and whole file it reaches, and says it stopped only where code would go on
- [ ] 2.3 The scenarios of `A hop across a contract is no hop through code`

## 3. The tests left out

- [ ] 3.1 `ImpactRequest.no_tests` and `Impact.tests_left_out`, the line `N test files left out`, and the first line of the answer, with the scenarios of `Tests left out of an impact answer`
- [ ] 3.2 `--no-tests` on `gsttt impact`, and an end-to-end test of `gsttt impact --name` without a depth from a method of a service through the resolver to the client of the GraphQL operation
- [ ] 3.3 `tests` on the MCP `impact` tool, with the schema of the listing

## 4. Docs

- [ ] 4.1 The sentences on depth in `docs/architecture.md` and the plugin skill
