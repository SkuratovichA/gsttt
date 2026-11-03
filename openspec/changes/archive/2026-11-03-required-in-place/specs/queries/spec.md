# Spec Delta

## ADDED Requirements

### Requirement: Impact through a package required in place
Impact SHALL reach the caller of a route that a file serves through a router made by a package required in place and mounted with a module required in place.

#### Scenario: A router required in place reaches a fetch of its route
- **WHEN** `a.js` has `require('express').Router()` with `router.get('/a', h)`, `b.js` mounts `require('./a')` at `/api`, and a TypeScript file calls `fetch('/api/a')`
- **THEN** impact of `a.js` lists the TypeScript file
