# Spec Delta

## MODIFIED Requirements

### Requirement: HTTP requests
With the `requests` and `httpx` packs, `requests.get` and the other verbs, `requests.request(method, url)`, the same verbs on `httpx` and on receivers made by `requests.Session()`, `httpx.Client()` or `httpx.AsyncClient()`, held in a variable or called where they are made, SHALL call the URL with the verb. An f-string URL SHALL keep its literal parts with each computed part as a parameter, and a URL with no literal path, or whose path is computed in every segment, SHALL be an unread site, also when its receiver was made with no base URL.

#### Scenario: Requests become calls
- **WHEN** a file has `requests.post(f"{BASE}/bookings/{id}/cancel")` and `httpx.get(url)`
- **THEN** it calls `http POST /bookings/{}/cancel` and has an unread site at the line of `httpx.get`

#### Scenario: A client with no base URL leaves a computed URL unread
- **WHEN** a file has `session = requests.Session()` and `session.get(f"{base}{path}")`, `httpx.Client().get(f"{url}")`, and `client = httpx.Client(base_url="https://api.example.com/v1")` with `client.get("/bookings")`
- **THEN** it has an unread site at the line of each of the first two calls
- **AND** it calls `http GET /v1/bookings`

#### Scenario: A URL whose path is only computed segments is a blind spot
- **WHEN** a file has `requests.get(f"{base}/{id}")` and `requests.get(f"{base}/users/{id}")`
- **THEN** the first is an unread site at its line
- **AND** the second calls `http GET /users/{}`
