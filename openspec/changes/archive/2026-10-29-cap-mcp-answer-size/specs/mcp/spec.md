# Spec Delta

## MODIFIED Requirements

### Requirement: Answers

Every tool except `index_status` SHALL refresh the index through the source and return one text block holding the answer's text as the CLI prints it, with lists cut unless `all` is true. A text over 25,000 characters SHALL show the most entries of each list that fit, after a first line that starts with `cut:`; the problems of a coverage answer are never cut. `index_status` SHALL read the stored index without refreshing it, and SHALL answer in plain text when there is no index yet.

#### Scenario: A tool answers with the CLI's text
- **WHEN** `dependents` is called for a file that two files import
- **THEN** the result is one text block equal to the answer's text, and the source was refreshed once

#### Scenario: all lists every entry
- **WHEN** `dependents` is called with `all` set to true for a file that 45 files import
- **THEN** the text lists all 45 files

#### Scenario: index_status does not refresh
- **WHEN** `index_status` is called
- **THEN** the source was asked for the stored index and was not refreshed

#### Scenario: index_status without an index answers in text
- **WHEN** `index_status` is called and the source has no stored index
- **THEN** the result is not an error and says there is no index yet and that the first question builds one

#### Scenario: An answer over the budget is cut to fit
- **WHEN** `dependents` and `impact` are called with `all` set to true about a file that 600 files with long paths import
- **THEN** each text holds at most 25,000 characters, starts with the `cut:` line, and lists more than 30 importers with a count of the rest
- **AND** for `dependents` one importer more would take the text over 25,000 characters

#### Scenario: A cut list is cut further when it does not fit
- **WHEN** `dependents` is called without `all` about a file whose 30 first importers take more than 25,000 characters
- **THEN** the text holds at most 25,000 characters, starts with the `cut:` line, and lists fewer than 30 importers
