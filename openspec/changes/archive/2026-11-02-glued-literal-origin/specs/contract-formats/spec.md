## MODIFIED Requirements

### Requirement: The origin of a URL
A URL SHALL have an origin when its text starts with a scheme and `://`, and the host after it SHALL end at the first `/`, `?` or `#`. A scheme SHALL be a letter followed by letters, digits, `+`, `-` or `.`, or be written as placeholders, which lets it start with `$` or `{` and hold `$`, `{` and `}`; it SHALL never hold `/`. Placeholders alone, `//` and a host that holds a placeholder SHALL be an origin too. An HTTP key SHALL drop the origin and keep the path after it.

#### Scenario: A scheme and host written as placeholders are an origin
- **WHEN** the HTTP paths `${}://${}/bookings/{}`, `{scheme}://{host}:{port}/v1` and `${}//${}/api/x` are normalised, the last from `${location.protocol}//${location.host}/api/x`
- **THEN** they become `/bookings/{}`, `/v1` and `/api/x`, and `${}//api/x`, a base URL with a trailing slash, becomes `/api/x`

#### Scenario: A URL in a query string is no origin
- **WHEN** the HTTP path `/a?next=https://b/c` is normalised
- **THEN** it becomes `/a`

#### Scenario: The host ends at a query string
- **WHEN** the HTTP paths `http://h?next=/a` and `http://h#/a` are normalised
- **THEN** both become `/`

#### Scenario: A literal glued to a placeholder before `//` is no origin
- **WHEN** the HTTP paths `${base}api//${id}` and `${x}0//{}` are normalised
- **THEN** they become `/api/{}` and `/0/{}`
