# Proposal: gsttt update

## Why

Updating gsttt means remembering how it was installed, and the Claude Code plugin is a second thing to update on top of the binary. Claude Code versions this plugin by the git commit of the marketplace repository, so the plugin moves with the default branch while the binary moves with releases. Left alone, the two drift apart. Turning on the auto-update of the marketplace was rejected, because it moves the plugin without the binary.

## What Changes

- `gsttt update [VERSION]` installs the newest release of gsttt, or VERSION, over the running binary by running the same `install.sh` that `curl … | sh` runs, which the binary embeds. The directory is `GSTTT_INSTALL_DIR` or else the one the running binary is in. A development build, one built from source, is not replaced unless `GSTTT_INSTALL_DIR` names a directory.
- After a newest-release install it updates the plugin in every Claude Code config on the machine that has it, by running `claude`: the config of `CLAUDE_CONFIG_DIR` and every `~/.claude*` directory. It updates the marketplace, then each install in its scope, from the project of an install that has one. A named VERSION leaves the plugin alone, because Claude Code installs a plugin only at the newest commit of its marketplace.
- `install.sh` downloads nothing when the gsttt in the install directory already is the version to install. This is what makes `gsttt update` cheap when there is nothing new.
- A failed install ends the command with exit 1 and leaves the plugin alone. A failed plugin update, or a machine without `claude`, ends it with exit 1 after the other configs were updated, naming the ones that were not.

## Impact

- `crates/gsttt`: the command, its arguments, the errors, and the `update` module with the installer step and the plugin step.
- `install.sh`: the check for an installed version, and the `PATH` hint as a function.
- Specs: `cli` (Commands, and two new requirements), `distribution` (a new requirement).
- Docs: the install sections of `README.md` and `plugin/README.md`.
