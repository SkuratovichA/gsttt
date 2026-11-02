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
A `[[graphql_endpoints]]` entry SHALL hold `clients` and `servers`, each a list of globs matched against repository-relative paths, and at least one glob in the two together. The globs SHALL be checked as the config is read, and an entry with no glob or a glob that does not compile SHALL fail with the line of the entry, its place `graphql_endpoints[n]`, and, for a glob, the list and the glob. The endpoints SHALL be kept in the index, so that a query reads them from the index it is asked of.

#### Scenario: Endpoints enter the setup in the order of the file
- **WHEN** the config has two `[[graphql_endpoints]]` entries, the first with `servers = ["b/**"]` and `clients = ["a/**", "c/*.ts"]`, the second with `clients = ["d/**"]` and `servers = ["e/**"]`
- **THEN** the setup lists the first entry and then the second, each with its globs in the order written

#### Scenario: An endpoint needs one glob
- **WHEN** an entry has neither key, or has `clients = []` and `servers = []`
- **THEN** evaluation fails with a message that starts with `.gsttt/config.toml:` and the line of the entry
- **AND** it names `graphql_endpoints[0]` and says it needs at least one glob in `clients` or `servers`

#### Scenario: An endpoint may list one side only
- **WHEN** an entry has `clients = ["web/**"]` and no `servers`
- **THEN** the setup holds the endpoint with those clients and no servers

#### Scenario: An invalid glob of an endpoint is named
- **WHEN** the first `[[graphql_endpoints]]` entry has `servers = ["api/**", "["]`
- **THEN** evaluation fails with a message that starts with `.gsttt/config.toml:1:` and names `graphql_endpoints[0].servers` and the glob `[`

#### Scenario: The endpoints are kept in the index and in its snapshot
- **WHEN** the setup has the endpoint `a` with the clients `web-a/**` and the servers `api-a/**` and the endpoint `b` with `web-b/**` and `api-b/**`, a server and a page of each serve and call `graphql Query.user`, and the tree is indexed twice, the second time from the snapshot
- **THEN** both indexes hold the endpoints, and a change of `api-a/users.ts` reaches the page of `a` and not the page of `b` in both

### Requirement: A glob of an endpoint that matches no file is reported
When the setup has endpoints, each glob of `clients` or `servers` that matches no file of the index SHALL be one line of the coverage problems, naming `graphql_endpoints[n].clients` or `graphql_endpoints[n].servers` and the glob. The lines SHALL be worked out when the index is built and kept in it. A file that no endpoint lists SHALL give no line.

#### Scenario: A glob of an endpoint that matches no file is named in the coverage
- **WHEN** the first of two endpoints has `servers = ["./api-a/**"]` and the second has `clients = ["./web-b/**"]`, and paths are relative to the repository root and have no `./`
- **THEN** the coverage problems hold two lines, the first `` `graphql_endpoints[0].servers` has the glob "./api-a/**", which matches no indexed file (git-ignored and excluded files are not indexed) `` and the second the same about `graphql_endpoints[1].clients` and `"./web-b/**"`
- **AND** an index read from the snapshot holds the same lines

#### Scenario: A file in no endpoint gives no line
- **WHEN** every glob of the endpoints matches a file, and `legacy/users.ts` matches none of them
- **THEN** the coverage problems hold no line about the endpoints
