# gsttt for Claude Code

This plugin gives Claude Code the gsttt MCP server, a skill that says when to ask it, and a hook that tells you when the `gsttt` binary is not installed.

## Install

1. Install the binary:

   ```sh
   curl -fsSL https://raw.githubusercontent.com/SkuratovichA/gsttt/main/install.sh | sh
   ```

   The installer checks the SHA-256 of what it downloads and puts `gsttt` in `~/.local/bin`, or in `GSTTT_INSTALL_DIR` when that is set. The plugin finds it there either way; to run `gsttt` by name in a terminal, put that directory on your `PATH`, as the installer says if it is not. The checksum comes from the same release as the package, so it guards against a damaged download and not against a compromised release.

2. Add the marketplace and install the plugin from Claude Code.

   ```text
   /plugin marketplace add SkuratovichA/gsttt
   /plugin install gsttt@gsttt
   ```

3. Update both later with one command. `gsttt update` installs the newest release of the binary and updates this plugin in every Claude Code config on the machine that has it. It prints the version of the plugin before and after for each install, such as `Updated the plugin in ~/.claude (user): 9b6517d98ed1 → 4c1f2aa3b5c6`. Claude Code sessions that are running keep the plugin they loaded, so restart them when it says so.

   ```sh
   gsttt update
   ```

## What is in it

| Path | What it is |
| --- | --- |
| `.claude-plugin/plugin.json` | The manifest: the skill, the hook and the MCP server. |
| `scripts/gsttt.sh` | Runs `gsttt` for the hook and the server, found on `PATH` or in the install directory. A hook never fails a session: its errors are dropped and it is cut off after 5 seconds. When `gsttt` is nowhere, a session start prints one note for you with the install command, other hook events do nothing and the server stops with one line on stderr. |
| `skills/gsttt/SKILL.md` | When to ask gsttt and how to read its answers. |

`cargo xtask check` in the repository checks that the manifests parse and that every path they name exists.
