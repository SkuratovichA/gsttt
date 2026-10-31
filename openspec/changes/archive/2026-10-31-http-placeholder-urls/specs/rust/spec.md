# Spec Delta

## MODIFIED Requirements

### Requirement: Request URLs
With the `reqwest` pack, a URL from `format!` SHALL keep its literal parts with each placeholder as a parameter; a URL with no literal path, or whose path is computed in every segment, SHALL be an unread site.

#### Scenario: Built URLs keep their literal parts and bare URLs are unread
- **WHEN** a file has `let client = reqwest::Client::new();`, `client.delete(format!("/bookings/{}", id))` on line 12 and `client.get(url)` on line 13
- **THEN** it calls `http DELETE /bookings/{}` at line 12
- **AND** the file has an unread site at line 13

#### Scenario: A URL whose path is only computed segments is a blind spot
- **WHEN** a file has `client.get(format!("{}/{}", base, id))` and `client.get(format!("{}/users/{}", base, id))`
- **THEN** the first is an unread site at its line
- **AND** the second calls `http GET /users/{}`
