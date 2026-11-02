## MODIFIED Requirements

### Requirement: Config keys
The top level of the config SHALL hold `exclude`, `http_prefixes`, `max_file_bytes`, `detect`, `disable` and `disable_packs`, one table per module of the registry, which turns that module on with the table as its options, `[[external]]` entries and `[[graphql_endpoints]]` entries. Modules SHALL enter the setup in the registry's order, externals and endpoints in the file's order, and `disable` SHALL name known modules.

#### Scenario: The keys fill the setup
- **WHEN** the config sets `exclude = ["fixtures/**"]`, `http_prefixes = ["/api"]`, `disable = ["openapi"]` and `disable_packs = ["axios"]`, has a `[ts]` table with `tsconfig = "tsconfig.base.json"`, and an `[[external]]` named `kotlin` with `cmd = ["gsttt-kotlin"]`, `files = ["**/*.kt"]`, `version = "1"` and `env = ["JAVA_HOME"]`, and a `[[graphql_endpoints]]` entry with `clients = ["web/**/*.ts"]` and `servers = ["api/src/**/*.ts"]`
- **THEN** the setup holds each of those values, matching the reviewed snapshot

#### Scenario: An empty table turns a module on
- **WHEN** the config sets `detect = false` and holds an empty `[python]` table
- **THEN** the setup runs `python` with no options and runs no other module

#### Scenario: The order of the tables is not the order of the modules
- **WHEN** the config has a `[rust]` table before a `[ts]` table
- **THEN** the setup lists `ts` before `rust`, in the order of the registry

#### Scenario: Disable names a known module
- **WHEN** the config sets `disable = ["tss"]`
- **THEN** evaluation fails with a message naming `tss` and listing the known modules

### Requirement: Fingerprint
The setup SHALL have a fingerprint computed from its canonical JSON form, so that an edit that changes no value leaves it unchanged.

#### Scenario: Comments do not change the fingerprint
- **WHEN** a comment is added to `.gsttt/config.toml`
- **THEN** the fingerprint of the setup is unchanged, and changing `http_prefixes`, an option of a module, an option of an external or a glob of a GraphQL endpoint changes it

#### Scenario: Moving a module's table does not change the fingerprint
- **WHEN** the `[ts]` table moves from the top of `.gsttt/config.toml` to its end, or the keys inside it change places
- **THEN** the fingerprint of the setup is unchanged

## ADDED Requirements

### Requirement: GraphQL endpoints
A `[[graphql_endpoints]]` entry SHALL hold `clients` and `servers`, each a non-empty list of globs matched against repository-relative paths. The globs SHALL be checked as the config is read, and a missing key, an empty list or a glob that does not compile SHALL fail with the line of the entry, the place `graphql_endpoints[n].clients` or `graphql_endpoints[n].servers`, and, for a glob, the glob. The endpoints SHALL be kept in the index, so that a query reads them from the index it is asked of.

#### Scenario: Endpoints enter the setup in the order of the file
- **WHEN** the config has two `[[graphql_endpoints]]` entries, the first with `servers = ["b/**"]` and `clients = ["a/**", "c/*.ts"]`, the second with `clients = ["d/**"]` and `servers = ["e/**"]`
- **THEN** the setup lists the first entry and then the second, each with its globs in the order written

#### Scenario: An endpoint needs a client glob and a server glob
- **WHEN** an entry has no `servers` or no `clients`, or has `clients = []` or `servers = []`
- **THEN** evaluation fails with a message that starts with `.gsttt/config.toml:` and the line of the entry
- **AND** it names the missing key or, for an empty list, `graphql_endpoints[0].clients` or `graphql_endpoints[0].servers`, and says it needs at least one glob

#### Scenario: An invalid glob of an endpoint is named
- **WHEN** the first `[[graphql_endpoints]]` entry has `servers = ["api/**", "["]`
- **THEN** evaluation fails with a message that starts with `.gsttt/config.toml:1:` and names `graphql_endpoints[0].servers` and the glob `[`
