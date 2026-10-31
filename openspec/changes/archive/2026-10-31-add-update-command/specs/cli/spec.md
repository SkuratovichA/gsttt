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

`gsttt update [<version>]` SHALL run `install.sh` to put the newest release of gsttt, or `<version>`, into `GSTTT_INSTALL_DIR` or else the directory of the running binary, without passing on a `GSTTT_VERSION` it inherited, then update the Claude Code plugin unless a version was named. It SHALL not replace a development build when `GSTTT_INSTALL_DIR` is unset, SHALL exit 1 and leave the plugin alone when the installer fails, and SHALL exit 2 with `-C`, `--json`, `--all` or `--no-refresh`.

#### Scenario: An update with nothing new still updates the plugin
- **WHEN** `gsttt update` runs and the installer finds the newest version already installed
- **THEN** the installer's line on stderr says so, the plugin is updated in the config that has it and said to be already the newest, stderr has no line that asks for a restart, and the exit code is 0

#### Scenario: A version in the environment does not name the release
- **WHEN** `gsttt update` runs with no version and with `GSTTT_VERSION` set in its environment to a version that is not the newest
- **THEN** the installer is run for the newest release, which stderr says is installed, and not for the version of the environment, and the exit code is 0

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
- **WHEN** an install of the plugin names a project directory that is not on disk, or that goes between the planning of the calls and the call
- **THEN** stderr notes that the install was left out because its project is not a directory, does not say that `claude` is missing, the other installs are updated and the exit code is 0

### Requirement: The update speaks on stderr

`gsttt update` has no answer, so it SHALL write nothing to stdout, whatever the exit code. What it says, what the installer says and its notes SHALL go to stderr.

#### Scenario: Update writes nothing to stdout
- **WHEN** `gsttt update` runs and works, runs with a `claude` that fails, runs with a named version, runs where no config has the plugin, and runs with an installer that fails
- **THEN** stdout is empty each time, the installer's lines are on stderr, and the exit codes are 0, 1, 0, 0 and 1

### Requirement: The plugin step says what changed

For each install it updates, `gsttt update` SHALL compare the `version` and `gitCommitSha` that `plugins/installed_plugins.json` records before and after the call, and say `Updated the plugin in <config> (<scope>): <old> → <new>` when either became another (`to <new>` when it had no version before), or `The plugin in <config> (<scope>) is already the newest: <version>`. It SHALL not show what `claude` prints, and SHALL note it and exit 1 when the install has no version after the call.

#### Scenario: The plugin lines say what changed
- **WHEN** `gsttt update` runs for a config with a user install that `claude` moves from one version to another and a project install that is at the newest version already, a config with an install that has no version until `claude` gives it one, and a config with an install that stays at its version and moves to another commit of it
- **THEN** stderr has the lines `Updated the plugin in ~/.claude (user): <old> → <new>`, `The plugin in ~/.claude (local, ~/app) is already the newest: <version>`, `Updated the plugin in <config> (user) to <version>` and `Updated the plugin in <config> (user): <version>, commit <old> → <new>`, the home directory being written `~` and the project following the scope, holds nothing that `claude` printed, and the exit code is 0

#### Scenario: An install that is gone after the update is a failure
- **WHEN** an install is no longer in `plugins/installed_plugins.json` after `claude` updated it
- **THEN** stderr notes that the plugin in that config and scope has no version in the file after the update, and the exit code is 1

### Requirement: The plugin step tells what it could not do

When a call of `claude` fails, `gsttt update` SHALL note the config and scope, the line that repeats the call and the last 5 lines of what `claude` printed (its stdout, then its stderr), run the calls after it, and exit 1. When `claude` is not found it SHALL note which configs were left as they were and the lines that update them, and exit 1. When no config has the plugin it SHALL print `/plugin marketplace add SkuratovichA/gsttt` and `/plugin install gsttt@gsttt`, not run `claude`, and exit 0.

#### Scenario: A failing plugin update fails the command
- **WHEN** `claude` fails to update the plugin of a project install in a config, the config and the project having a space and a single quote in their names, and works for another config
- **THEN** the other config is updated, stderr notes the config and scope of the call that failed with the line that repeats it, written with the quotes a shell needs, and the last 5 lines of what `claude` printed, its stdout and then its stderr, and the exit code is 1

#### Scenario: A machine without claude is told which configs were not updated
- **WHEN** `claude` is not on `PATH` and two configs have the plugin
- **THEN** stderr names both configs with the lines that update them, and the exit code is 1

#### Scenario: Without the plugin the commands to add it are printed
- **WHEN** no config has the plugin
- **THEN** stderr holds the two commands that add it in Claude Code, `claude` is not called, and the exit code is 0

### Requirement: A marketplace that did not refresh leaves its plugins alone

When `claude plugin marketplace update` fails for a marketplace in a config, `gsttt update` SHALL not update the plugin from that marketplace in that config, whose copy of it is stale, SHALL note for each install that it was not updated because the marketplace was not refreshed, SHALL update the other configs, and SHALL exit 1.

#### Scenario: A failed marketplace refresh leaves its plugins alone
- **WHEN** `claude` fails to refresh the marketplace in a config that has a user install and a project install, and works for another config
- **THEN** `claude` is not called for those two installs, stderr notes for each that the plugin in that config and scope was not updated because the marketplace was not refreshed, the other config is updated, and the exit code is 1

### Requirement: The lines to run by hand are safe to paste

Each line `gsttt update` prints for a person to run SHALL call `claude` by its name, so that an alias or a function of the shell works, and name the config it is for, as `CLAUDE_CONFIG_DIR=<config>` or, for `~/.claude`, by unsetting `CLAUDE_CONFIG_DIR` in a subshell. It SHALL quote each word a shell would split, and run the call of a project as `(cd <project> && …)`, so that a shell that runs several is left in the directory it was in.

#### Scenario: The lines to run by hand are safe to paste
- **WHEN** `claude` is not on `PATH`, the plugin is installed in `~/.claude`, and in a config and for a project whose names have a space and a single quote, and the four lines are run in one shell that has another `CLAUDE_CONFIG_DIR` exported and where `claude` is a function, not a file on `PATH`
- **THEN** `claude` is called four times, with `CLAUDE_CONFIG_DIR` unset for the two calls of `~/.claude` and set to the config the line names for the others, the call of the project runs in the project, and the shell ends in the directory it started in

### Requirement: The restart is asked for after a change

`gsttt update` SHALL say `Restart running Claude Code sessions to load the plugin.` after the plugin lines, and before the report of an error when the run fails, when the recorded version of a plugin install changed, or its recorded commit changed to another, or when it installed another version of the binary and a config has the plugin. It SHALL not say it otherwise.

#### Scenario: The restart is asked for only after a change
- **WHEN** `gsttt update` runs with nothing new, with a binary to install and a plugin at the newest version, with a named version to install, with a plugin to move to another version, and with a binary to install, named or not, where no config has the plugin
- **THEN** stderr ends with the line for the second, third and fourth, and does not have it for the first and the last
