# Spec Delta

## Purpose

Indexing turns a working tree into an index of files, imports and contract sites: it lists the files, hands each to one connector project, takes facts from snapshots or parses them, resolves and links them, and stores the result, deterministically and without letting one connector's failure stop the run.

## ADDED Requirements

### Requirement: Workspace location
The engine SHALL find the work tree root, the git dir and the common dir from any directory inside a work tree, and SHALL keep its state in `<git dir>/gsttt`. Outside git the root SHALL be the start directory and the state dir SHALL be `$XDG_CACHE_HOME/gsttt/<hash of the root>`, with `~/.cache` when the variable is unset. A bare repository SHALL be an error.

#### Scenario: Linked worktree has its own state dir
- **WHEN** the engine locates from a subdirectory of a linked worktree named `feature`
- **THEN** the root is the worktree's top directory
- **AND** the state dir is `<common dir>/worktrees/feature/gsttt`

#### Scenario: Directory outside git uses the cache home
- **WHEN** the engine locates from a directory with no enclosing repository and `XDG_CACHE_HOME` set to `/c`
- **THEN** the root is that directory
- **AND** the state dir is `/c/gsttt/<hex hash of the root path>`

### Requirement: File set
The engine SHALL index the tracked files present in the work tree, conflicted ones included, and the untracked files git does not ignore, SHALL leave out submodules, symbolic links, skip-worktree entries and files matching `Setup.exclude`, and SHALL number files in path order.

#### Scenario: Ignored, excluded and linked files stay out
- **WHEN** a repository has a committed `src/a.ts`, an untracked `src/new.ts`, `dist/a.js` ignored by `.gitignore`, `vendor/b.ts` matching `exclude = ["vendor/**"]` and a symbolic link `link.ts`
- **THEN** the index holds `src/a.ts` and `src/new.ts` and nothing else

#### Scenario: A deleted tracked file is gone
- **WHEN** a committed file is removed from the work tree without a commit
- **THEN** the index does not hold it

#### Scenario: A conflicted file stays in the set
- **WHEN** a merge stops on a conflict in a tracked file
- **THEN** the index holds that file with the id of the work tree file, conflict markers included

### Requirement: Content ids
The engine SHALL identify each file's content by its git blob id, SHALL take the id of a clean tracked file from the git index without reading the file, and SHALL hash a modified or untracked file. Outside git a file whose size and modification time match the snapshot SHALL keep its stored id.

#### Scenario: An edited file gets the id git would give it
- **WHEN** a committed file is edited and the engine refreshes
- **THEN** the file's content id equals what `git hash-object` prints for it

#### Scenario: A clean tree is not parsed again
- **WHEN** a committed, clean repository is indexed twice
- **THEN** the second run parses no file and reuses the facts of every file

### Requirement: Facts follow the bytes that were read
A file saved between the listing and the read of a run SHALL have its facts filed under the content id of the bytes that were read, so that saving the earlier text back, or a sibling worktree holding it, never finds them under the earlier id. A file that changed while it was being read SHALL have no id, so nothing is filed under it. A file unchanged since the listing SHALL keep the id the listing gave it.

#### Scenario: A file saved during a run is filed under what was read
- **WHEN** a file is saved between the listing and the read of a run
- **THEN** the next refresh is a no-op, and saving the earlier text back parses the file again

### Requirement: File assignment
Each file SHALL belong to at most one project: the first connector in registry order that has a project whose `matches` accepts the file, and among that connector's projects the one with the deepest root. A file no project takes SHALL be counted as unclaimed under its language.

#### Scenario: The deepest root wins
- **WHEN** one connector opens projects rooted at `""` and `apps/web`
- **THEN** `apps/web/src/a.ts` belongs to `apps/web` and `scripts/b.ts` to the root project

#### Scenario: Registry order decides between connectors
- **WHEN** two connectors both have a project matching `schema.graphql`
- **THEN** the file belongs to the connector registered first

#### Scenario: Unclaimed files are counted by language
- **WHEN** the tree holds `Main.kt` and no project matches it
- **THEN** coverage counts one unclaimed `kotlin` file

### Requirement: Fact reuse
The engine SHALL key a file's facts by the fact format, the connector's name and version, the module options, the fingerprint of the packs the connector received, the project salt, the path and the content id. It SHALL take facts with an equal key from this worktree's snapshot, then, as the next requirement says, from the snapshots of sibling worktrees, and parse only when neither has them.

#### Scenario: Only edited files are parsed
- **WHEN** one of three indexed files is edited and the engine refreshes
- **THEN** the run parses one file and reuses two

#### Scenario: Changed options parse that module's files again
- **WHEN** the options of one module change between two runs
- **THEN** every file of that module is parsed again and no file of another module is

#### Scenario: A new worktree reuses its sibling's facts
- **WHEN** a linked worktree is indexed for the first time after the main worktree was indexed at the same commit
- **THEN** the run reuses every file's facts, parses none and reads one sibling snapshot

