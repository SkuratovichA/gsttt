# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the deltas of the `cli` and `distribution` specs

## 2. The installer

- [x] 2.1 `install.sh` leaves a gsttt that already is the version to install as it is, downloads nothing and prints the `PATH` hint, with the scenario of `An installed version is not downloaded again`

## 3. The command

- [x] 3.1 `update [VERSION]` in the command line, refusing `-C`, `--json`, `--all` and `--no-refresh` with exit 2
- [x] 3.2 The install step: the embedded installer, the install directory, the refusal of a development build, and the failed install that leaves the plugin alone
- [x] 3.3 The plugin step: the configs that have the plugin, the calls of `claude` for each, the notes and the exit code of a failure
- [x] 3.4 The scenarios of the `cli` requirements `The update command` and `The plugin follows the binary`

## 4. Docs

- [x] 4.1 The install sections of `README.md` and `plugin/README.md`

## 5. What the update says

- [x] 5.1 The plugin lines say the version before and after for each install, and `claude`'s own output is captured and quoted only when a call failed
- [x] 5.2 Everything `update` says, the installer's lines included, goes to stderr, and stdout stays empty
- [x] 5.3 The restart line is said only after a binary was installed or a plugin version changed
- [x] 5.4 The scenarios of `The update speaks on stderr`, `The plugin step says what changed` and `The restart is asked for after a change`

## 6. Review fixes

- [x] 6.1 The lines to run by hand name their config either way (`CLAUDE_CONFIG_DIR` unset for `~/.claude`), run a project's call in a subshell, and the integration tests pin their quoting with a config and a project whose names have a space and a quote
- [x] 6.2 A failed marketplace refresh skips the plugin calls of that marketplace in that config, with a note for each, and the exit code is still 1
- [x] 6.3 A project that goes between the plan and its call is left out with a note, not read as a missing `claude`
- [x] 6.4 An install counts as changed when its version or its commit differs, or it had no version before; the restart is asked for only when a plugin changed or a binary was installed where a plugin exists
- [x] 6.5 The scenarios of `A marketplace that did not refresh leaves its plugins alone` and `The lines to run by hand are safe to paste`, and the wording of the restart and failure requirements
- [x] 6.6 The lines to run by hand call `claude` by name, so an alias or a function works, and unset `CLAUDE_CONFIG_DIR` in a subshell for `~/.claude`; an empty install list is no plugin; a commit counts as changed only when both records have one; a `GSTTT_VERSION` in the environment is not passed on, with its scenario
