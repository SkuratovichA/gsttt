# Spec Delta

## ADDED Requirements

### Requirement: A failed link leaves its sites unread
When a project's `link` fails or panics, every place its files leave to the link, as the project's `unlinked` names them, SHALL count as an unread site of its file with the reason `link failed`, next to the coverage line of the failure. A link that succeeds SHALL add no such site.

#### Scenario: A failed link leaves what waits for it unread
- **WHEN** a project's `link` fails because the extra data of one file does not decode, and `unlinked` names a site at line 3 of another file, `a.txt`
- **THEN** coverage counts one unread site of `a.txt` at line 3 with the reason `link failed`
- **AND** the coverage line of the failed link stays
- **AND** the file whose extra data does not decode has no such site

#### Scenario: A panicking link leaves what waits for it unread
- **WHEN** a project's `link` panics and `unlinked` names a site at line 3 of `a.txt`
- **THEN** coverage counts one unread site of `a.txt` at line 3 with the reason `link failed`
- **AND** the coverage line of the failed link stays

#### Scenario: A link that succeeds reports nothing of the kind
- **WHEN** a project's `link` succeeds and `unlinked` names a site at line 3 of `a.txt`
- **THEN** coverage counts no site with the reason `link failed`
