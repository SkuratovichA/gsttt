## MODIFIED Requirements

### Requirement: HTTP matching
Two HTTP keys SHALL meet when their methods are equal or either is `*`, and their paths have the same number of segments, each pair equal or `{}` on either side, with or without one prefix from the index's `http_prefixes` removed from the start of one of the two paths. A prefix SHALL NOT be removed from both paths, and SHALL NOT be removed from a path that it is all of. When both paths hold a literal segment, at least one pair SHALL be the same literal on both sides.

#### Scenario: A prefix is stripped before matching
- **WHEN** `http_prefixes` is `["/api"]` and `http GET /api/bookings/me` is compared with `http GET /bookings/{}`
- **THEN** they meet

#### Scenario: A prefix is removed from one path only
- **WHEN** `http_prefixes` is `["/api", "/v1"]` and `http GET /api/users` is compared with `http GET /v1/users`
- **THEN** they do not meet

#### Scenario: A prefix alone is no path
- **WHEN** `http_prefixes` is `["/api"]` and `http GET /api` is compared with `http GET /`
- **THEN** they do not meet

#### Scenario: Methods must agree
- **WHEN** `http POST /bookings` is compared with `http GET /bookings`, and `http * /bookings` with each of them
- **THEN** the first pair does not meet and `http * /bookings` meets both

#### Scenario: Segment counts must agree
- **WHEN** `http GET /bookings/{}` is compared with `http GET /bookings/{}/items`
- **THEN** they do not meet

#### Scenario: Meeting is symmetric
- **WHEN** any two HTTP keys and any prefixes are compared in both orders
- **THEN** both comparisons give the same result

#### Scenario: Literals that meet only parameters make no match
- **WHEN** `http POST /{}/messages` is compared with `http POST /u/{}`
- **THEN** they do not meet

#### Scenario: A path of parameters alone meets a literal path
- **WHEN** `http GET /{}` is compared with `http GET /users`, and `http GET /{}/{}` with `http GET /a/b`
- **THEN** both pairs meet
