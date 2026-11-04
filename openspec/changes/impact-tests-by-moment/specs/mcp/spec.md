# Spec Delta

## MODIFIED Requirements

### Requirement: Impact without tests over MCP
`impact` SHALL take `tests`, a boolean that is true when the call gives `changed` and false otherwise, unless the call gives it. With `tests` false the answer SHALL list no tests and say how many test files it left out and how to ask for them, `ask with tests=true`, as the query does when asked to leave the tests out. The schema of `impact` SHALL describe `tests` and its default.

#### Scenario: impact leaves the tests out
- **WHEN** `a.ts` is imported by `b.ts` and by the test file `a.test.ts`, and `impact` is called with `paths` set to `["a.ts"]` and `tests` set to false
- **THEN** the answer lists `b.ts`, lists no tests and says it left out 1 test file and that they are listed when asked with `tests=true`, as `gsttt impact a.ts --no-tests` does

#### Scenario: impact counts the tests while the edit is still being worked out
- **WHEN** `a.ts` is imported by `b.ts` and by the test file `a.test.ts`, and `impact` is called with `paths` set to `["a.ts"]` and no `tests`
- **THEN** the answer lists `b.ts`, lists no tests and says it left out 1 test file and that they are listed when asked with `tests=true`

#### Scenario: impact lists the tests when asked
- **WHEN** `a.ts` is imported by `b.ts` and by the test file `a.test.ts`, and `impact` is called with `paths` set to `["a.ts"]` and `tests` set to true
- **THEN** the answer lists `b.ts` and `a.test.ts`, and says nothing was left out

#### Scenario: impact lists the tests of a change
- **WHEN** `a.ts` is modified and imported by the test file `a.test.ts`, and `impact` is called with `changed` set to true, and then with `changed` set to true and `tests` set to false
- **THEN** the first answer lists `a.test.ts`, and the second lists no test and says it left out 1 test file

#### Scenario: A tests argument that is not a boolean is a tool error
- **WHEN** `impact` is called with `paths` set to `["a.ts"]` and `tests` set to `"no"`
- **THEN** the result has `isError` set and names `tests` and says it must be true or false

## ADDED Requirements

### Requirement: Instructions name the moment of work
The `initialize` instructions SHALL tell the model to call `impact` before it edits a file that other files import or call and again with `changed` set to true before it finishes a change, and a client that lists the tools by name only SHALL be told to load the schema of `impact` before the first call.

#### Scenario: The instructions say when in the work to call impact
- **WHEN** a client sends `initialize`
- **THEN** the `instructions` say to call `impact` before editing a file that other files import or call, to call it again with `changed=true` before finishing a change, and to load its schema first when the tools are listed by name only
