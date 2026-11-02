# Spec Delta

## ADDED Requirements

### Requirement: Changed declarations are walked
`gsttt impact --changed` SHALL walk the declarations of a modified TypeScript file whose text differs from its old blob, not the file whole, and its header SHALL name them. The `--json` answer SHALL list them in `parts`.

#### Scenario: --changed walks the declarations a patch changes
- **WHEN** one exported function of `src/format.ts` is edited, and two files import it and two import another export
- **THEN** `gsttt impact --changed` lists only the importers of the edited function, and starts with `impact of formatUser in src/format.ts, within 1 hop through code`

### Requirement: Private helpers are followed
A declaration that uses a changed declaration of the same file SHALL be walked too, so a changed private helper reaches the importers of the declarations that use it.

#### Scenario: --changed follows a changed private helper to the declarations that use it
- **WHEN** a private `normalize` is edited and only `formatUser` uses it
- **THEN** the `parts` names include `normalize` and `formatUser`, and only the importers of `formatUser` are affected

### Requirement: Unchanged declarations are not traced
A modified file in which no declaration differs, only comments, blank lines or indentation, SHALL NOT be walked, and the answer SHALL say `<file> changes no declaration, so it is not traced`.

#### Scenario: --changed does not trace a file where no declaration changed
- **WHEN** `src/format.ts` gains a comment and is reindented
- **THEN** nothing is affected and the text says `src/format.ts changes no declaration, so it is not traced`

### Requirement: Changed imports are read
An import that gains, loses or re-points a name SHALL change the declarations that use that name, whether the import sits on one line or on several.

#### Scenario: --changed reads a multi-line import that gains a name only one declaration uses
- **WHEN** a multi-line import in `src/view.ts` gains `shout` and only `show` uses it
- **THEN** only the importers of `show` are affected

### Requirement: Re-pointed imports are followed
An import re-pointed to another module SHALL change the declarations that use its names, and no other declaration of the file.

#### Scenario: --changed follows a re-pointed import to the declarations that use it
- **WHEN** `src/user.ts` imports `clock` from `./b` instead of `./a` and only `now` uses `clock`
- **THEN** only the importers of `now` are affected

### Requirement: Removed declarations walk the file whole
A file in which a declaration is removed or renamed SHALL be walked whole, as the importers of the old name are not known from the new text.

#### Scenario: --changed walks a file whole when a declaration is removed or renamed
- **WHEN** `formatId` is removed from `src/format.ts`, and in another run renamed
- **THEN** every importer of the file is affected

### Requirement: New declarations have no importer
A declaration that is new SHALL be walked on its own, and no file that imports the file SHALL be listed for it.

#### Scenario: --changed lists no importer of a new declaration nobody imports
- **WHEN** `src/format.ts` gains an exported `formatDate`
- **THEN** nothing is affected and `parts` names `formatDate`

### Requirement: Statements outside declarations walk the file whole
A file in which a side-effect import or a top-level statement outside a declaration changes SHALL be walked whole.

#### Scenario: --changed walks a file whole when a side-effect import or a top-level statement changes
- **WHEN** a side-effect import is re-pointed, and in another run a top-level `console.log` is added
- **THEN** every importer of the file is affected and the answer has no `parts`

### Requirement: Languages without declarations walk the file whole
A changed file of a language that records no declarations, and a file that is new, SHALL be walked whole, and the header SHALL name it as before.

#### Scenario: --changed walks a changed file whole when its language records no declarations
- **WHEN** `tools/job.py` is edited and `src/fresh.ts` is new
- **THEN** the header is `impact of src/fresh.ts and tools/job.py, within 1 hop through code` and the answer has no `parts`

### Requirement: Declarations are compared with the merge base
With `--base <rev>` the old text of a file SHALL be its blob at the best common ancestor of `HEAD` and `<rev>`, and without it the blob of `HEAD`.

#### Scenario: --changed with --base compares with the merge base
- **WHEN** a commit on a topic branch edits `formatUser` and the working tree edits `formatId`
- **THEN** `--changed` names `formatId`, and `--changed --base main` names both

### Requirement: A changed method reaches its callers
A changed method SHALL be named with its class, `Service.place`, and the walk SHALL reach the declarations that call that method and not those that call another method of the class.

#### Scenario: --changed follows a changed method to its callers and not to the callers of another
- **WHEN** `Service.place` is edited, `src/places.ts` calls `Service.place` and `src/makes.ts` calls `Service.make`
- **THEN** `parts` names `Service.place` and only `src/places.ts` is affected

### Requirement: A file that does not parse walks whole
A file whose new version does not parse SHALL be walked whole.

#### Scenario: --changed walks a file whole when its new version does not parse
- **WHEN** the signature of `formatUser` is broken so the file does not parse
- **THEN** every importer of the file is affected and the answer has no `parts`

### Requirement: A moved declaration walks the file whole
A file in which a declaration moves relative to the other declarations and statements SHALL be walked whole, as moving it changes when its code runs.

#### Scenario: --changed walks a file whole when a declaration moves
- **WHEN** `formatId` is moved above `formatUser` in `src/format.ts`
- **THEN** every importer of the file is affected and the answer has no `parts`

### Requirement: Whitespace in a template is a change
Whitespace inside a template literal SHALL count as a change of the declaration that holds it, as it is part of the value, while indentation of other code SHALL NOT.

#### Scenario: --changed reads the whitespace inside a template as a change
- **WHEN** a line inside a multi-line template of `render` is indented further, and in another run a line of other code is
- **THEN** the importers of `render` are affected in the first run, and in the second `src/sql.ts` changes no declaration

