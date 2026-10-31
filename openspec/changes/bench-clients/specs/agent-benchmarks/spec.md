# Spec Delta

## Purpose

Agent benchmarks are the comparison of the README and the site of what Claude Code spends and how right it is when it answers questions about how the code of a pinned repository is connected: alone, with a TypeScript language server, with codebase-memory-mcp and with the gsttt plugin. The questions and their answers are checked in, and anyone with a logged-in Claude Code can rerun every number from the repository.

## ADDED Requirements

### Requirement: Questions about clients
A question of the kind `clients` SHALL name a method with a body of a class that is no resolver, in TypeScript below the tsconfig's directory. Its answer SHALL be every tracked file of the scope with a GraphQL operation selecting, at its top level, a field that a resolver calling the method directly serves. A resolver is a `@Resolver` class method with `@Query`, `@Mutation` or `@Subscription`. A method SHALL be drawn only with 1 to 30 files in its answer and no such field under two root types.

#### Scenario: A method with no client or too many clients is not drawn
- **WHEN** the candidates are a method with no client, one with 1, one with 30 and one with 31
- **THEN** only the methods with 1 and with 30 clients become questions

#### Scenario: The prompt asks which files send an operation
- **WHEN** the prompt of a question about clients is written
- **THEN** it names the method and the file that defines it, says what a resolver and an operation are, and ends as the prompt of a question about users does

### Requirement: The answer of a question about clients
The clients of a method SHALL be computed without gsttt. A resolver calls the method when the TypeScript language service finds a reference to it in the text of that resolver. Documents are read by the `graphql` package from `.graphql` and `.gql` files and `gql` and `graphql` templates, and an operation selects a field of the root type of its kind. A document that does not parse and a decorator argument that hides the field name SHALL stop the draw.

#### Scenario: The clients of a method are the files that select the field of a resolver that calls it
- **WHEN** a resolver that serves a mutation calls a method, a query of the same name is selected by another file, and an alias and a fragment select the fields of two other resolvers
- **THEN** the mutation's clients are the files that select it alone, an alias and a fragment count, and a method that only a class that is not a resolver calls is not listed

### Requirement: Resolvers and documents as the stacks write them
The oracle SHALL read the decorators of `@nestjs/graphql` whether a file imports them by name or as a namespace (`@graphql.Resolver()`), and SHALL read as a document a call of `gql` or `graphql` whose first argument is a template or a string, as the client preset of graphql-codegen writes it, as well as a tagged template.

#### Scenario: A namespace import of the decorators is read like named ones
- **WHEN** a resolver file imports `@nestjs/graphql` as a namespace and decorates its class and methods with `@graphql.Resolver()`, `@graphql.Query()` and `@graphql.Mutation()`
- **THEN** its methods serve the fields the decorators name, as with named imports

#### Scenario: A call of gql or graphql with a template or a string is a document
- **WHEN** a script calls `graphql` with a template literal, with a template with an expression or with a string, and once with a name
- **THEN** the first three are documents whose operations and fragments count, and the call with a name is not

### Requirement: The answer by field
With `--fields` the oracle SHALL print, in place of the methods, each root field (`Query.user`) that a resolver serves or a document selects, with the methods that serve it and the files that select it, sorted so that the same checkout gives the same bytes.

#### Scenario: The fields flag lists each root field with its servers and its clients
- **WHEN** the script runs with `--fields` over a tree where a field is served under two root types, one is only selected by a document and one is only served by a resolver
- **THEN** it prints one entry for each `Root.field`, sorted, with the resolver methods that serve it and the files that select it, and no list of methods

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
