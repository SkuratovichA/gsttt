# Spec Delta

## MODIFIED Requirements

### Requirement: Config schema
The crate SHALL generate draft-07 JSON Schemas of the config and of a pack file. In the config's, each module's table SHALL be the schema of that module's options, and every table the reader checks for unknown keys SHALL have `additionalProperties` false. Each type SHALL be described once, under `definitions`. No schema SHALL offer `null`, which a TOML file cannot hold, and the table of a module without options SHALL take no keys.

#### Scenario: Options become a schema of the module's table
- **WHEN** the config schema is generated for a registry with a module `ts` whose options have a string `tsconfig` and a list of strings `conditions`, each with a description
- **THEN** `properties.ts`, followed through its reference, describes `tsconfig` as a string and `conditions` as an array of strings, each with its description

#### Scenario: The schema closes the top level
- **WHEN** the config schema is generated
- **THEN** its top level has `additionalProperties` false and a property for every top-level key, for every module of the registry and for `external`

#### Scenario: A pack file has a schema
- **WHEN** the pack schema is generated
- **THEN** it describes `name`, `language`, `detect` and the six kinds of rules, and refuses unknown keys

#### Scenario: No schema offers null
- **WHEN** a module's options have a field that may be left out
- **THEN** the config schema neither allows `null` for that field nor gives it a `null` default

#### Scenario: A module without options takes an empty table
- **WHEN** a module has no options
- **THEN** its table in the config schema is an object that takes no keys
