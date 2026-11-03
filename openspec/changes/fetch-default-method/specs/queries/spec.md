## MODIFIED Requirements

### Requirement: Contract lookup
`contract` SHALL take a canonical key and list its sites grouped by role, and for an HTTP key also the sites of every key that meets it, each with its own key. Text that is not a canonical key SHALL be searched case-insensitively in every key, and the answer SHALL list the matching keys with the number of sites of each role. A blank query SHALL be an error that says what to ask for, not an answer.

#### Scenario: A route lists its callers
- **WHEN** `http GET /bookings/{}` is looked up, one file serves that key and another calls `http GET /bookings/me`
- **THEN** the answer lists the server under serves and the caller under calls with the key `http GET /bookings/me`

#### Scenario: Free text searches keys
- **WHEN** `booking` is looked up and the index has `table bookings`, `graphql Query.bookings` and `env API_URL`
- **THEN** the answer lists `table bookings` and `graphql Query.bookings` with their site counts and not `env API_URL`

#### Scenario: A blank query is an error
- **WHEN** `contract` is asked for empty text or only whitespace, and the index holds contracts
- **THEN** it returns an error that says to give a contract key or part of one
- **AND** it does not list the contracts of the index

#### Scenario: Asking about a GET finds a fetch that writes no method
- **WHEN** `GET /x` is looked up, and a TypeScript file has `fetch('/x')`
- **THEN** the answer lists `http GET /x` with one call
- **AND** looking up `POST /x` lists no key
