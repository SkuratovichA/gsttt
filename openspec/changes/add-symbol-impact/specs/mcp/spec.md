# Spec Delta

## MODIFIED Requirements

### Requirement: Tools

The server SHALL offer the tools `impact` (`paths`, `changed`, `base`, `depth`, `names`, `all`), `dependents` (`path`, `all`), `dependencies` (`path`, `all`), `path` (`from`, `to`, `all`), `contract` (`query`, `all`), `coverage` (`all`) and `index_status` (no arguments). Each tool SHALL have a description that says what it answers and when to call it, and an input schema of its arguments. Paths SHALL be read from the root, by the same rule the CLI uses.

#### Scenario: tools/list names the seven tools
- **WHEN** a client sends `tools/list`
- **THEN** the result lists `impact`, `dependents`, `dependencies`, `path`, `contract`, `coverage` and `index_status`, each with a description and an input schema

#### Scenario: A path argument is read from the root
- **WHEN** `dependents` is called with `path` set to `lib/util.ts`
- **THEN** the answer is about `lib/util.ts`

#### Scenario: A path outside the root is a tool error
- **WHEN** `dependents` is called with `path` set to `../elsewhere.ts`
- **THEN** the result has `isError` set and says the path is outside the repository

## ADDED Requirements

### Requirement: Impact by name over MCP
`impact` SHALL take `names`, the names to ask about in its `paths`, and answer as the query does for them; `names` with `changed`, and a blank entry of `names`, SHALL be a tool error. The description of `impact` and the server instructions SHALL say that a question about one function, class, method or variable is asked with its name, and that the answer lists its direct callers.

#### Scenario: impact takes names
- **WHEN** `b.ts` uses `foo` of `a.ts` in `run`, and `impact` is called with `paths` set to `["a.ts"]` and `names` set to `["foo"]`
- **THEN** the answer lists `b.ts` with the caller `run`, as `gsttt impact a.ts --name foo` does

#### Scenario: names with changed is a tool error
- **WHEN** `impact` is called with `changed` set to true and `names` set to `["foo"]`
- **THEN** the result has `isError` set and says that `names` needs `paths`

#### Scenario: A blank name is a tool error
- **WHEN** `impact` is called with `paths` set to `["a.ts"]` and `names` set to `[""]`
- **THEN** the result has `isError` set, says how to write a name, and the index is not asked

#### Scenario: The description of impact speaks of names
- **WHEN** a client sends `tools/list`
- **THEN** the description of `impact` says that `names` asks about a function, class, method or variable and that the answer lists its direct callers
