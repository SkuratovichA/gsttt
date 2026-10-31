# Spec Delta

## ADDED Requirements

### Requirement: Computed route paths
A NestJS route decorator whose path argument is not a string literal or template SHALL be a blind spot at the line of the decorator, not a route at the controller's prefix; a route decorator with no argument SHALL still serve the prefix alone.

#### Scenario: A route whose path is a constant is a blind spot
- **WHEN** a class decorated `@Controller('/')` has a method decorated `@Get(LOGIN_PATH)`, where `LOGIN_PATH` is an imported constant, and another method decorated `@Get()`
- **THEN** the file has an unread site at the line of `@Get(LOGIN_PATH)` and serves `http GET /` only at the line of `@Get()`
