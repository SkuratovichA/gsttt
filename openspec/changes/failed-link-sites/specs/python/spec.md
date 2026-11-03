# Spec Delta

## ADDED Requirements

### Requirement: Routes on a router wait for the link
The connector SHALL name each route declared on a router as a place that only the link turns into a fact, with its line and the decorator as written.

#### Scenario: A route on a router is named as waiting for the link
- **WHEN** a file declares `@router.get("/{booking_id}")` on an `APIRouter` at line 6 and another file only mounts the router
- **THEN** the first file names line 6 with `router.get`
- **AND** the second names nothing
