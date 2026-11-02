# Spec Delta

## ADDED Requirements

### Requirement: A tree nested past the limit is one failed file
A file whose syntax tree would nest deeper than 2000 levels SHALL be one failed file, with a syntax error at the line where the nesting passes the limit that says `nested deeper than 2000 levels`, and it SHALL read nothing. The other files of the project SHALL be read as they would without it.

#### Scenario: A file nested past the limit is one failed file
- **WHEN** a crate has four files nested 5000 levels, by parentheses closed and not closed, by a sum and by prefix operators, and `lib.rs` uses a constant of one of them
- **THEN** each of the four fails with `nested deeper than 2000 levels`
- **AND** `lib.rs` is read, with its import
