## ADDED Requirements

### Requirement: A call rule that lists receiver types reads nothing
A call rule with `receiver_types` SHALL read nothing in Python, because the connector knows no declared type and reading every receiver would make false edges.

#### Scenario: A call rule that lists receiver types reads nothing
- **WHEN** a pack has a call rule on `client.send` with `receiver_types` and a file calls `client.send("topic")`
- **THEN** the file calls no contract and has no unread site
