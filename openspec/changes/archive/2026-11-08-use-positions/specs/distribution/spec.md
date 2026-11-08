# Spec Delta

## ADDED Requirements

### Requirement: The skill does not say how to lengthen an answer
The skill SHALL NOT tell the agent to pass `tests: true` to see the tests while working out an edit, to give a larger depth or a depth that runs the walk to the end, or to ask for every entry of a list. It SHALL say how to read the line of a file at distance 1 and the count of generated files.

#### Scenario: The skill holds no advice to lengthen an answer
- **WHEN** `plugin/skills/gsttt/SKILL.md` is read
- **THEN** it holds none of `tests: true`, `larger depth`, `4294967295` and `Ask for every entry`
- **AND** it names `generated=true` and the `> ` row under an entry at distance 1
