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
- **THEN** the installer's line on stderr says so, the plugin is updated in the config that has it and said to be already the newest, stderr has no line that asks for a restart, and the exit code is 0

#### Scenario: A named version leaves the plugin alone
- **WHEN** `gsttt update 0.1.98` runs and that version is already installed
- **THEN** stderr says the plugin follows the newest release and is left as it is, `claude` is not called, and the exit code is 0

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

### Requirement: The update speaks on stderr

`gsttt update` has no answer, so it SHALL write nothing to stdout, whatever the exit code. What it says, what the installer says and its notes SHALL go to stderr.

#### Scenario: Update writes nothing to stdout
- **WHEN** `gsttt update` runs and works, runs with a `claude` that fails, runs with a named version, runs where no config has the plugin, and runs with an installer that fails
- **THEN** stdout is empty each time, the installer's lines are on stderr, and the exit codes are 0, 1, 0, 0 and 1

### Requirement: The plugin step says what changed

For each install it updates, `gsttt update` SHALL read the install's `version` from `plugins/installed_plugins.json` before the call and after it, and say `Updated the plugin in <config> (<scope>): <old> → <new>` when it changed, or `The plugin in <config> (<scope>) is already the newest: <version>`. It SHALL not show what `claude` prints, and SHALL note it and exit 1 when the install has no version in the file after the call.

#### Scenario: The plugin lines say what changed
- **WHEN** `gsttt update` runs for a config with a user install that `claude` moves from one version to another and a project install that is at the newest version already
- **THEN** stderr has the line `Updated the plugin in ~/.claude (user): <old> → <new>` and the line `The plugin in ~/.claude (local, ~/app) is already the newest: <version>`, the home directory being written `~` and the project following the scope, holds nothing that `claude` printed, and the exit code is 0

#### Scenario: An install that is gone after the update is a failure
- **WHEN** an install is no longer in `plugins/installed_plugins.json` after `claude` updated it
- **THEN** stderr notes that the plugin in that config and scope has no version in the file after the update, and the exit code is 1

### Requirement: The plugin step tells what it could not do

When a call of `claude` fails, `gsttt update` SHALL note the config and scope, the line that repeats the call by hand and at most the last 5 lines `claude` printed, run the calls after it, and exit 1. When `claude` is not found it SHALL note which configs were left as they were and the lines that update them, and exit 1. When no config has the plugin it SHALL print `/plugin marketplace add SkuratovichA/gsttt` and `/plugin install gsttt@gsttt`, not run `claude`, and exit 0.

#### Scenario: A failing plugin update fails the command
- **WHEN** `claude` fails for one config and works for the others
- **THEN** the others are updated, stderr notes the config and scope of the call that failed with the line that repeats it and the last 5 lines `claude` printed, and the exit code is 1

#### Scenario: A machine without claude is told which configs were not updated
- **WHEN** `claude` is not on `PATH` and two configs have the plugin
- **THEN** stderr names both configs with the lines that update them, and the exit code is 1

#### Scenario: Without the plugin the commands to add it are printed
- **WHEN** no config has the plugin
- **THEN** stderr holds the two commands that add it in Claude Code, `claude` is not called, and the exit code is 0

### Requirement: The restart is asked for after a change

`gsttt update` SHALL end with the line `Restart running Claude Code sessions to load the plugin.` when it installed another version of the binary or a plugin version changed, whatever the exit code, and SHALL not say it otherwise.

#### Scenario: The restart is asked for only after a change
- **WHEN** `gsttt update` runs with nothing new, with a binary to install and a plugin at the newest version, with a named version to install, and with a plugin to move to another version
- **THEN** stderr ends with the line for the last three and does not have it for the first
