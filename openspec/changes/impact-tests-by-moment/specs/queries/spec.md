# Spec Delta

## ADDED Requirements

### Requirement: A hint beside the tests left out
A request for an `impact` answer MAY carry a hint that says how to have the tests listed. The text SHALL follow the count of the test files left out with the hint in brackets, and the JSON SHALL hold the count and no hint. Without a hint, and when the tests are listed, the text SHALL add nothing.

#### Scenario: A hint says how to list the tests left out
- **WHEN** `a.test.ts` imports `a.ts`, and `impact` is asked about `a.ts` to leave the tests out with the hint `ask with tests=true`
- **THEN** the text says `1 test file left out (ask with tests=true)`, the JSON holds `tests_left_out` 1 and no hint, the same request without the hint says `1 test file left out`, and with the tests listed the text has no hint
