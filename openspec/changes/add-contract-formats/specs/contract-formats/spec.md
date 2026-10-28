# Spec Delta

## Purpose

The contract-format connectors read the files that define seams rather than implement them: OpenAPI documents, GraphQL schemas and operations, protobuf services, Prisma schemas, SQL and env files. Their sites are mostly declarations, which is what lets a change to a schema reach the code that serves and calls it. The GraphQL and SQL connectors also read the documents other connectors find embedded in code.

## ADDED Requirements

### Requirement: Connectors and files
The crate SHALL provide the connectors `openapi`, `graphql`, `proto`, `prisma`, `sql` and `env`. `openapi`, `proto`, `prisma` and `env` SHALL each open one project at the repository root when the file list holds a file it claims, and none otherwise; `graphql` and `sql` SHALL always run and open one, because code in any language can embed their documents.

#### Scenario: A connector opens a project only for files it claims
- **WHEN** the file list holds `schema.graphql` and `.env.example` and no other contract file
- **THEN** the `graphql`, `sql` and `env` connectors each open one project at the repository root
- **AND** the `openapi`, `proto` and `prisma` connectors open none

#### Scenario: Embedded SQL is read without an SQL file
- **WHEN** the file list holds no file that `sql` claims and a file of another connector embeds `SELECT id FROM bookings`
- **THEN** the `sql` connector reads it and that file calls `table bookings`

### Requirement: File claims
`openapi` SHALL claim `openapi.*`, `swagger.*` and `*.openapi.*` with the extensions `yaml`, `yml` and `json`, the files its `files` option names, and, as fragments, the `yaml`, `yml` and `json` files that their `$ref`s reach; `graphql` SHALL claim `graphql`, `graphqls` and `gql` files; `proto` SHALL claim `proto` files; `prisma` SHALL claim `prisma` files; `sql` SHALL claim `sql` files; and `env` SHALL claim `.env`, `.env.*` and `*.env` files.

#### Scenario: Each format claims its files
- **WHEN** the file list holds `api/openapi.yaml`, which refers to `./paths/bookings.yaml`, and `api/paths/bookings.yaml`, `schema.graphql`, `proto/booking/v1/booking.proto`, `prisma/schema.prisma`, `migrations/001_init.sql`, `.env.example` and `config/app.yaml`
- **THEN** each of the first seven belongs to the matching connector's project, and `config/app.yaml` is claimed by none of them

### Requirement: Parse failures
A file a format's parser cannot read SHALL be a syntax failure with its line, and a parser that recovers SHALL give a recovered status with the number of errors and the line of the first.

#### Scenario: A broken document fails alone
- **WHEN** `api/openapi.yaml` has a tab where YAML expects spaces on line 12
- **THEN** its status is a syntax failure at line 12 and the other contract files are read

### Requirement: OpenAPI
Each operation under `paths` SHALL declare `http <METHOD> <base><path>` at the line of the operation, once for each distinct base in the order of the servers. A base SHALL be the path of an entry of `servers` in OpenAPI 3 or the `basePath` in Swagger 2, without its origin, whether the origin is written `https://host`, `//host` or `{variable}`; a document with neither has the empty base.

#### Scenario: Operations are declared under the server path
- **WHEN** `openapi.yaml` has `servers: [{url: "https://api.example.com/v1"}]` and the path `/bookings/{id}` with `get` and `delete`
- **THEN** it declares `http GET /v1/bookings/{}` and `http DELETE /v1/bookings/{}` at the lines of `get` and `delete`

#### Scenario: Operations are declared under every server path
- **WHEN** `api/openapi.yaml` has the servers `https://api.example.com/v1`, `https://staging.example.com/v1`, `//api.example.com/v2/` and `{baseUrl}/v3`, the path `/bookings` with `get`, and the path `/carts` that is a `$ref` to a fragment with `get`
- **THEN** it declares `http GET /v1/bookings`, `http GET /v2/bookings` and `http GET /v3/bookings` at the line of `get`, and the fragment declares `http GET /v1/carts`, `http GET /v2/carts` and `http GET /v3/carts`

### Requirement: OpenAPI references
A `$ref` to another file SHALL be an import resolving relative to the referring file with its fragment dropped, and a path item that is a `$ref` to a fragment SHALL declare the fragment's operations under the referring path, once for each base, at their lines in the fragment.

#### Scenario: References are imports
- **WHEN** `api/openapi.yaml` has `$ref: "./schemas/booking.yaml#/Booking"`
- **THEN** it has an import that resolves to `api/schemas/booking.yaml`

#### Scenario: Path items in fragments are declared
- **WHEN** `api/openapi.yaml` has `/bookings: {$ref: "./paths/bookings.yaml"}` and `api/paths/bookings.yaml` has `get` and `post` at its top level
- **THEN** `api/paths/bookings.yaml` declares `http GET /bookings` and `http POST /bookings`

### Requirement: OpenAPI documents that cannot be searched
A document that is too large, unreadable or invalid when the connector looks for the fragments a `$ref` reaches SHALL be skipped with a logged warning that names it, and the references of the other documents SHALL still be followed.

