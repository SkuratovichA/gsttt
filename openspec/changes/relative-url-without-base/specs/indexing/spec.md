# Spec Delta

## ADDED Requirements

### Requirement: Relative requests
A request whose target is relative and has no base URL SHALL follow the `relative_without_base` of its pack: `rooted` calls the path from the root, `unread` makes the site an unread site, and a rule with no receiver in a pack that sets none is no request. A client whose base is there and cannot be read SHALL call the path. A target with a `..` segment SHALL be unread under either setting, and one that names no path SHALL be no request.

#### Scenario: A relative request with no base follows the pack
- **WHEN** a request rule is applied to the target `items` with no base
- **THEN** a rule with no receiver calls `http GET /items` under `rooted`, is an unread site under `unread` and is no request when the pack sets nothing
- **AND** a rule with a receiver whose base is absent or empty calls `http GET /items` under `rooted` and when the pack sets nothing, and is an unread site under `unread`

#### Scenario: A client whose base cannot be read calls the path
- **WHEN** a rule with a receiver is applied to `items` and the receiver has a base that is not text
- **THEN** it calls `http GET /items` under every setting of the pack
- **AND** with the base `/api` it calls `http GET /api/items`

#### Scenario: A relative request that climbs out of its directory is unread
- **WHEN** a request rule with no base is applied to `../items` or to `a/../items?x=1`
- **THEN** it is an unread site under `rooted` and under `unread`, and no request when the pack sets nothing and the rule has no receiver
- **AND** `items?back=../x` calls `http GET /items` under `rooted`

#### Scenario: A request target that names no path is no request
- **WHEN** a request rule with no base is applied to an empty target, `?x=1`, `#top`, `data:text/plain,x`, `mailto:a@b.c` or `localhost:3000/x`
- **THEN** it is no request under every setting, with a receiver or without one
