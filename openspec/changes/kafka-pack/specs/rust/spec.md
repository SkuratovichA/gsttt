## ADDED Requirements

### Requirement: A call rule that lists receiver types reads nothing
A call rule with `receiver_types` SHALL read nothing in Rust, because the connector knows no declared type and reading every receiver would make false edges.

#### Scenario: A call rule that lists receiver types reads nothing
- **WHEN** a pack has a call rule on `*.get` with `receiver_types` and a file calls `self.http.get(url)`
- **THEN** the file calls no contract of that rule
