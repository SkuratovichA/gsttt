# Spec Delta

## ADDED Requirements

### Requirement: The skill speaks of the moment of work
The description of the skill SHALL begin with the moment it is for, before an edit of a file that other files import or call and before a change is finished, and the skill SHALL tell the agent to load the tools of the server before it calls them, naming a tool only by the id Claude Code gives a plugin's tool, `mcp__plugin_<plugin>_<server>__<tool>`, built from the names in the manifest.

#### Scenario: The description of the skill names the moment of work
- **WHEN** the description of `plugin/skills/gsttt/SKILL.md` is read
- **THEN** it begins with `Use before you edit a file that other files import or call` and names the moment before a change is finished

#### Scenario: The skill names a tool only by the id the manifest gives
- **WHEN** every `mcp__` id in `plugin/skills/gsttt/SKILL.md` is compared with the plugin name and the server name of the manifest
- **THEN** each is `mcp__plugin_<plugin>_<server>__` and a tool of the server, and the skill names at least one
