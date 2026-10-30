# Spec Delta

## Purpose

Agent benchmarks are the comparison of the README and the site of what Claude Code spends and how right it is when it answers questions about how the code of a pinned repository is connected: alone, with a TypeScript language server, with codebase-memory-mcp and with the gsttt plugin. The questions and their answers are checked in, and anyone with a logged-in Claude Code can rerun every number from the repository.

## ADDED Requirements

### Requirement: Questions about clients
A question of the kind `clients` SHALL name a method with a body of a class of the scope that is not a resolver, and its answer SHALL be every tracked file of the scope that holds a GraphQL operation selecting, at its top level, a field that a resolver calling the method directly serves. A resolver is a method decorated with `@Query`, `@Mutation` or `@Subscription` of `@nestjs/graphql` in a class decorated with `@Resolver`. A method SHALL be drawn only when 1 to 30 files make its answer.

#### Scenario: A method with no client or too many clients is not drawn
- **WHEN** the candidates are a method with no client, one with 1, one with 30 and one with 31
- **THEN** only the methods with 1 and with 30 clients become questions

#### Scenario: The prompt asks which files send an operation
- **WHEN** the prompt of a question about clients is written
- **THEN** it names the method and the file that defines it, says what a resolver and an operation are, and ends as the prompt of a question about users does

### Requirement: The answer of a question about clients
The clients of a method SHALL be computed without gsttt. A resolver calls the method when the TypeScript language service finds a reference to it in the text of that resolver. Documents are read by the `graphql` package from `.graphql` and `.gql` files and from `gql` and `graphql` tagged templates, and an operation selects a field of the root type of its kind. A document that does not parse SHALL stop the draw.

#### Scenario: The clients of a method are the files that select the field of a resolver that calls it
- **WHEN** a resolver that serves a mutation calls a method, a query of the same name is selected by another file, and an alias and a fragment select the fields of two other resolvers
- **THEN** the mutation's clients are the files that select it alone, an alias and a fragment count, and a method that only a class that is not a resolver calls is not listed

### Requirement: The place of the questions about clients
The questions of a repository SHALL be in `bench/agents/questions.json` with those about users first, those about tests next and those about clients last, so that drawing the questions about clients leaves the questions drawn before them as they were.

#### Scenario: Questions about clients come after the others of their repository
- **WHEN** a question about clients stands between the questions about users and the ones about tests of a repository
- **THEN** the questions are refused, and with the order users, tests, clients they are read back as they were written

### Requirement: A repository that asks about one kind
A repository SHALL ask for any of the kinds `users`, `tests` and `clients` alone or together, and an `agents` table that asks for none, or for a kind with a count of zero, SHALL be refused naming the repository.

#### Scenario: A repository can ask only about the clients of methods
- **WHEN** a repository has an `agents` table with `clients = 6` and neither `users` nor `tests`
- **THEN** its pins are valid and ask for 6 questions about clients, and `clients = 0` or no kind at all is refused
