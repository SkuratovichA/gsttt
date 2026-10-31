# Spec Delta

## MODIFIED Requirements

### Requirement: HTTP requests
With the `fetch` pack, `fetch` SHALL call `http <method> <url>`, the method taken from the options' `method` and `*` when absent. With the `axios` pack, `axios.get`, `axios.post` and the other verbs, and the same verbs on a receiver made by `axios.create()`, SHALL call the URL with that verb. A URL built from a template SHALL keep its literal parts with each computed part as a parameter; a URL with no literal path, or whose path is computed in every segment, SHALL be an unread site.

#### Scenario: Requests become calls
- **WHEN** a file has ``fetch(`/bookings/${id}`, { method: 'DELETE' })`` and `axios.post('/bookings', body)`
- **THEN** it calls `http DELETE /bookings/{}` and `http POST /bookings`

#### Scenario: A base URL in a template is dropped
- **WHEN** a file has ``fetch(`${API_URL}/bookings`)``
- **THEN** it calls `http * /bookings`

#### Scenario: A computed URL is a blind spot
- **WHEN** a file has `fetch(url)`
- **THEN** the file has an unread site at that line with the reason computed at run time

#### Scenario: A URL whose path is only computed segments is a blind spot
- **WHEN** a file has ``fetch(`${API_URL}/${id}`)`` and ``fetch(`${base}${period}/${name}`)``
- **THEN** each is an unread site at its line and the file calls nothing for them
- **AND** ``fetch(`${API_URL}/users/${id}`)`` still calls `http * /users/{}`
