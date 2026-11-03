# Spec Delta

## MODIFIED Requirements

### Requirement: The registry

The binary SHALL register the built-in connectors in this order: `ts`, `python`, `rust`, `go`, `openapi`, `graphql`, `proto`, `prisma`, `sql`, `env`, followed by the trusted external connectors in the order the config declares them. The built-in packs SHALL be read once per run.

#### Scenario: config show lists the modules in registry order
- **WHEN** `gsttt config show` runs in a repository with no config
- **THEN** it lists `ts`, `python`, `rust`, `go`, `openapi`, `graphql`, `proto`, `prisma`, `sql` and `env` in that order, each with its options
