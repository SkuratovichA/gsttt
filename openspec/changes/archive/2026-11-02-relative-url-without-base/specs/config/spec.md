# Spec Delta

## ADDED Requirements

### Requirement: Relative requests of a pack
A pack MAY set `relative_without_base` to `rooted` or `unread`, which is what a request of the pack means when its target is relative and no base URL goes before it. A pack that omits it SHALL serialise as before, and any other value SHALL fail with the file, the line and the key. The built-in `axios` and `fetch` packs SHALL set `rooted`, and `requests`, `httpx` and `reqwest` SHALL set `unread`.

#### Scenario: A pack sets what a relative request means
- **WHEN** `.gsttt/packs/ktor.toml` sets `relative_without_base = "rooted"`, and another pack file sets `"unread"` or sets nothing
- **THEN** the packs of the setup hold `rooted`, `unread` and no setting

#### Scenario: A relative setting that is neither value is refused
- **WHEN** line 5 of `.gsttt/packs/ktor.toml` sets `relative_without_base = "skipped"`
- **THEN** evaluation fails with a message that starts with `.gsttt/packs/ktor.toml:5:` and names `relative_without_base` and `skipped`