### Requirement: Sibling snapshots
The engine SHALL read the snapshots of sibling worktrees only when this worktree's snapshot is absent or lacks the facts of more than 64 files and more than a twentieth of the files a connector took. It SHALL leave a sibling snapshot written by another version unread beyond its header.

#### Scenario: Siblings are read only when the own snapshot cannot answer
- **WHEN** a worktree with a snapshot has one file edited, and then seventy files edited the way a sibling holds them
- **THEN** the first run reads no sibling snapshot, and the second reads one and parses nothing

#### Scenario: A sibling of another version is not read
- **WHEN** the only sibling snapshot was written by another version and a new worktree is indexed
- **THEN** the run reads no sibling snapshot and parses every file

### Requirement: Size and encoding limits
A file larger than `Setup.max_file_bytes` SHALL be recorded as `TooLarge` and a file that is not UTF-8 as `NotUtf8`, and neither SHALL be handed to its connector.

#### Scenario: An oversized bundle is not parsed
- **WHEN** a 3 MiB file is indexed under the default limit
- **THEN** its status is `TooLarge` with the size and the limit
- **AND** its connector's `parse` is never called for it

#### Scenario: A binary file is not parsed
- **WHEN** a file claimed by a connector holds invalid UTF-8
- **THEN** its status is `NotUtf8` and its connector's `parse` is never called for it

### Requirement: Run phases
The engine SHALL call each project's `prepare` once with every file it owns before any `resolve`, SHALL resolve every import through its project against the repository view, SHALL send each embedded document to the first project in registry order whose `embeds()` names its language and place the returned facts in the host file at the document's line, and SHALL call each project's `link` once and add the linked facts as sites.

#### Scenario: Prepare sees every file before resolution
- **WHEN** a recording connector owns three files in two projects
- **THEN** both `prepare` calls happen before the first `resolve`
- **AND** each `prepare` receives exactly the files its project owns

#### Scenario: An embedded document reaches its reader
- **WHEN** a file of one connector embeds a `graphql` document starting at line 10, and another connector's project embeds `graphql` and returns a fact at line 2 of the document
- **THEN** the graph has that contract site in the host file at line 11

#### Scenario: An embedded language nobody reads is a blind spot
- **WHEN** a file embeds an `sql` document and no project embeds `sql`
- **THEN** coverage counts the document as an unread site of the host's connector

#### Scenario: Linked facts become sites
- **WHEN** a project's `link` returns a fact for one of its files
- **THEN** the graph has that site on that file

### Requirement: Failure containment
The engine SHALL catch a panic in every connector call. A panic in `parse` SHALL record the file as `Panicked` with the message, a panic in any other call SHALL become a coverage line naming the connector, the project and the call, and the run SHALL finish. Options that do not deserialise SHALL fail the run with an error naming the module and the problem.

#### Scenario: A panicking parse fails one file
- **WHEN** a connector panics while parsing one of three files
- **THEN** the index holds all three files
- **AND** that file's status is `Panicked` with the panic message and the other two have facts

#### Scenario: A panicking link loses only linked facts
- **WHEN** a project's `link` panics
- **THEN** the index is built with every parsed fact and without linked ones
- **AND** coverage names the connector, the project and `link`

#### Scenario: Bad options fail the run
- **WHEN** the setup gives a module an option its schema does not allow, an unknown one or one of the wrong type
- **THEN** the run fails with an error naming the module and the option

### Requirement: Failure containment of detection and matching
A panic in `detect` SHALL mean the connector finds nothing to read, and a panic in `matches` SHALL mean the project does not take that file. Each SHALL become a coverage line naming the connector and the call, and a project whose `matches` panics SHALL add one line, not one per file.

#### Scenario: A panicking detect disables its connector
- **WHEN** a connector's `detect` panics
- **THEN** the run finishes without that connector
- **AND** coverage names the connector and `detect`

#### Scenario: A panicking matches leaves one file unclaimed
- **WHEN** a project's `matches` panics for two of three files
- **THEN** those two files are unclaimed and the third keeps its project
- **AND** coverage names the connector, the project and `matches` once

### Requirement: Determinism
Indexing the same tree with the same setup SHALL give equal indexes apart from `IndexInfo`, whatever the number of threads.

#### Scenario: One thread and eight agree
- **WHEN** a fixture repository is indexed with one rayon thread and with eight
- **THEN** the two indexes are equal apart from `IndexInfo`

### Requirement: Snapshot
The engine SHALL store the index, every file's key, content id, size, modification time, facts and connector extra in `<state dir>/snapshot.gsttt`, written to a temporary file and renamed. The file SHALL start with the magic `GSTTSNAP`, a format version and a JSON header, followed by a validated rkyv body. A snapshot with another magic, format or `built_by`, or one that fails validation, SHALL be treated as absent. `peek` SHALL read the header without reading the body.

#### Scenario: A stored index loads back equal
- **WHEN** an index is stored and loaded
- **THEN** the loaded index equals the stored one

#### Scenario: A corrupt snapshot counts as none
- **WHEN** the snapshot's body is truncated
- **THEN** loading returns no index
- **AND** the next refresh rebuilds and replaces the snapshot

