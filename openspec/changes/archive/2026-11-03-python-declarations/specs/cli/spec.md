# Spec Delta

## MODIFIED Requirements

### Requirement: Languages without declarations walk the file whole
A changed file of a language that records no declarations, which is Rust yet, and a file that is new, SHALL be walked whole, and the header SHALL name it as before.

#### Scenario: --changed walks a changed file whole when its language records no declarations
- **WHEN** `tools/src/lib.rs` is edited and `src/fresh.ts` is new
- **THEN** the header is `impact of src/fresh.ts and tools/src/lib.rs, within 1 hop through code` and the answer has no `parts`

## ADDED Requirements

### Requirement: A changed Python function reaches its callers
A changed function, class or method of a Python file SHALL be named, a method with its class as `Class.method`, and the walk SHALL reach the declarations that use that name and not those that use another name of the file.

#### Scenario: --changed follows a changed Python function to its callers and not to the callers of another
- **WHEN** `tools/job.py` declares `run` and `stop`, `tools/main.py` calls `run` and `tools/halt.py` calls `stop`, and `run` is edited
- **THEN** `parts` names `run` in `tools/job.py` and only `tools/main.py` is affected
