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