#### Scenario: A document that cannot be searched for references is logged
- **WHEN** `openapi.yaml` has the path items `/bookings` and `/carts` as `$ref`s to `paths/bookings.yaml` and `paths/carts.yaml`, and `paths/carts.yaml` has a tab where YAML expects spaces
- **THEN** a warning names `paths/carts.yaml` and none names `paths/bookings.yaml`, and both files belong to the project

### Requirement: GraphQL
In a schema, each field of `Query`, `Mutation` and `Subscription`, including their `extend type` forms and the types a `schema` definition names for those roots, SHALL declare `graphql <Root>.<field>`, with a renamed root written as `Query`, `Mutation` or `Subscription`.

#### Scenario: Schema fields are declared
- **WHEN** a schema has `schema { query: RootQuery }`, `type RootQuery { bookings: [Booking] }` and `extend type Mutation { cancel(id: ID!): Booking }`
- **THEN** it declares `graphql Query.bookings` and `graphql Mutation.cancel`

### Requirement: GraphQL operations
In an operation, each root field SHALL call `graphql <Root>.<field>`, an anonymous operation being a query, and the root fields of a fragment spread at the root SHALL count when the fragment is in the same document.

#### Scenario: Operations call their root fields
- **WHEN** a document has `query Bookings { bookings { id } me { id } }`, `mutation { cancel(id: 1) { id } }`, the shorthand `{ me }`, and `query WithFragment { ...F }` with `fragment F on Query { me }` in the same document
- **THEN** it calls `graphql Query.bookings`, `graphql Query.me` and `graphql Mutation.cancel`, and `graphql Query.me` again at the shorthand and at the field of the fragment

### Requirement: GraphQL imports
A `#import "<path>"` comment SHALL be an import relative to the file.

#### Scenario: Import comments are imports relative to the file
- **WHEN** `api/schema.graphql` has the comment `#import "./types/booking.graphql"` and `api/types/booking.graphql` exists
- **THEN** it has an import that resolves to `api/types/booking.graphql`

### Requirement: Embedded GraphQL
The connector SHALL read embedded `graphql` documents the same way as it reads `graphql` files and SHALL return lines within the document, line 1 being its first line; the engine places them in the host file.

#### Scenario: Embedded documents report lines within the document
- **WHEN** `parse_embedded` gets `query { bookings { id } }` embedded at line 5
- **THEN** it returns a call to `graphql Query.bookings` at line 1

### Requirement: Protobuf
Each `rpc` of a `service` SHALL declare `grpc <package>.<Service>/<Method>` at its line, and a `google.api.http` option on it SHALL declare `http <METHOD> <path>` for its binding and each additional binding, whether the option is written as one message or one field at a time (`option (google.api.http).get = "…"`). A path variable, `{id}` or `{name=projects/*}`, SHALL be a parameter wherever it stands, `{id}:cancel` and a variable that spans several segments included.

#### Scenario: Services and HTTP bindings are declared
- **WHEN** `booking.proto` has `package booking.v1;` and `service BookingService { rpc GetBooking(GetBookingRequest) returns (Booking) { option (google.api.http) = { get: "/v1/bookings/{id}" }; } }`
- **THEN** it declares `grpc booking.v1.BookingService/GetBooking` and `http GET /v1/bookings/{}`

#### Scenario: A path variable is a parameter wherever it stands
- **WHEN** an `rpc` has `option (google.api.http) = { post: "/v1/things/{id}:cancel" };` and another has `option (google.api.http) = { get: "/v1/{parent=projects/*/locations/*}/things" };`
- **THEN** they declare `http POST /v1/things/{}` and `http GET /v1/{}/things`

#### Scenario: A binding set one field at a time is declared
- **WHEN** an `rpc` has `option (google.api.http).get = "/v1/things/{id}";` and another has `option (google.api.http).delete = "/v1/things/{id}";` and `option (google.api.http).body = "*";`
- **THEN** they declare `http GET /v1/things/{}` and `http DELETE /v1/things/{}`

### Requirement: Protobuf imports
An `import` SHALL resolve against the module roots of `buf.yaml` and the directories of `buf.work.yaml` when they exist, and then against each directory above the importing file; `google/protobuf/` imports SHALL resolve to builtins and any other import found nowhere to an unknown package.

#### Scenario: Imports resolve from the proto root
- **WHEN** `proto/booking/v1/booking.proto` imports `booking/v1/types.proto`, `google/protobuf/timestamp.proto` and `google/api/annotations.proto`, and `proto/booking/v1/types.proto` exists
- **THEN** the imports resolve to `proto/booking/v1/types.proto`, a builtin and an unknown package

### Requirement: Prisma
Each `model` SHALL declare `table <model>` and, with `@@map("<name>")`, also `table <name>`. A datasource `url = env("<NAME>")` SHALL call `env <NAME>`. A brace inside a string literal SHALL close no block.

