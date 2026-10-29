//! The init event of a run, and the check that the run loaded what its setup gives it and nothing else: the tools, the MCP servers, the plugins, and the skills, commands and agents.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, ensure};
use serde::Deserialize;
use serde_json::Value;

use super::setups::Setup;

/// What Claude Code says it loaded, in the first event of its stream.
#[derive(Debug, Deserialize)]
pub(super) struct Init {
    pub(super) tools: Vec<String>,
    pub(super) mcp_servers: Vec<McpServer>,
    pub(super) plugins: Vec<Plugin>,
    pub(super) skills: Vec<String>,
    pub(super) slash_commands: Vec<String>,
    pub(super) agents: Vec<String>,
    /// The id of the model, not the alias the run was asked for.
    pub(super) model: String,
    pub(super) claude_code_version: String,
    /// The id of the session, which names the directory Claude Code makes for it in the temporary root that every session of the user shares.
    pub(super) session_id: String,
}

/// An MCP server and whether it started.
#[derive(Debug, Deserialize)]
pub(super) struct McpServer {
    pub(super) name: String,
    pub(super) status: String,
}

/// A plugin, by the name it has and where it came from: `<name>@builtin` for the ones Claude Code ships, `<name>@inline` for the ones given with `--plugin-dir`.
#[derive(Debug, Deserialize)]
pub(super) struct Plugin {
    pub(super) name: String,
    pub(super) source: String,
}

/// The init event `event` is, or `None` when it is another event.
pub(super) fn from_event(event: &Value) -> anyhow::Result<Option<Init>> {
    if event["type"] != "system" || event["subtype"] != "init" {
        return Ok(None);
    }
    Init::deserialize(event)
        .map(Some)
        .context("the init event of a run lacks a field the task reads")
}

/// The names the configuration of the user holds, which no run may load.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct UserEntries {
    /// Its skills and its commands, which both are slash commands.
    pub(super) skills: BTreeSet<String>,
    pub(super) agents: BTreeSet<String>,
}

/// Fails, naming the setup and what the run loaded, at the first thing a run of `setup` should not have.
pub(super) fn check(setup: Setup, init: &Init, user: &UserEntries) -> anyhow::Result<()> {
    let own = setup.tools();
    let prefix = setup.mcp_prefix();
    for tool in &init.tools {
        let allowed =
            own.contains(&tool.as_str()) || prefix.is_some_and(|prefix| tool.starts_with(prefix));
        ensure!(allowed, "a `{setup}` run loaded the tool `{tool}`");
    }
    for tool in &own {
        ensure!(
            init.tools.iter().any(|loaded| loaded == tool),
            "a `{setup}` run did not load the tool `{tool}`"
        );
    }
    if let Some(prefix) = prefix {
        ensure!(
            init.tools.iter().any(|tool| tool.starts_with(prefix)),
            "a `{setup}` run loaded no tool of its MCP server"
        );
    }

    for server in &init.mcp_servers {
        ensure!(
            Some(server.name.as_str()) == setup.mcp_server(),
            "a `{setup}` run loaded the MCP server `{}`",
            server.name
        );
        ensure!(
            server.status == "connected",
            "the MCP server `{}` of a `{setup}` run is `{}`, not connected",
            server.name,
            server.status
        );
    }
    if let Some(name) = setup.mcp_server() {
        ensure!(
            init.mcp_servers.iter().any(|server| server.name == name),
            "a `{setup}` run did not load its MCP server `{name}`"
        );
    }

    // The plugins Claude Code ships are part of it; every other one was given to the run or found on the machine. The one that was given is known by its source as well as its name: a plugin of the same name from a marketplace is another plugin.
    let loaded = init
        .plugins
        .iter()
        .filter(|plugin| !plugin.source.ends_with("@builtin"));
    let given = setup.plugin().zip(setup.plugin_source());
    for plugin in loaded.clone() {
        ensure!(
            given
                .as_ref()
                .is_some_and(|(name, source)| { plugin.name == *name && plugin.source == *source }),
            "a `{setup}` run loaded the plugin `{}`",
            plugin.source
        );
    }
    if let Some((name, source)) = &given {
        ensure!(
            loaded
                .clone()
                .any(|plugin| plugin.name == *name && plugin.source == *source),
            "a `{setup}` run did not load its plugin `{name}` from `{source}`"
        );
    }

    for (what, names, own) in [
        ("skill", &init.skills, &user.skills),
        ("command", &init.slash_commands, &user.skills),
        ("agent", &init.agents, &user.agents),
    ] {
        for name in names {
            // A name with a colon is a plugin's. Only the gsttt plugin has one, and only the `gsttt` setup has the plugin.
            ensure!(
                !name.contains(':') || (setup == Setup::Gsttt && name.starts_with("gsttt:")),
                "a `{setup}` run loaded the {what} `{name}` of a plugin"
            );
            ensure!(
                !own.contains(name),
                "a `{setup}` run loaded the {what} `{name}` of the configuration of the user"
            );
        }
    }
    Ok(())
}

