# Spec Delta

## Purpose

The MCP server answers an agent's questions about a repository over the Model Context Protocol, on the standard streams of `gsttt mcp`. It has a tool for each question the command line answers, and answers a model in the text the command line prints, with lists cut unless it asks for all of them. Every tool but `index_status` refreshes the index first, so that an answer is never older than the question. The server takes its index from a source: the binary serves the repository around the directory it was started in, and the tests serve a fake.

## ADDED Requirements

### Requirement: Startup without the repository

The server SHALL answer `initialize` and `tools/list` without calling its index source, so that it starts at once in any directory, including one where indexing would fail. The `initialize` result SHALL carry instructions that say what gsttt answers and when to call each kind of tool.

#### Scenario: initialize and tools/list do not touch the repository
- **WHEN** a client sends `initialize` and `tools/list` to a server whose source fails every call
- **THEN** both succeed and the source was not called

#### Scenario: initialize carries instructions
- **WHEN** a client sends `initialize`
- **THEN** the result's `instructions` say when to call `impact` and that coverage gaps must be reported

### Requirement: Tools

The server SHALL offer the tools `impact` (`paths`, `changed`, `base`, `depth`, `all`), `dependents` (`path`, `all`), `dependencies` (`path`, `all`), `path` (`from`, `to`, `all`), `contract` (`query`, `all`), `coverage` (`all`) and `index_status` (no arguments). Each tool SHALL have a description that says what it answers and when to call it, and an input schema of its arguments. Paths SHALL be read from the root, by the same rule the CLI uses.

#### Scenario: tools/list names the seven tools
- **WHEN** a client sends `tools/list`
- **THEN** the result lists `impact`, `dependents`, `dependencies`, `path`, `contract`, `coverage` and `index_status`, each with a description and an input schema

#### Scenario: A path argument is read from the root
- **WHEN** `dependents` is called with `path` set to `lib/util.ts`
- **THEN** the answer is about `lib/util.ts`

#### Scenario: A path outside the root is a tool error
- **WHEN** `dependents` is called with `path` set to `../elsewhere.ts`
- **THEN** the result has `isError` set and says the path is outside the repository

### Requirement: Answers

Every tool except `index_status` SHALL refresh the index through the source and return one text block holding the answer's text as the CLI prints it, with lists cut unless `all` is true. `index_status` SHALL read the stored index without refreshing it, and SHALL answer in plain text when there is no index yet.

#### Scenario: A tool answers with the CLI's text
- **WHEN** `dependents` is called for a file that two files import
- **THEN** the result is one text block equal to the answer's text, and the source was refreshed once

#### Scenario: all lists every entry
- **WHEN** `dependents` is called with `all` set to true for a file that 45 files import
- **THEN** the text lists all 45 files

#### Scenario: index_status does not refresh
- **WHEN** `index_status` is called
- **THEN** the source was asked for the stored index and was not refreshed

#### Scenario: index_status without an index answers in text
- **WHEN** `index_status` is called and the source has no stored index
- **THEN** the result is not an error and says there is no index yet and that the first question builds one

### Requirement: Errors

A call to a known tool whose `arguments` is an object or is left out SHALL never fail with a protocol error. Arguments that do not match the tool's schema, conflicting arguments, a failed refresh and a source error SHALL each be a result with `isError` set and a plain-English text that names the argument or the failure and says what to do. A call to an unknown tool SHALL be the protocol error the MCP specification prescribes.

#### Scenario: A malformed argument is a tool error
- **WHEN** `impact` is called with `paths` set to the number 3
- **THEN** the result has `isError` set and its text names `paths` and says it must be a list of paths, and no JSON-RPC error is sent

#### Scenario: Conflicting arguments are a tool error
- **WHEN** `impact` is called with both `paths` and `changed` set to true
- **THEN** the result has `isError` set and says to pass either `paths` or `changed`

#### Scenario: A failed refresh is a tool error
- **WHEN** `coverage` is called and the source fails with a message
- **THEN** the result has `isError` set and holds that message

### Requirement: Files the index does not hold

`dependents`, `dependencies` and `path`, asked about a file the index does not hold, SHALL each be a result with `isError` set and the text the CLI prints for it.

#### Scenario: A file the index does not hold is a tool error
- **WHEN** `dependents`, `dependencies` or `path` is called with a file that no connector read
- **THEN** the result has `isError` set and its text is the one the CLI prints on stderr for the same question, which starts with the file's path and "is not in the index"

### Requirement: Malformed requests

A `tools/call` whose `arguments` is not an object SHALL be answered with the JSON-RPC error for invalid parameters, and the server SHALL keep serving.

#### Scenario: Arguments that are not an object are refused
- **WHEN** `coverage` is called with `arguments` set to `[]`, to `"x"`, to the number 3 or to true
- **THEN** each call is answered with the JSON-RPC error for invalid parameters, and a call after them is answered as usual

### Requirement: One refresh at a time

Calls into the source SHALL run one at a time, on a blocking thread, so that a long refresh neither runs twice at once nor stalls the transport.

#### Scenario: Concurrent calls refresh one at a time
- **WHEN** eight tool calls arrive at once
- **THEN** all eight answer, and the source never had two calls in progress at the same time

### Requirement: Lifecycle

stdout SHALL carry only protocol messages, with logs on stderr. The server SHALL stop when stdin closes, and `gsttt mcp` SHALL then exit 0.

#### Scenario: The server stops when its input closes
- **WHEN** the client closes the server's input after `initialize`
- **THEN** `serve` returns without an error

#### Scenario: gsttt mcp exits 0 when its input closes
- **WHEN** the client closes the input of `gsttt mcp` after `initialize`
- **THEN** it exits 0 within 10 seconds

#### Scenario: stdout carries only the protocol
- **WHEN** `gsttt mcp` runs with `GSTTT_LOG=debug` through `initialize`, `tools/list` and one tool call
- **THEN** every line on stdout parses as a JSON-RPC message

### Requirement: The binary serves its repository

`gsttt mcp` SHALL serve the repository found from the directory it was started in, as the CLI finds it, opening it again on every call, so that edits to the config and the trust store take effect without a restart. A directory the CLI refuses SHALL fail the call, not the server. `gsttt mcp` SHALL refuse `--json`, `--all` and `--no-refresh` as a usage error.

#### Scenario: gsttt mcp answers about its repository
- **WHEN** `gsttt mcp` is started in a repository and `dependents` is called for a file that another file imports
- **THEN** the text lists the importing file

#### Scenario: A refused root fails the call, not the server
- **WHEN** `gsttt mcp` is started in the home directory and `coverage` is called
- **THEN** `initialize` succeeded, the result has `isError` set and says gsttt does not index the home directory, and the server keeps running

#### Scenario: gsttt mcp refuses the flags of the command line
- **WHEN** `gsttt mcp` runs with `--json`, with `--all` or with `--no-refresh`
- **THEN** it exits 2, stdout is empty, and stderr names the flag
