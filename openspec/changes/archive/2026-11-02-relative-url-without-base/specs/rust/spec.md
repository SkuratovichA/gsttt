# Spec Delta

## ADDED Requirements

### Requirement: Relative requests
With the `reqwest` pack, a request whose URL is relative and has no base SHALL be an unread site, since it fails at run time: `reqwest::get("users")`, and `get("users")` on a receiver bound to `Client::new()`. An absolute URL SHALL be read as before.

#### Scenario: A relative request with no base is unread
- **WHEN** a file has `reqwest::get("users")` on line 2 and, on `let client = reqwest::Client::new();`, `client.get("users")` on line 7
- **THEN** each is an unread site at its line

#### Scenario: An absolute request is unchanged by relative requests
- **WHEN** a file has `reqwest::get("https://a.test/x")` on line 11
- **THEN** it calls `http GET /x` at line 11
