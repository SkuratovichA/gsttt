# Spec Delta

## ADDED Requirements

### Requirement: Operations of a fragment wait for the link
The `openapi` connector SHALL name each operation written at the top level of a fragment as a place that only the link turns into a fact, with its line and its method, since a fragment declares nothing until a document refers to it.

#### Scenario: An operation of a fragment is named as waiting for the link
- **WHEN** a fragment has `get` at line 1 and `post` at line 5 and a document refers to it
- **THEN** the fragment names line 1 with `GET` and line 5 with `POST`
- **AND** the document names nothing
