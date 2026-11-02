# LEARNING

Mistakes made in this repository and what fixed them.

## A fact type changed shape and the snapshot layout did not

`StoredFile` keeps each file's `FileFacts` in the rkyv body of the snapshot, so a new field in `FileFacts`, or in any type it holds, changes the archived layout. Bumping `FACT_FORMAT` in `crates/core/src/engine/keys.rs` makes every cached fact be read again, but it does not stop a snapshot written by the previous build from being opened as the new layout. Fix: bump `LAYOUT` in `crates/core/src/engine/snapshot.rs` in the same commit as any change to a type the snapshot body holds, so that `parse_prefix` refuses the old file.

## Builds in several worktrees at once failed with errors the code did not have

`CARGO_TARGET_DIR` is set in the environment to the `target` of the main checkout, so every worktree builds there and not into its own `target`. Two worktrees with different sources of the same crates rebuilt each other's artifacts and got "unresolved import" errors that neither tree had. Fix: when more than one worktree builds at a time, give each its own with `CARGO_TARGET_DIR=<worktree>/target` on the command.

## gsttt use in agent transcripts was counted too high

Grepping a transcript of `cargo xtask agents run` for `gsttt` counted every Bash command that names a path in the checkout the agent works in, because that path holds the word. Fix: count the calls of the gsttt MCP tools by their names, and a Bash command only when the program it runs is `gsttt`.

## openspec validate --strict failed on a requirement that had grown

A sentence added to a requirement of a delta took its text past 500 characters, and `openspec validate --strict` turns that warning into a failure. Fix: a new behaviour gets a requirement of its own with its scenario, rather than one more sentence in a requirement that already holds several.

## openspec validate rejected a MODIFIED requirement that dropped scenarios

A MODIFIED requirement in a delta must carry every scenario the requirement has in the current spec, so one whose scenarios were renamed or split failed `openspec validate`. Fix: REMOVE the old requirement with its `**Reason**:` and `**Migration**:` lines and ADD the new ones under new names, keeping word for word the name of every scenario that stays; rename in the same change every test that names a scenario of the removed requirement, because `cargo xtask spec` fails on it.

## A glob in a zsh command aborted the whole command

The shell of the sessions is zsh, where a glob that matches nothing is an error (`no matches found`) that stops the command before it runs, so `grep -rn x --include=*.rs` and `ls dir/vitest.config.*` failed instead of matching nothing. Fix: quote every glob that is meant for the program (`--include='*.rs'`), and test for a file with `[ -e ]` or run the line under `bash -c`. A word that starts with `=` is expanded to the path of a command as well, so a separator `echo ===` failed with `== not found`: quote it (`echo '==='`).

## main broke after the archive bot merged a change into a spec

Archiving `http-placeholder-urls` made the Python requirement "HTTP requests" 543 characters long, over the 500 that `openspec validate --strict` allows, although the change had validated in its pull request. The archive-specs workflow pushes with the default token, and GitHub starts no workflow for such a push, so nothing checked main and the OpenSpec check failed on the next pull request instead. Fix: after an archive commit lands, run `openspec validate --all --strict` on main and fix a failure in a pull request of its own (#39).

## A cherry-picked line of tasks.md kept its task number

A task line cherry-picked from another change into `tasks.md` kept the number it had there, so the file held the same task ID twice and `openspec validate --strict` failed on the duplicate. Fix: renumber the picked line to the next free number of the file in the same commit, and run `openspec validate <change> --strict` before the commit.

## A branch made from a remote-tracking ref tracked it

`git worktree add -b x <path> origin/main` sets `origin/main` as the upstream of the new branch, so a plain `git push` or `git pull` on a branch that was meant to stay local talked to main. Fix: create such a branch with `--no-track` (`git worktree add --no-track -b x <path> origin/main`), or run `git branch --unset-upstream` on it, and check with `git rev-parse --abbrev-ref x@{upstream}`, which must fail.

## A build in the main checkout handed back the binary of a worktree

With one `CARGO_TARGET_DIR` for the main checkout and a worktree, a crate gets the same unit hash in both trees and cargo decides freshness by file times, so `cargo build --release` in the main checkout after a build in the worktree finished in a quarter of a second and left the worktree's binary in `target/release`. A timing of "main against the branch" then compared the branch with itself. Fix: build the other tree into a target dir of its own (`CARGO_TARGET_DIR=<scratch>/target-main`), and compare `shasum` of the two binaries before measuring.

## A rule fix in core would have left the facts of unchanged files as they were

The key of a file's cached facts covers the fact format, the connector's name and version, the options and the packs, but not the code that applies a rule, so a fix in `crates/core` that reads the same text differently leaves every unchanged file with its old facts. The first commit of such a fix did not bump anything, and review caught it. Fix: a change in `crates/core` that gives a different outcome for the same text bumps `FACT_FORMAT` in `crates/core/src/engine/keys.rs` and names the bump in the proposal; a change confined to one connector bumps that connector's version instead, such as `READER_VERSION` in `crates/lang-ts/src/project.rs`.

## A field result was put down to a stale snapshot that could not exist

A drop in a field score was first explained as a stale snapshot of the index, but the scoring script removes the snapshot before each run, so no run could have read one. Fix: before blaming a cache, check whether the run could have used one, and call the cause unverified until then.

## A speed difference of 125 against 137 ms was noise

One timing of each side gave 125 against 137 ms, and the difference was within the spread of repeated runs of one binary. Fix: time each side several times, alternating the two, and report the median and the spread.
