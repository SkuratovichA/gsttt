# Spec Delta

## ADDED Requirements

### Requirement: A use carries its line
Every name the connector records as used SHALL carry the line of the node that makes the use, in the uses of a declaration, in those of the code that runs as the file loads and in the members read on a value of unknown class. A call chain broken over lines SHALL take the line of the member, and a name used on several lines SHALL keep the first.

#### Scenario: A use keeps the line it is written on
- **WHEN** a typescript file declares a function that uses two names on two lines, and a chained call whose member stands on a line after its receiver
- **THEN** each name is recorded with the line it is written on, and the chained member with the line of the member
