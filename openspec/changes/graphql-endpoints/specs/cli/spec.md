## MODIFIED Requirements

### Requirement: Showing the config

`gsttt config show` SHALL print the effective setup: each module with whether it is on and its options, the GraphQL endpoints, the packs, and each external with whether it is trusted. Under `--json` it SHALL print the setup as JSON.

#### Scenario: config show marks an untrusted external
- **WHEN** the config declares an external that is not trusted, and `gsttt config show` runs
- **THEN** the external is listed as not trusted

#### Scenario: config show lists the GraphQL endpoints
- **WHEN** the config has one `[[graphql_endpoints]]` entry with `clients = ["web/**"]` and `servers = ["api/**", "gateway/**"]`, and `gsttt config show` runs
- **THEN** the section `graphql_endpoints:` lists `clients: web/**; servers: api/**, gateway/**`, and under `--json` the setup holds that entry
- **AND** without an entry the section says `(none)`
