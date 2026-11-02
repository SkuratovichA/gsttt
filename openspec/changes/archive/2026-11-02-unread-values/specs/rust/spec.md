# Spec Delta

## ADDED Requirements

### Requirement: Actix resources that are not text
With the `actix` pack, a `web::resource` whose path is not text, such as a constant or a variable, SHALL serve nothing and SHALL be an unread site at the line of the call, since the verb routes on it would be served at a path that is not its own.

#### Scenario: An actix resource whose path is not text serves nothing
- **WHEN** `src/main.rs` has `const BOOKINGS: &str = "/bookings";`, `web::resource(BOOKINGS).route(web::get().to(list))` on line 6 and `web::resource("/rooms").route(web::get().to(rooms))` on line 7
- **THEN** `src/main.rs` serves `http GET /rooms` at line 7 and no other path
- **AND** the file has an unread site at line 6
