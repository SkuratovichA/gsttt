# Spec Delta

## ADDED Requirements

### Requirement: A chain is matched against roots of at most 64 segments
A chain of members SHALL be matched against the roots of the chain rules up to 64 segments long, whatever its length, so that the cost of a chain grows with its length and not with its square. A rule keeps the contract of a chain that starts with its root and goes on for any number of members.

#### Scenario: A long chain still matches a root of a rule
- **WHEN** a file reads `process.env.PORT` followed by 300 members
- **THEN** the file calls `env PORT` at its line
- **AND** the run has no problem line
