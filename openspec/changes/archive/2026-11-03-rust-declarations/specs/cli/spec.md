# Spec Delta

## REMOVED Requirements

### Requirement: Languages without declarations walk the file whole
**Reason**: Rust records declarations now, so the requirement no longer names a language; a file that is new and a file read past syntax errors remain whole.
**Migration**: `A new file walks whole` holds the behaviour that stays.

## ADDED Requirements

### Requirement: A new file walks whole
A changed file that is new, and a changed file the parser could not read, SHALL be walked whole, and the header SHALL name it as before.

#### Scenario: --changed walks a new file whole
- **WHEN** `tools/src/lib.rs` is edited and `src/fresh.ts` is new
- **THEN** the header is `impact of run in tools/src/lib.rs, and of src/fresh.ts, within 1 hop through code` and `parts` names only `run` in `tools/src/lib.rs`

### Requirement: A changed Rust function reaches its callers
A changed function, struct, enum or method of a Rust file SHALL be named, a method with its type as `Type.method`, and the walk SHALL reach the declarations that use that name and not those that use another name of the file.

#### Scenario: --changed follows a changed Rust function to its callers and not to the callers of another
- **WHEN** `tools/src/job.rs` declares `run` and `stop`, `tools/src/main.rs` calls `run` and `tools/src/halt.rs` calls `stop`, and `run` is edited
- **THEN** `parts` names `run` in `tools/src/job.rs` and only `tools/src/main.rs` is affected