/// The directory Claude Code keeps the configuration of the user in: where `CLAUDE_CONFIG_DIR` says, else `~/.claude`. `host` answers for a variable of the environment of this task.
pub(super) fn config_dir(host: &dyn Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    host("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| host("HOME").map(|home| PathBuf::from(home).join(".claude")))
}

/// The names of the skills, commands and agents in `config`.
pub(super) fn user_entries(config: &Path) -> UserEntries {
    let names = |kinds: &[&str]| -> BTreeSet<String> {
        kinds
            .iter()
            .flat_map(|kind| {
                fs::read_dir(config.join(kind))
                    .into_iter()
                    .flatten()
                    .flatten()
            })
            .filter_map(|entry| {
                // A skill is a directory; a command and an agent are a file.
                let path = entry.path();
                let name = if path.is_dir() {
                    path.file_name()
                } else {
                    path.file_stem()
                };
                name.map(|name| name.to_string_lossy().into_owned())
            })
            .collect()
    };
    UserEntries {
        skills: names(&["skills", "commands"]),
        agents: names(&["agents"]),
    }
}

#[cfg(test)]
fn names(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

#[cfg(test)]
fn builtin() -> Vec<Plugin> {
    ["cc-plugin-agents-md", "cc-plugin-telemetry"]
        .iter()
        .map(|name| Plugin {
            name: (*name).to_owned(),
            source: format!("{name}@builtin"),
        })
        .collect()
}

/// What Claude Code says about a run of `setup` that loaded what it should and nothing else.
#[cfg(test)]
pub(super) fn expected(setup: Setup) -> Init {
    let mut tools = names(&setup.tools());
    let mut servers = Vec::new();
    let mut plugins = builtin();
    let mut skills = names(&["debug", "simplify"]);
    if let (Some(prefix), Some(name)) = (setup.mcp_prefix(), setup.mcp_server()) {
        tools.push(format!("{prefix}search_graph"));
        servers.push(McpServer {
            name: name.to_owned(),
            status: "connected".to_owned(),
        });
    }
    if let (Some(name), Some(source)) = (setup.plugin(), setup.plugin_source()) {
        plugins.push(Plugin {
            name: name.to_owned(),
            source,
        });
    }
    if setup == Setup::Gsttt {
        skills.push("gsttt:gsttt".to_owned());
    }
    Init {
        tools,
        mcp_servers: servers,
        plugins,
        slash_commands: skills.clone(),
        skills,
        agents: names(&["Explore", "Plan"]),
        model: "claude-test-1".to_owned(),
        claude_code_version: "9.9.9".to_owned(),
        session_id: format!("session-{setup}"),
    }
}

/// The line Claude Code writes for the init event `init` describes.
#[cfg(test)]
pub(super) fn event_of(init: &Init) -> String {
    let servers: Vec<Value> = init
        .mcp_servers
        .iter()
        .map(|server| serde_json::json!({ "name": server.name, "status": server.status }))
        .collect();
    let plugins: Vec<Value> = init
        .plugins
        .iter()
        .map(|plugin| serde_json::json!({ "name": plugin.name, "source": plugin.source }))
        .collect();
    serde_json::json!({
        "type": "system",
        "subtype": "init",
        "tools": init.tools,
        "mcp_servers": servers,
        "plugins": plugins,
        "skills": init.skills,
        "slash_commands": init.slash_commands,
        "agents": init.agents,
        "model": init.model,
        "claude_code_version": init.claude_code_version,
        "session_id": init.session_id,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(setup: Setup, init: &Init) -> String {
        check(setup, init, &UserEntries::default())
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn every_setup_accepts_what_it_is_given() {
        for setup in Setup::ALL {
            check(setup, &expected(setup), &UserEntries::default()).unwrap();
        }
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A run that loaded something else stops the task"
    )]
    #[test]
    fn a_run_that_loaded_something_else_stops_the_task() {
        let mut init = expected(Setup::Plain);
        init.mcp_servers.push(McpServer {
            name: "telegram".to_owned(),
            status: "connected".to_owned(),
        });
        let error = refused(Setup::Plain, &init);
        assert!(
            error.contains("`plain`") && error.contains("`telegram`"),
            "{error}"
        );

        let mut init = expected(Setup::Plain);
        init.tools.push("LSP".to_owned());
        let error = refused(Setup::Plain, &init);
        assert!(
            error.contains("`plain`") && error.contains("`LSP`"),
            "{error}"
        );

        let mut init = expected(Setup::Lsp);
        init.tools
            .push("mcp__plugin_gsttt_gsttt__impact".to_owned());
        assert!(refused(Setup::Lsp, &init).contains("mcp__plugin_gsttt_gsttt__impact"));

        let mut init = expected(Setup::Cbm);
        init.plugins.push(Plugin {
            name: "superpowers".to_owned(),
            source: "superpowers@claude-plugins-official".to_owned(),
        });
        let error = refused(Setup::Cbm, &init);
        assert!(
            error.contains("`cbm`") && error.contains("superpowers"),
            "{error}"
        );
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "A plugin from another source stops the task")]
    #[test]
    fn a_plugin_of_the_right_name_from_another_source_stops_the_task() {
        for setup in [Setup::Lsp, Setup::Gsttt] {
            let name = setup.plugin().unwrap();
            let other = format!("{name}@claude-plugins-official");
            let mut init = expected(setup);
            for plugin in &mut init.plugins {
                if plugin.name == name {
                    plugin.source.clone_from(&other);
                }
            }
            let error = refused(setup, &init);
            assert!(
                error.contains(&format!("`{setup}`")) && error.contains(&other),
                "{error}"
            );
        }

        // The plugin given with `--plugin-dir` and one of the name from elsewhere together are refused too.
        let mut init = expected(Setup::Gsttt);
        init.plugins.push(Plugin {
            name: "gsttt".to_owned(),
            source: "gsttt@claude-plugins-official".to_owned(),
        });
        assert!(refused(Setup::Gsttt, &init).contains("gsttt@claude-plugins-official"));
    }

    #[test]
    fn the_configuration_directory_is_where_the_host_says_or_dot_claude_in_the_home() {
        let host = |name: &str| -> Option<OsString> {
            match name {
                "CLAUDE_CONFIG_DIR" => Some("/elsewhere/claude".into()),
                "HOME" => Some("/home/quincy".into()),
                _ => None,
            }
        };
        assert_eq!(config_dir(&host), Some(PathBuf::from("/elsewhere/claude")));
        let home_only = |name: &str| (name == "HOME").then(|| OsString::from("/home/quincy"));
        assert_eq!(
            config_dir(&home_only),
            Some(PathBuf::from("/home/quincy/.claude"))
        );
        assert_eq!(config_dir(&|_| None), None);
    }

    #[test]
    fn a_skill_of_a_plugin_or_of_the_user_is_refused() {
        let mut init = expected(Setup::Plain);
        init.skills.push("gsttt:gsttt".to_owned());
        assert!(refused(Setup::Plain, &init).contains("`gsttt:gsttt`"));

        let user = UserEntries {
            skills: BTreeSet::from(["notes-workspace".to_owned()]),
            agents: BTreeSet::from(["reviewer".to_owned()]),
        };
        let mut init = expected(Setup::Gsttt);
        init.skills.push("notes-workspace".to_owned());
        let error = check(Setup::Gsttt, &init, &user).unwrap_err().to_string();
        assert!(
            error.contains("notes-workspace") && error.contains("the user"),
            "{error}"
        );

        // An agent of the user is refused as an agent, and does not make a skill of that name one.
        let mut init = expected(Setup::Gsttt);
        init.agents.push("reviewer".to_owned());
        assert!(check(Setup::Gsttt, &init, &user).is_err());
        let mut init = expected(Setup::Gsttt);
        init.skills.push("reviewer".to_owned());
        check(Setup::Gsttt, &init, &user).unwrap();
    }

    #[test]
    fn a_setup_that_lacks_its_own_is_refused() {
        let mut init = expected(Setup::Gsttt);
        init.mcp_servers.clear();
        assert!(refused(Setup::Gsttt, &init).contains("did not load its MCP server"));

        let mut init = expected(Setup::Lsp);
        init.tools.retain(|tool| tool != "LSP");
        assert!(refused(Setup::Lsp, &init).contains("did not load the tool `LSP`"));

        let mut init = expected(Setup::Cbm);
        init.mcp_servers[0].status = "failed".to_owned();
        assert!(refused(Setup::Cbm, &init).contains("not connected"));
    }

    #[test]
    fn the_init_event_is_read_from_the_stream() {
        let line = r#"{"type":"system","subtype":"init","tools":["Bash"],"mcp_servers":[{"name":"a","status":"connected","source":"dynamic"}],"plugins":[{"name":"p","path":"x","source":"p@inline"}],"skills":["s"],"slash_commands":["s"],"agents":["Plan"],"model":"claude-test-1","claude_code_version":"9.9.9","session_id":"7d1f0f0e-3c2b-4a55-8c1e-2f6a9b0c4d11","cwd":"/x"}"#;
        let event: Value = serde_json::from_str(line).unwrap();
        let init = from_event(&event).unwrap().unwrap();
        assert_eq!(
            (
                init.model.as_str(),
                init.tools.len(),
                init.session_id.as_str()
            ),
            ("claude-test-1", 1, "7d1f0f0e-3c2b-4a55-8c1e-2f6a9b0c4d11")
        );
        let other: Value = serde_json::from_str(r#"{"type":"assistant"}"#).unwrap();
        assert!(from_event(&other).unwrap().is_none());
        let broken: Value = serde_json::from_str(r#"{"type":"system","subtype":"init"}"#).unwrap();
        assert!(from_event(&broken).is_err());
    }

    #[test]
    fn the_user_entries_are_the_names_in_the_config_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("skills/backend-practices")).unwrap();
        fs::create_dir_all(dir.path().join("commands")).unwrap();
        fs::write(dir.path().join("commands/review.md"), "").unwrap();
        fs::create_dir_all(dir.path().join("agents")).unwrap();
        fs::write(dir.path().join("agents/architect.md"), "").unwrap();
        assert_eq!(
            user_entries(dir.path()),
            UserEntries {
                skills: BTreeSet::from(["backend-practices".to_owned(), "review".to_owned()]),
                agents: BTreeSet::from(["architect".to_owned()]),
            }
        );
        assert_eq!(
            user_entries(&dir.path().join("missing")),
            UserEntries::default()
        );
    }
}
