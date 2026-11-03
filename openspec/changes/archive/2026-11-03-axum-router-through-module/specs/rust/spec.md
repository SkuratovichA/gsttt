# Spec Delta

## ADDED Requirements

### Requirement: A router imported through its module
With the `axum` pack, a `Router` that a `use` brings in from `axum::routing` SHALL be the same type as one from `axum`, because axum exports it from both. A `Router` that a `use` brings in from another crate SHALL be no router of the pack, whatever its module is called.

#### Scenario: A router imported through its module serves its routes
- **WHEN** a file has `use axum::routing::{get, Router};` and `router.route("/nested", get(show))`, and `router` is a parameter of type `Router`
- **THEN** it serves `http GET /nested`

#### Scenario: A router of another crate serves nothing
- **WHEN** a file has `use axum::routing::get;`, `use other_web::routing::Router;` and `router.route("/other_module", get(show))`, and `router` is a parameter of type `Router`
- **THEN** the file serves nothing
