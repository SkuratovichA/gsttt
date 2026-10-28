# gsttt for Claude Code

This plugin gives Claude Code the gsttt MCP server, a skill that says when to ask it, and a hook that tells you when the `gsttt` binary is not installed.

## Install

1. Install the binary:

   ```sh
   curl -fsSL https://raw.githubusercontent.com/SkuratovichA/gsttt/main/install.sh | sh
   ```

   The installer checks the SHA-256 of what it downloads and puts `gsttt` in `~/.local/bin`, or in `GSTTT_INSTALL_DIR` when that is set. Make sure that directory is on your `PATH`.

2. Add the marketplace and install the plugin from Claude Code:

   ```text
   /plugin marketplace add SkuratovichA/gsttt
   /plugin install gsttt@gsttt
   ```

## What is in it

| Path | What it is |
| --- | --- |
| `.claude-plugin/plugin.json` | The manifest: the skill, the hook and the MCP server. |
| `scripts/gsttt.sh` | Runs `gsttt` for the hook and the server. When `gsttt` is not on `PATH`, a session start prints one line with the install command and nothing else happens. |
| `skills/gsttt/SKILL.md` | When to ask gsttt and how to read its answers. |

`cargo xtask check` in the repository checks that the manifests parse and that every path they name exists.
