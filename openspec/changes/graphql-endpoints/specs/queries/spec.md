## MODIFIED Requirements

### Requirement: Impact across contracts
From every changed and every affected file, `impact` SHALL cross the contracts the file has sites of: a `declares` site reaches every file that serves or calls the contract, a `serves` site reaches every file that calls it, and a `calls` site reaches no file. An HTTP contract SHALL be crossed to the sites of every key that meets it, and a GraphQL contract only between sites that meet in an endpoint. The reason of a file reached across a contract SHALL be its own role and key.

#### Scenario: Changing a schema reaches servers and callers
- **WHEN** `openapi.yaml` declares `http GET /bookings/{}`, `bookings.controller.ts` serves it, `client.ts` calls it, and `openapi.yaml` changes
- **THEN** impact lists `bookings.controller.ts` with the reason `serves http GET /bookings/{}` and `client.ts` with the reason `calls http GET /bookings/{}`

#### Scenario: Changing a server reaches its callers only
- **WHEN** `openapi.yaml` declares `http GET /bookings/{}`, `bookings.controller.ts` serves it, `client.ts` calls it, and `bookings.controller.ts` changes
- **THEN** impact lists `client.ts` and does not list `openapi.yaml`

#### Scenario: Changing a caller reaches no server
- **WHEN** `client.ts` calls `http GET /bookings/{}`, nothing imports `client.ts`, and `client.ts` changes
- **THEN** impact is empty

#### Scenario: A call reaches a route through a parameter
- **WHEN** a controller serves `http GET /bookings/{}`, `app.ts` calls `http GET /bookings/me`, and the controller changes
- **THEN** impact lists `app.ts` with the reason `calls http GET /bookings/me`

#### Scenario: Contract and import hops chain
- **WHEN** a controller serves `http GET /bookings/{}`, `api.ts` calls it, `page.tsx` imports `api.ts`, and the controller changes
- **THEN** impact lists `api.ts` at distance 1 and `page.tsx` at distance 2

### Requirement: Paths between files
`path` SHALL return a shortest chain from one file to another, where a step is an import, a hop from a file that calls a contract to a file that serves or declares it, or a hop from a file that serves a contract to a file that declares it. HTTP hops SHALL follow keys that meet, and GraphQL hops sites that meet in an endpoint. When there is no chain, the answer SHALL say so and how many files are reachable from the first file.

#### Scenario: A chain runs through a contract
- **WHEN** `page.tsx` imports `api.ts`, `api.ts` calls `http GET /bookings/{}`, and `bookings.controller.ts` serves it
- **THEN** the path from `page.tsx` to `bookings.controller.ts` is `page.tsx`, `api.ts` by import, `bookings.controller.ts` by `http GET /bookings/{}`

#### Scenario: No chain is reported with its reach
- **WHEN** a path is asked between two files with no chain between them
- **THEN** the answer says there is none and how many files the first one reaches

## ADDED Requirements

### Requirement: GraphQL endpoints
With endpoints in the index, `impact` and `path` SHALL cross a GraphQL contract only between sites that meet. A site belongs to the endpoints whose globs match its file: a `serves` site by `servers`, a `declares` site by `clients` or `servers`, a `calls` site by `clients` or, when none matches, by `servers`. Two sites meet when the one in the higher role (`calls`, `serves`, `declares`) belongs to no endpoint, or the two share one. Other contracts and the `contract` query SHALL NOT be scoped.

#### Scenario: A change reaches the callers of its own endpoint only
- **WHEN** the endpoint `a` has the clients `web-a/**` and the servers `api-a/**`, the endpoint `b` has `web-b/**` and `api-b/**`, `api-a/users.ts` and `api-b/users.ts` serve `graphql Query.user`, `web-a/page.ts` and `web-b/page.ts` call it, and `api-a/users.ts` changes
- **THEN** impact lists `web-a/page.ts` with the reason `calls graphql Query.user` and does not list `web-b/page.ts`

#### Scenario: A chain runs within an endpoint only
- **WHEN** the endpoints and files above are given and a path is asked from `web-a/page.ts`
- **THEN** the path to `api-a/users.ts` is `web-a/page.ts` and `api-a/users.ts`, and there is no path to `api-b/users.ts`

#### Scenario: A schema reaches the sites of its own endpoint only
- **WHEN** the endpoints and files above are given, `api-a/schema.graphql` and `api-b/schema.graphql` declare `graphql Query.user`, and `api-a/schema.graphql` changes
- **THEN** impact lists the server and the page of `a` and no file of `b`
- **AND** a path from `web-b/page.ts` reaches the schema of `b` and not the schema of `a`

#### Scenario: A site in no endpoint meets by the role it has
- **WHEN** the endpoints and files above are given, and `scripts/probe.ts` calls `graphql Query.user` and `legacy/users.ts` serves it, both matching no glob
- **THEN** a change of `api-a/users.ts` reaches `scripts/probe.ts`, and a path from `scripts/probe.ts` reaches the servers of both endpoints and `legacy/users.ts`
- **AND** a change of `legacy/users.ts` reaches `scripts/probe.ts` and no page of an endpoint, and no path leads from `web-a/page.ts` to `legacy/users.ts`

#### Scenario: A file that serves one endpoint and calls another crosses both
- **WHEN** the first endpoint has the clients `web/**` and the servers `gateway/**` and `schema-a/**`, the second has the clients `gateway/**` and the servers `api/**`, `gateway/graph.ts` serves and calls `graphql Query.user`, `schema-a/user.graphql` declares it, `api/users.ts` serves it, `web/page.ts` calls it, and `entry.ts` imports the gateway
- **THEN** a path from `entry.ts` reaches `schema-a/user.graphql` and `api/users.ts`, each through the gateway
- **AND** a path from `web/page.ts` reaches `api/users.ts` through the gateway, and a change of `api/users.ts` reaches the gateway and `web/page.ts`

#### Scenario: Without endpoints a GraphQL contract meets every site
- **WHEN** the files of the first scenario are given and the index has no endpoints
- **THEN** a change of `api-a/users.ts` reaches both pages, and a path from `web-a/page.ts` reaches `api-b/users.ts`

#### Scenario: Endpoints do not scope other contracts
- **WHEN** the endpoints and files of the first scenario serve and call `http GET /user` in place of the GraphQL key
- **THEN** a change of `api-a/users.ts` reaches both pages

#### Scenario: The contract query lists every site whatever the endpoints
- **WHEN** the endpoints and files of the first scenario are given and `contract` is asked for `graphql Query.user`
- **THEN** it lists `api-a/users.ts` and `api-b/users.ts` as servers and `web-a/page.ts` and `web-b/page.ts` as callers

#### Scenario: A walk by name crosses a GraphQL contract within an endpoint
- **WHEN** the endpoints and files of the first scenario are given, `lib/user.ts`, which matches no glob, declares `loadUser`, and `api-a/users.ts` uses it in the resolver that serves `graphql Query.user`
- **THEN** impact asked about `loadUser` lists `api-a/users.ts` and `web-a/page.ts` and not `web-b/page.ts`