#### Scenario: A model declares its name and its table
- **WHEN** a schema has `model BookingItem { id Int @id @@map("booking_items") }` and `datasource db { url = env("DATABASE_URL") }`
- **THEN** it declares `table bookingitem` and `table booking_items` and calls `env DATABASE_URL`

#### Scenario: A brace inside a string does not end a model
- **WHEN** a model has `meta Json @default("{}")` on one line and `@@map("bookings")` on the next
- **THEN** the model declares `table bookings` and the schema is read without errors

### Requirement: SQL
`CREATE`, `ALTER` and `DROP` of a table or view, and `CREATE INDEX … ON`, SHALL declare `table <name>`; `FROM`, `JOIN`, `INSERT INTO`, `UPDATE`, `DELETE FROM` and `MERGE INTO` SHALL call `table <name>`, leaving out the names of common table expressions. Names SHALL drop their schema and quotes. `ALTER TABLE <name> RENAME TO <new>` SHALL declare the new name as well.

#### Scenario: A migration declares its tables
- **WHEN** a file has `CREATE TABLE IF NOT EXISTS public."BookingItem" (id int);` and `ALTER TABLE bookings ADD COLUMN note text;`
- **THEN** it declares `table bookingitem` and `table bookings`

#### Scenario: A rename declares the new name
- **WHEN** a file has `ALTER TABLE bookings RENAME TO reservations;`
- **THEN** it declares `table bookings` and `table reservations`

#### Scenario: Queries call their tables
- **WHEN** a file has `WITH recent AS (SELECT * FROM bookings) SELECT * FROM recent JOIN booking_items i ON true WHERE id = %s;`
- **THEN** it calls `table bookings` and `table booking_items` and not `table recent`

### Requirement: SQL dialects
Each statement SHALL be read with the dialect that the `sql` connector's `dialect` option names, PostgreSQL by default, and a dialect the parser does not know SHALL be an options error naming the option and the value.

#### Scenario: The dialect option picks the grammar
- **WHEN** the `dialect` option is `mysql` and a file has ``SELECT id FROM `bookings`;``
- **THEN** its status is ok and it calls `table bookings`

#### Scenario: An unknown dialect is an options error
- **WHEN** the `dialect` option is `cobol`
- **THEN** opening the `sql` connector fails with an options error that names `dialect` and `cobol`

### Requirement: SQL placeholders
Placeholders of the common drivers (`$1`, `?`, `:name`, `%s`, `%(name)s`) SHALL be read as parameters, and a statement that is read with its placeholders replaced SHALL report its tables at the lines they are written on.

#### Scenario: Driver placeholders are parameters
- **WHEN** a file has `SELECT * FROM bookings WHERE id = $1;` on line 1 and the same statement on lines 2, 3, 4 and 5 with `?`, `:id`, `%s` and `%(id)s` in place of `$1`, respectively
- **THEN** its status is not recovered and it calls `table bookings` at lines 1 to 5

#### Scenario: A statement across lines keeps the lines of its tables
- **WHEN** a file has a statement that reads `bookings` on line 2 and `booking_items` on line 3 and has `%s` on line 4, and another with `'it''s'` on line 6, `carts` on line 7 and `%(owner)s` on line 8
- **THEN** its status is not recovered and it calls `table bookings` at line 2, `table booking_items` at line 3 and `table carts` at line 7

### Requirement: SQL recovery
A statement that does not parse, and a statement with a string, quoted name or comment that is never closed, SHALL each be counted in a recovered status at the line it starts on. The statements before it SHALL be read, and so SHALL those after the next semicolon that follows the unclosed text.

#### Scenario: One bad statement does not hide the rest
- **WHEN** a file has three statements and the second is not valid SQL
- **THEN** its status is recovered with one error at the second statement's line and the sites of the first and third are read

#### Scenario: A statement that does not tokenize does not hide the rest
- **WHEN** a file has `SELECT * FROM a;`, `SELECT 'oops FROM b;` and `SELECT * FROM c;` on lines 1 to 3
- **THEN** its status is recovered with one error at line 2 and it calls `table a` and `table c`

### Requirement: Embedded SQL
The connector SHALL read embedded `sql` documents the same way as it reads `sql` files and SHALL return lines within the document, line 1 being its first line; the engine places them in the host file.

#### Scenario: Embedded SQL reports lines within the document
- **WHEN** `parse_embedded` gets `SELECT id FROM bookings` embedded at line 7
- **THEN** it returns a call to `table bookings` at line 1

### Requirement: Env files
Each `<NAME>=` line, with or without `export`, SHALL declare `env <NAME>` at its line; comments and blank lines SHALL be skipped.

#### Scenario: Example env files declare their variables
- **WHEN** `.env.example` has `# database`, `DATABASE_URL=postgres://localhost/app` and `export PORT=3000`
- **THEN** it declares `env DATABASE_URL` and `env PORT`

### Requirement: Provides
Each connector SHALL report what it provides, with its blind spots in words.

#### Scenario: Blind spots are reported
- **WHEN** the coverage of the contract connectors is read
- **THEN** among the blind spots are GraphQL fragments spread from other files, environment variables set in compose files, CI and deployment manifests, and schemas generated at build time
