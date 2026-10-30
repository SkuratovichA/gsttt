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
