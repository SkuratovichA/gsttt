# Spec Delta

## ADDED Requirements

### Requirement: Relative requests
With the `requests` and `httpx` packs, a request whose URL is relative and has no base URL SHALL be an unread site, since it fails at run time: `requests.get("users")`, `requests.Session().get("users/me")` and `httpx.Client().get("items")`. A client whose base URL is there and is not text SHALL call the path it names. An absolute URL SHALL be read as before.

#### Scenario: A relative request with no base URL is unread
- **WHEN** a file has `requests.get("users")` on line 8, `requests.Session().get("users/me")` on line 12 and `httpx.Client().get("items")` on line 16
- **THEN** each is an unread site at its line

#### Scenario: A client with an unread base URL calls the relative path
- **WHEN** a file has `BASE = settings.API` and `httpx.Client(base_url=BASE).get("items")`
- **THEN** it calls `http GET /items`

#### Scenario: An absolute request is unchanged by relative requests
- **WHEN** a file has `requests.get("https://a.test/x")`
- **THEN** it calls `http GET /x`
