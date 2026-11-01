## MODIFIED Requirements

### Requirement: Contract keys
Every HTTP contract key SHALL pass through one normaliser, whichever connector made it: the method in upper case or `*`, and the path without scheme, host, query, fragment and repeated or trailing slashes, with each parameter segment as `{}` and a placeholder before the first `/` dropped as the base URL, except a literal glued after it that cannot be part of a host (`api` in `${base}api/x`), which starts the path.

#### Scenario: Paths from every framework meet
- **WHEN** keys are made from `/bookings/:id`, `/bookings/{id:int}`, `/bookings/<int:id>`, `${API_URL}/bookings/${booking.id}?expand=1` and `https://api.example.com/bookings/{}#top`
- **THEN** each has the path `/bookings/{}`

#### Scenario: A literal glued to a base URL starts the path
- **WHEN** keys are made from `${catalogServiceUrl}api/versions/${id}`, `${HOST}:8080/bookings`, `${SUB}.example.com/bookings` and `${base}${period}/bookings`
- **THEN** the first has the path `/api/versions/{}` and each of the others the path `/bookings`
