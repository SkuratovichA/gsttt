## ADDED Requirements

### Requirement: A type reads no member
A type that names a class SHALL read no member of it. Asked about a member, a file that names the class only in a type, as `import('x').Y` does, SHALL be a holder of the class, and its declarations SHALL be callers only where the file reads that member.

#### Scenario: A type that names a class reads no member of it
- **WHEN** two files name `Runner` in a type and only one of them reads `start` on a value
- **THEN** asked about `Runner.start`, only that one is a caller
- **AND** asked about `Runner`, both are
