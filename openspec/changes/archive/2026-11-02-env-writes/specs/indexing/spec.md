## ADDED Requirements

### Requirement: Writes of a chain
A chain rule SHALL name the key of a site that reads the chain with its `role`, of a site that assigns to it with its `written` role, and of a site that does both with the two. A rule without `written` SHALL name nothing for a site that only assigns. A computed segment SHALL leave one unread site whatever the access.

#### Scenario: A read, a write and a read-write of one chain
- **WHEN** a chain rule has `role = "calls"` and `written = "serves"`, and the chain `process.env.API_URL` is read, written, and read and written
- **THEN** the read names `calls env API_URL`, the write names `serves env API_URL`, and the read-write names both

#### Scenario: A write under a rule with no written role names nothing
- **WHEN** a chain rule has `role = "calls"` and no `written`, and a site only assigns to its chain
- **THEN** the site names no contract and leaves no unread site, and the pack hashes as it did before the field existed

#### Scenario: A computed segment is one unread site whatever the access
- **WHEN** a chain rule has `role = "calls"` and `written = "serves"`, and a read, a write and a read-write of its chain each have a segment that is not a literal
- **THEN** each of the three leaves one unread site and names no contract
