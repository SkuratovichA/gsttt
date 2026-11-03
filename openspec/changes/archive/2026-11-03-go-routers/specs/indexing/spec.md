## ADDED Requirements

### Requirement: Catch-all segments
A path segment that starts with `*` SHALL be a parameter in the key of an HTTP contract, as the segments that start with `:` are.

#### Scenario: A catch-all segment is a parameter
- **WHEN** keys are made from `/files/*path`, `/files/*`, `/files/{path...}` and `/files/:path*`
- **THEN** each has the path `/files/{}`