#### Scenario: Peek reads only the header
- **WHEN** `peek` runs on a snapshot whose body bytes are garbage
- **THEN** it returns the header with `built_by`, `head`, `dirty` and `files`

### Requirement: Snapshot write failure
A snapshot that cannot be written SHALL be logged and SHALL NOT fail the run.

#### Scenario: A snapshot that cannot be written does not fail the run
- **WHEN** the state directory cannot be created
- **THEN** the refresh still returns the index it built
- **AND** no snapshot exists afterwards

### Requirement: Unchanged trees answer from the snapshot
A refresh whose file set, content id of every file, keys, setup fingerprint, HEAD commit and gsttt version all match the snapshot SHALL return the stored index without calling `parse`, `prepare`, `resolve` or `link`. A refresh that differs in any of them SHALL rebuild the index and store it.

#### Scenario: A no-op refresh calls no connector work
- **WHEN** the engine refreshes twice with nothing changed in between
- **THEN** the second refresh calls none of `parse`, `prepare`, `resolve` and `link` and reports the index unchanged

#### Scenario: An edit to an unclaimed file is not a no-op
- **WHEN** a file no connector reads is edited after indexing and the engine refreshes
- **THEN** the refresh rebuilds the index and parses no file
- **AND** the index counts one dirty file and the snapshot reports the tree as fresh

#### Scenario: A new commit is not a no-op
- **WHEN** the dirty files are committed after indexing and the engine refreshes
- **THEN** the refresh rebuilds the index with the new head and no dirty file

### Requirement: Freshness
The engine SHALL report, without building an index, how many files were added, removed and changed since a given index was built.

#### Scenario: An edit after indexing makes the index stale
- **WHEN** one file is edited and one added after an index was built
- **THEN** freshness reports one changed file, one added and none removed

### Requirement: Changed files
The engine SHALL list the files of the work tree that differ from `HEAD` as modified, added (including untracked files git does not ignore) and deleted, and, given a base revision, SHALL add the files changed between the merge base of `HEAD` and that revision and `HEAD`. A revision git cannot resolve SHALL be an error naming it.

#### Scenario: Work tree changes are listed by kind
- **WHEN** after a commit one tracked file is edited, one is deleted and one new file is created
- **THEN** the changed files are the edited one as modified, the new one as added and the removed one as deleted

#### Scenario: A base revision adds committed changes
- **WHEN** a branch commits a change to `a.ts` after leaving `main`, the work tree also edits `b.ts`, and the base is `main`
- **THEN** the changed files hold `a.ts` and `b.ts`

#### Scenario: An unknown base is an error
- **WHEN** the base revision is `no-such-branch`
- **THEN** listing changed files fails with an error naming `no-such-branch`

### Requirement: Unreadable history
A HEAD or a history git cannot read SHALL NOT fail an index run. The index SHALL then record no head or no dirty count, and its coverage SHALL carry a line saying what git could not tell. A repository without commits SHALL have no head and no such line.

#### Scenario: A HEAD git cannot read is a coverage line
- **WHEN** the checked-out branch points at a commit the repository does not hold
- **THEN** the run finishes with no head and no dirty count
- **AND** the coverage has a line naming HEAD

#### Scenario: A history git cannot read is a coverage line
- **WHEN** the tree of the HEAD commit is missing from the repository
- **THEN** the run finishes with the head and no dirty count
- **AND** the coverage has a line saying the files that differ from HEAD are not counted

### Requirement: Pack merging
The engine SHALL hand each connector the built-in packs of its language without those named in `Setup.disable_packs`, with each pack of `Setup.packs` replacing the built-in pack of the same name or adding to them when no built-in has that name.

#### Scenario: A user pack replaces the built-in one
- **WHEN** the setup holds a `ts` pack named `express` and the built-in packs include `express`
- **THEN** the TypeScript connector receives the setup's `express` and not the built-in one

#### Scenario: A disabled pack reaches nobody
- **WHEN** `disable_packs` names `axios`
- **THEN** no connector receives a pack named `axios`

### Requirement: Coverage counting
Coverage SHALL count per connector the files by parse status, the imports by resolution and the unread sites by reason, keeping the first place where each reason occurred.

#### Scenario: An unresolved import is counted with its place
- **WHEN** `src/a.ts` imports `./missing` at line 3 and no such file exists
- **THEN** the TypeScript coverage counts one `not-found` import whose first place is `src/a.ts:3`

### Requirement: Contract keys
Every HTTP contract key SHALL pass through one normaliser, whichever connector made it: the method in upper case or `*`, and the path without scheme, host, query, fragment and repeated or trailing slashes, with each parameter segment as `{}` and a placeholder before the first `/` dropped as the base URL.

#### Scenario: Paths from every framework meet
- **WHEN** keys are made from `/bookings/:id`, `/bookings/{id:int}`, `/bookings/<int:id>`, `${API_URL}/bookings/${booking.id}?expand=1` and `https://api.example.com/bookings/{}#top`
- **THEN** each has the path `/bookings/{}`
