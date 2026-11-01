# Spec Delta

## ADDED Requirements

### Requirement: Unread sites the link finds
A connector's link SHALL be able to return the sites it could not read beside the facts it found, and each such site SHALL count in coverage as an unread site of its file, as a site the parse could not read does.

#### Scenario: An unread site from the link is counted
- **WHEN** the link of a connector returns an unread site at line 3 of `a.txt`
- **THEN** coverage counts it as an unread site of that connector
- **AND** it lists the site for `a.txt` at line 3
