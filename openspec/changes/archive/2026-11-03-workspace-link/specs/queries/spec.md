## ADDED Requirements

### Requirement: Impact through a router of another package
A change to a router in one package SHALL reach the files that call the paths the router serves under the mounts of another package, because the router serves each route under the path an application mounts it at.

#### Scenario: A router of another package reaches the client of the mounted path
- **WHEN** an app of one package mounts a router of a second package at `/ext`, a client of a third package calls `/ext/x`, and the router file is the changed path
- **THEN** the affected files are the app and the client, and the client's reason is a call of `http GET /ext/x` through a contract
