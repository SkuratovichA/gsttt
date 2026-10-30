# Spec Delta

## MODIFIED Requirements

### Requirement: Commands

The `gsttt` binary SHALL offer the commands `index`, `impact`, `dependents`, `dependencies`, `path`, `contract`, `coverage`, `status`, `mcp`, `hook session-start`, `init`, `config check`, `config show`, `config trust`, `config untrust`, `schema external`, `update [<version>]` and `completions <shell>`, and the global flags `-C <dir>`, `--json`, `--all` and `--no-refresh`. With no command it SHALL print the usage to stderr and exit 2.

#### Scenario: No command prints the usage
- **WHEN** `gsttt` runs with no arguments
- **THEN** stderr lists every command, stdout is empty and the exit code is 2

#### Scenario: Completions are printed for a shell
- **WHEN** `gsttt completions zsh` runs
- **THEN** stdout holds a zsh completion script for `gsttt` and the exit code is 0

## ADDED Requirements

### Requirement: The update command

`gsttt update [<version>]` SHALL run `install.sh` to put the newest release of gsttt, or `<version>`, into `GSTTT_INSTALL_DIR` or else the directory of the running binary, then update the Claude Code plugin unless a version was named. It SHALL not replace a development build when `GSTTT_INSTALL_DIR` is unset, SHALL exit 1 and leave the plugin alone when the installer fails, and SHALL exit 2 with `-C`, `--json`, `--all` or `--no-refresh`.

#### Scenario: An update with nothing new still updates the plugin
- **WHEN** `gsttt update` runs and the installer finds the newest version already installed
- **THEN** the installer's line says so, the plugin is updated in the config that has it, and the exit code is 0

#### Scenario: A named version leaves the plugin alone
- **WHEN** `gsttt update 0.1.98` runs and that version is already installed
- **THEN** stdout says the plugin follows the newest release and is left as it is, `claude` is not called, and the exit code is 0

#### Scenario: A failed install leaves the plugin alone
- **WHEN** `gsttt update` runs and the installer exits non-zero
- **THEN** stderr says the plugin was left as it was, `claude` is not called, and the exit code is 1

#### Scenario: A gsttt built from source is not replaced
- **WHEN** `gsttt update` runs on a development build with `GSTTT_INSTALL_DIR` unset
- **THEN** stderr names `GSTTT_INSTALL_DIR`, stdout is empty, the installer and `claude` are not run, and the exit code is 1

#### Scenario: The update takes no repository flags
- **WHEN** `gsttt update` runs with `-C <dir>`, `--json`, `--all` or `--no-refresh`
- **THEN** stderr names the flag, stdout is empty, nothing is installed and the exit code is 2

### Requirement: The plugin follows the binary

After it installs the newest release, `gsttt update` SHALL update the gsttt plugin by running `claude` in every Claude Code config that has it: `CLAUDE_CONFIG_DIR` and each directory in the home directory named `.claude*`, a config that shares its `plugins` directory with an earlier one counting once. For each marketplace it SHALL run `claude plugin marketplace update`, then `claude plugin update gsttt@<marketplace> --scope <scope>` for each install, from the project of an install that has one.

#### Scenario: Every config that has the plugin is updated
- **WHEN** the plugin is installed in `.claude` for the user and for a project, in `.claude-work`, and in a config that `CLAUDE_CONFIG_DIR` names, while `.claude-backup` links to the `plugins` of `.claude` and `.claude-none` lacks the plugin
- **THEN** `claude` runs once for each marketplace and each install of those three configs, in the order of their paths, from the project for the project install and from the home directory otherwise, with `CLAUDE_CONFIG_DIR` unset for `.claude` and set to the config for the others, and runs for neither of the other two

#### Scenario: A project that no longer exists is skipped
- **WHEN** an install of the plugin names a project directory that is not on disk
- **THEN** stderr notes that the install was left out because its project is not a directory, the other installs are updated and the exit code is 0

### Requirement: The plugin step tells what it could not do

When a call of `claude` fails, `gsttt update` SHALL note it, run the calls after it, and exit 1. When `claude` is not found it SHALL note which configs were left as they were and the lines that update them, and exit 1. When no config has the plugin it SHALL print `/plugin marketplace add SkuratovichA/gsttt` and `/plugin install gsttt@gsttt`, not run `claude`, and exit 0. After an update that worked it SHALL say that running sessions need a restart.

#### Scenario: A failing plugin update fails the command
- **WHEN** `claude` fails for one config and works for the others
- **THEN** the others are updated, stderr notes the call that failed, and the exit code is 1

#### Scenario: A machine without claude is told which configs were not updated
- **WHEN** `claude` is not on `PATH` and two configs have the plugin
- **THEN** stderr names both configs with the lines that update them, and the exit code is 1

#### Scenario: Without the plugin the commands to add it are printed
- **WHEN** no config has the plugin
- **THEN** stdout holds the two commands that add it in Claude Code, `claude` is not called, and the exit code is 0
