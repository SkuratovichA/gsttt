# Spec Delta

## ADDED Requirements

### Requirement: Sites that wait for the link are named
The connector SHALL name, for each file, the places that only the link turns into facts: a site left to the link, a route on an Express host, a route of a class without the decorator, and a tRPC procedure listed in a router. A route that a controller declares SHALL not be named, because its fact stands without the link.

#### Scenario: A site left to the link is named as waiting for it
- **WHEN** a file calls `fetch(ROUTES.users)` on a constant imported from another file, another file registers `app.get('/health')` on an Express app, and a NestJS controller declares `@Get('all')`
- **THEN** the first file names the call at its line, and the second names the route at its line
- **AND** the controller names nothing
