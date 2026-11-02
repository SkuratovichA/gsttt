# Spec Delta

## ADDED Requirements

### Requirement: A file nested deeper than a stack is not the end of the run
A file nested deeper than the stack of a thread SHALL be read, or reported as one failed file, and SHALL never end the run. A connector call that reads the text of a file runs on a stack that grows with the length of that text, and a deep value in a fact position SHALL be kept whole through the link.

#### Scenario: A file nested deeper than a stack does not end the run
- **WHEN** a repository holds a normal file and, for TypeScript, Python and Rust, one file nested 5000 levels, closed and not closed, and `index` runs twice, the second after an edit
- **THEN** both runs exit with 0, the second reads the deep files back from the snapshot, and the normal file keeps its facts
- **AND** `impact` of the normal file answers

#### Scenario: A parse that needs more stack than a thread has still ends with the facts of its file
- **WHEN** a connector whose parse recurses once per byte, in a frame of 2 KiB, reads a file of 100000 bytes
- **THEN** the run ends and coverage counts the file as read, with its facts

#### Scenario: A deep value in a fact position survives the link
- **WHEN** `use.ts` calls `fetch` with a sum of 5000 pieces of text, once starting with a name that `i.ts` declares and once with text only
- **THEN** the call of the sum with the name is a linked fact with the whole path and the other is a fact of the file with the whole path
- **AND** the run has no problem line
