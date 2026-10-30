# Spec Delta

## ADDED Requirements

### Requirement: Impact without tests over MCP
`impact` SHALL take `tests`, a boolean that is true unless the call gives it. With `tests` false the answer SHALL list no tests and say how many test files it left out, as the query does when asked to leave the tests out. The schema of `impact` SHALL describe `tests` and its default.

#### Scenario: impact leaves the tests out
- **WHEN** `a.ts` is imported by `b.ts` and by the test file `a.test.ts`, and `impact` is called with `paths` set to `["a.ts"]` and `tests` set to false
- **THEN** the answer lists `b.ts`, lists no tests and says it left out 1 test file, as `gsttt impact a.ts --no-tests` does

#### Scenario: A tests argument that is not a boolean is a tool error
- **WHEN** `impact` is called with `paths` set to `["a.ts"]` and `tests` set to `"no"`
- **THEN** the result has `isError` set and names `tests` and says it must be true or false

## MODIFIED Requirements

### Requirement: Tools

The server SHALL offer the tools `impact` (`paths`, `changed`, `base`, `depth`, `names`, `tests`, `all`), `dependents` (`path`, `all`), `dependencies` (`path`, `all`), `path` (`from`, `to`, `all`), `contract` (`query`, `all`), `coverage` (`all`) and `index_status` (no arguments). Each tool SHALL have a description that says what it answers and when to call it, and an input schema of its arguments. Paths SHALL be read from the root, by the same rule the CLI uses.

#### Scenario: tools/list names the seven tools
- **WHEN** a client sends `tools/list`
- **THEN** the result lists `impact`, `dependents`, `dependencies`, `path`, `contract`, `coverage` and `index_status`, each with a description and an input schema

#### Scenario: A path argument is read from the root
- **WHEN** `dependents` is called with `path` set to `lib/util.ts`
- **THEN** the answer is about `lib/util.ts`

#### Scenario: A path outside the root is a tool error
- **WHEN** `dependents` is called with `path` set to `../elsewhere.ts`
- **THEN** the result has `isError` set and says the path is outside the repository
