## ADDED Requirements

### Requirement: Packs that find documents in the provides
The provides of a project SHALL name every pack whose `[[embeds]]` rules find a document, in `via` of the calls of the contract that document holds.

#### Scenario: The graphql pack is credited with the documents it finds
- **WHEN** a project that depends on `graphql-tag` has a file with a `gql` tagged template
- **THEN** the provides name `graphql` in `via` of the `graphql` calls
