//! The four setups of the agent benchmarks, and what a run of each is started with: its command line, its PATH and its environment, and the files it is given.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::bench::pins::Tool;
use crate::bench::plan::Programs;

/// The tools every setup has.
const BASE_TOOLS: [&str; 4] = ["Bash", "Read", "ToolSearch", "Skill"];

/// Where a run finds `git`, `grep` and the like. The PATH of the machine is never used: `~/.local/bin` holds both `claude` and an installed gsttt.
const SYSTEM_PATH: [&str; 4] = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"];

/// The variables of the environment of this task that a run keeps: where its login and its configuration are, how it reaches the network, and the language of the terminal. Nothing else is inherited, so that the variables of a session this task runs inside (`CLAUDE_CODE_*`, `ANTHROPIC_*`) cannot change what a run loads.
const KEEP: &[&str] = &[
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "TMPDIR",
    "LANG",
    "LC_ALL",
    "TERM",
    "CLAUDE_CONFIG_DIR",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "all_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
];

/// What Claude Code is given to find out about an agent's work: the same four tools in every setup, and one thing more in three of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Setup {
    /// Nothing more.
    Plain,
    /// The `LSP` tool, backed by typescript-language-server.
    Lsp,
    /// The tools of codebase-memory-mcp over its index of the checkout.
    Cbm,
    /// The plugin of this repository, running the binary under test over its index of the checkout.
    Gsttt,
}

impl Setup {
    /// Every setup, in the order a question is asked in.
    pub(super) const ALL: [Self; 4] = [Self::Plain, Self::Lsp, Self::Cbm, Self::Gsttt];

    /// The word the results and the table use.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Lsp => "lsp",
            Self::Cbm => "cbm",
            Self::Gsttt => "gsttt",
        }
    }

    /// The built-in tools of the setup, which `--tools` lists.
    pub(super) fn tools(self) -> Vec<&'static str> {
        let mut tools = BASE_TOOLS.to_vec();
        if self == Self::Lsp {
            tools.push("LSP");
        }
        tools
    }

    /// What the names of the tools of the MCP server of the setup start with.
    pub(super) fn mcp_prefix(self) -> Option<&'static str> {
        match self {
            Self::Cbm => Some("mcp__codebase-memory-mcp__"),
            Self::Gsttt => Some("mcp__plugin_gsttt_gsttt__"),
            Self::Plain | Self::Lsp => None,
        }
    }

    /// The name of the MCP server of the setup, as the init event gives it.
    pub(super) fn mcp_server(self) -> Option<&'static str> {
        match self {
            Self::Cbm => Some("codebase-memory-mcp"),
            Self::Gsttt => Some("plugin:gsttt:gsttt"),
            Self::Plain | Self::Lsp => None,
        }
    }

    /// The plugin the setup is given with `--plugin-dir`.
    pub(super) fn plugin(self) -> Option<&'static str> {
        match self {
            Self::Lsp => Some("gsttt-bench-lsp"),
            Self::Gsttt => Some("gsttt"),
            Self::Plain | Self::Cbm => None,
        }
    }
}

impl fmt::Display for Setup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The files the setups are started with, written once per task.
#[derive(Debug)]
pub(super) struct Files {
    /// An MCP config with no server, for the setups that have none of their own.
    pub(super) no_servers: PathBuf,
    /// An MCP config with codebase-memory-mcp alone.
    pub(super) cbm: PathBuf,
    /// The plugin that gives the `lsp` setup its language server.
    pub(super) lsp_plugin: PathBuf,
}

/// Writes the files of the setups under `work`: the MCP configs and the manifest of the plugin of the language server.
pub(super) fn write_files(work: &Path, programs: &Programs) -> anyhow::Result<Files> {
    let files = Files {
        no_servers: work.join("mcp-empty.json"),
        cbm: work.join("mcp-cbm.json"),
        lsp_plugin: work.join("lsp-plugin"),
    };
    write_json(&files.no_servers, &json!({ "mcpServers": {} }))?;
    write_json(&files.cbm, &cbm_config(programs))?;
    write_json(
        &files.lsp_plugin.join(".claude-plugin/plugin.json"),
        &lsp_manifest(programs),
    )?;
    Ok(files)
}

fn write_json(file: &Path, value: &Value) -> anyhow::Result<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    fs::write(file, text).with_context(|| format!("cannot write {}", file.display()))
}

/// The server of codebase-memory-mcp with the home and the cache its index was made with, so that it finds the index and writes nothing into the home of the user.
fn cbm_config(programs: &Programs) -> Value {
    json!({
        "mcpServers": {
            "codebase-memory-mcp": {
                "type": "stdio",
                "command": programs.of(Tool::CodebaseMemory).to_string_lossy(),
                "args": [],
                "env": {
                    "HOME": programs.cbm_home().to_string_lossy(),
                    "CBM_CACHE_DIR": programs.cbm_cache().to_string_lossy(),
                },
            }
        }
    })
}

/// The manifest of a plugin that has only a language server: the pinned typescript-language-server, running on the pinned TypeScript.
fn lsp_manifest(programs: &Programs) -> Value {
    let server = programs
        .node
        .join("node_modules/.bin/typescript-language-server");
    let tsserver = programs
        .node
        .join("node_modules/typescript/lib/tsserver.js");
    json!({
        "name": "gsttt-bench-lsp",
        "description": "typescript-language-server for the lsp setup of the agent benchmarks",
        "version": "1.0.0",
        "lspServers": {
            "typescript": {
                "command": server.to_string_lossy(),
                "args": ["--stdio"],
                "extensionToLanguage": {
                    ".ts": "typescript",
                    ".tsx": "typescriptreact",
                    ".js": "javascript",
                    ".jsx": "javascriptreact",
                    ".mts": "typescript",
                    ".cts": "typescript",
                    ".mjs": "javascript",
                    ".cjs": "javascript",
                },
                "initializationOptions": { "tsserver": { "path": tsserver.to_string_lossy() } },
            }
        }
    })
}

/// Copies the binary under test into `dir`, alone and called `gsttt`. A `gsttt` run puts `dir` first on its PATH, and the same copy indexes the checkouts, so the index is the one of the binary under test. Returns the path of the copy.
pub(super) fn place_binary(binary: &Path, dir: &Path) -> anyhow::Result<PathBuf> {
    if dir.exists() {
        fs::remove_dir_all(dir).with_context(|| format!("cannot clear {}", dir.display()))?;
    }
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let copy = dir.join("gsttt");
    fs::copy(binary, &copy)
        .with_context(|| format!("cannot copy {} to {}", binary.display(), copy.display()))?;
    Ok(copy)
}

/// What the command line of a run depends on besides its setup.
#[derive(Debug)]
pub(super) struct Context<'a> {
    /// The model alias the pins give.
    pub(super) model: &'a str,
    /// The limit of turns of a run.
    pub(super) turns: u32,
    /// The `plugin/` directory of this repository.
    pub(super) plugin: &'a Path,
    pub(super) files: &'a Files,
    /// The directory that holds the binary under test and nothing else.
    pub(super) bin: &'a Path,
    /// The directory of `node`, which the language server needs.
    pub(super) node: &'a Path,
}

/// How a run of one setup is started: the arguments of `claude`, and its whole environment, PATH included.
#[derive(Debug)]
pub(super) struct Invocation {
    pub(super) args: Vec<OsString>,
    pub(super) env: Vec<(String, OsString)>,
}

/// The invocation of `setup`. `host` answers for a variable of the environment of this task.
///
/// Every run gets the same flags: the pinned model; the stream as JSON; no session kept; no permission asked; only the settings of the project, so that no settings, hooks, memory or CLAUDE.md of the user are read; only the tools of the setup; and the limit of turns. The MCP servers of the machine are dropped by a strict config in every setup but `gsttt`, whose server comes from its plugin and would be dropped with them.
pub(super) fn invocation(
    setup: Setup,
    ctx: &Context,
    host: &dyn Fn(&str) -> Option<OsString>,
) -> anyhow::Result<Invocation> {
    let mut args: Vec<OsString> = [
        "-p",
        "--model",
        ctx.model,
        "--output-format",
        "stream-json",
        "--verbose",
        "--no-session-persistence",
        "--permission-mode",
        "bypassPermissions",
        "--setting-sources",
        "project",
        "--tools",
    ]
    .map(OsString::from)
    .to_vec();
    args.push(setup.tools().join(",").into());
    args.extend(["--max-turns".into(), ctx.turns.to_string().into()]);
    match setup {
        Setup::Plain | Setup::Lsp => args.extend(strict(&ctx.files.no_servers)),
        Setup::Cbm => args.extend(strict(&ctx.files.cbm)),
        Setup::Gsttt => {}
    }
    match setup {
        Setup::Lsp => args.extend(plugin_dir(&ctx.files.lsp_plugin)),
        Setup::Gsttt => args.extend(plugin_dir(ctx.plugin)),
        Setup::Plain | Setup::Cbm => {}
    }

    let mut path = Vec::new();
    if setup == Setup::Gsttt {
        path.push(ctx.bin.to_path_buf());
    }
    path.push(ctx.node.to_path_buf());
    path.extend(SYSTEM_PATH.map(PathBuf::from));
    if setup != Setup::Gsttt {
        ensure_out_of_reach(setup, &path)?;
    }
    let env = environment(host, &path)?;
    Ok(Invocation { args, env })
}

fn strict(config: &Path) -> [OsString; 3] {
    [
        "--strict-mcp-config".into(),
        "--mcp-config".into(),
        config.as_os_str().to_owned(),
    ]
}

fn plugin_dir(dir: &Path) -> [OsString; 2] {
    ["--plugin-dir".into(), dir.as_os_str().to_owned()]
}

/// Fails when a directory of the PATH of a run that must not have gsttt holds a program called `gsttt`.
fn ensure_out_of_reach(setup: Setup, path: &[PathBuf]) -> anyhow::Result<()> {
    for dir in path {
        ensure!(
            !dir.join("gsttt").exists(),
            "{} holds a program called gsttt, which a `{setup}` run would find on its PATH",
            dir.display()
        );
    }
    Ok(())
}

/// The environment of a run: what [`KEEP`] names from the environment of this task, the PATH, and the two variables that switch off memory and the connectors of claude.ai.
fn environment(
    host: &dyn Fn(&str) -> Option<OsString>,
    path: &[PathBuf],
) -> anyhow::Result<Vec<(String, OsString)>> {
    let mut env: Vec<(String, OsString)> = KEEP
        .iter()
        .filter_map(|&name| host(name).map(|value| (name.to_owned(), value)))
        .collect();
    env.push((
        "PATH".to_owned(),
        std::env::join_paths(path).context("a directory of the PATH of a run holds a colon")?,
    ));
    env.push(("CLAUDE_CODE_DISABLE_AUTO_MEMORY".to_owned(), "1".into()));
    env.push(("ENABLE_CLAUDEAI_MCP_SERVERS".to_owned(), "false".into()));
    Ok(env)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// A layout of the files a task has, under `dir`.
    struct Layout {
        programs: Programs,
        files: Files,
        bin: PathBuf,
        node: PathBuf,
        plugin: PathBuf,
    }

    fn layout(dir: &Path) -> Layout {
        let bin = dir.join("bin");
        let node = dir.join("node-bin");
        let plugin = dir.join("plugin");
        fs::create_dir_all(&node).unwrap();
        fs::create_dir_all(&plugin).unwrap();
        let copy = place_binary(&dir.join("built-gsttt"), &bin);
        assert!(copy.is_err(), "there is no binary to copy yet");
        fs::write(dir.join("built-gsttt"), "binary").unwrap();
        let copy = place_binary(&dir.join("built-gsttt"), &bin).unwrap();
        let programs = Programs::new(&dir.join("tools"), &copy);
        let files = write_files(&dir.join("work"), &programs).unwrap();
        Layout {
            programs,
            files,
            bin,
            node,
            plugin,
        }
    }

    /// The directories of the PATH an invocation gives its run.
    fn path_of(invocation: &Invocation) -> Vec<PathBuf> {
        let (_, value) = invocation
            .env
            .iter()
            .find(|(name, _)| name == "PATH")
            .unwrap();
        std::env::split_paths(value).collect()
    }

    fn invoke(layout: &Layout, setup: Setup) -> anyhow::Result<Invocation> {
        let ctx = Context {
            model: "sonnet",
            turns: 80,
            plugin: &layout.plugin,
            files: &layout.files,
            bin: &layout.bin,
            node: &layout.node,
        };
        invocation(setup, &ctx, &|_| None)
    }

    fn args(invocation: &Invocation) -> Vec<String> {
        invocation
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    fn after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        let at = args.iter().position(|arg| arg == flag)?;
        args.get(at + 1).map(String::as_str)
    }

    #[test]
    fn every_run_gets_the_same_flags() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        let plain = args(&invoke(&layout, Setup::Plain).unwrap());
        assert_eq!(
            plain[..13],
            [
                "-p",
                "--model",
                "sonnet",
                "--output-format",
                "stream-json",
                "--verbose",
                "--no-session-persistence",
                "--permission-mode",
                "bypassPermissions",
                "--setting-sources",
                "project",
                "--tools",
                "Bash,Read,ToolSearch,Skill",
            ]
        );
        assert_eq!(after(&plain, "--max-turns"), Some("80"));
        for setup in Setup::ALL {
            let args = args(&invoke(&layout, setup).unwrap());
            for flag in [
                "--setting-sources",
                "--no-session-persistence",
                "--permission-mode",
            ] {
                assert!(args.iter().any(|arg| arg == flag), "{setup} lacks {flag}");
            }
            assert_eq!(after(&args, "--setting-sources"), Some("project"));
        }
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "Each setup adds only its own tool")]
    #[test]
    fn each_setup_adds_only_its_own_tool() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        let planned: BTreeMap<Setup, Vec<String>> = Setup::ALL
            .iter()
            .map(|&setup| (setup, args(&invoke(&layout, setup).unwrap())))
            .collect();

        for (setup, args) in &planned {
            let tools = after(args, "--tools").unwrap();
            for base in ["Bash", "Read", "ToolSearch", "Skill"] {
                assert!(
                    tools.split(',').any(|tool| tool == base),
                    "{setup}: {tools}"
                );
            }
            assert_eq!(
                tools.split(',').any(|tool| tool == "LSP"),
                *setup == Setup::Lsp,
                "only lsp allows the LSP tool: {setup}: {tools}"
            );
            assert_eq!(
                args.iter().any(|arg| arg == "--strict-mcp-config"),
                *setup != Setup::Gsttt,
                "a strict config would drop the server of the gsttt plugin: {setup}"
            );
            let config = after(args, "--mcp-config").map(PathBuf::from);
            let expected = match setup {
                Setup::Plain | Setup::Lsp => Some(layout.files.no_servers.clone()),
                Setup::Cbm => Some(layout.files.cbm.clone()),
                Setup::Gsttt => None,
            };
            assert_eq!(config, expected, "{setup}");
            let plugin = after(args, "--plugin-dir").map(PathBuf::from);
            let expected = match setup {
                Setup::Lsp => Some(layout.files.lsp_plugin.clone()),
                Setup::Gsttt => Some(layout.plugin.clone()),
                Setup::Plain | Setup::Cbm => None,
            };
            assert_eq!(plugin, expected, "{setup}");
        }

        // Only the servers the files name are loaded: none, none, codebase-memory-mcp.
        let empty: Value =
            serde_json::from_str(&fs::read_to_string(&layout.files.no_servers).unwrap()).unwrap();
        assert_eq!(empty["mcpServers"], json!({}));
        let cbm: Value =
            serde_json::from_str(&fs::read_to_string(&layout.files.cbm).unwrap()).unwrap();
        let server = &cbm["mcpServers"]["codebase-memory-mcp"];
        assert_eq!(cbm["mcpServers"].as_object().unwrap().len(), 1);
        assert_eq!(
            server["command"],
            layout
                .programs
                .of(Tool::CodebaseMemory)
                .to_string_lossy()
                .as_ref()
        );
        assert_eq!(
            server["env"]["HOME"],
            layout.programs.cbm_home().to_string_lossy().as_ref()
        );
        assert_eq!(
            server["env"]["CBM_CACHE_DIR"],
            layout.programs.cbm_cache().to_string_lossy().as_ref()
        );

        // The plugin of the language server has a language server and nothing else.
        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(layout.files.lsp_plugin.join(".claude-plugin/plugin.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["name"], "gsttt-bench-lsp");
        let mut keys: Vec<&String> = manifest.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(keys, ["description", "lspServers", "name", "version"]);
        let server = &manifest["lspServers"]["typescript"];
        assert!(
            server["command"]
                .as_str()
                .unwrap()
                .ends_with("node_modules/.bin/typescript-language-server")
        );
        assert!(
            server["initializationOptions"]["tsserver"]["path"]
                .as_str()
                .unwrap()
                .ends_with("node_modules/typescript/lib/tsserver.js")
        );
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "gsttt is out of reach of the other setups")]
    #[test]
    fn gsttt_is_out_of_reach_of_the_other_setups() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        for setup in [Setup::Plain, Setup::Lsp, Setup::Cbm] {
            let path = path_of(&invoke(&layout, setup).unwrap());
            assert_ne!(path, Vec::<PathBuf>::new());
            for dir in &path {
                assert!(
                    !dir.join("gsttt").exists(),
                    "{setup}: {} holds gsttt",
                    dir.display()
                );
            }
            assert!(!path.contains(&layout.bin), "{setup}");
        }

        // The gsttt setup finds the binary under test first, in a directory that holds nothing else.
        let gsttt = invoke(&layout, Setup::Gsttt).unwrap();
        assert_eq!(path_of(&gsttt).first(), Some(&layout.bin));
        let names: Vec<_> = fs::read_dir(&layout.bin)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["gsttt"]);

        // A gsttt that is already on the way to node is a leak, and the setup refuses to start.
        fs::write(layout.node.join("gsttt"), "installed").unwrap();
        let error = invoke(&layout, Setup::Plain).unwrap_err().to_string();
        assert!(
            error.contains("`plain`") && error.contains("gsttt"),
            "{error}"
        );
        invoke(&layout, Setup::Gsttt).unwrap();
    }

    #[test]
    fn a_run_inherits_none_of_the_session_it_was_started_from() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        let host = |name: &str| -> Option<OsString> {
            match name {
                "HOME" => Some("/home/quincy".into()),
                "CLAUDE_CONFIG_DIR" => Some("/home/quincy/.claude-other".into()),
                "CLAUDE_CODE_ENTRYPOINT" | "ANTHROPIC_MODEL" | "CLAUDECODE" => Some("x".into()),
                _ => None,
            }
        };
        let ctx = Context {
            model: "sonnet",
            turns: 80,
            plugin: &layout.plugin,
            files: &layout.files,
            bin: &layout.bin,
            node: &layout.node,
        };
        let env: BTreeMap<String, OsString> = invocation(Setup::Plain, &ctx, &host)
            .unwrap()
            .env
            .into_iter()
            .collect();
        let names: Vec<&str> = env.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            [
                "CLAUDE_CODE_DISABLE_AUTO_MEMORY",
                "CLAUDE_CONFIG_DIR",
                "ENABLE_CLAUDEAI_MCP_SERVERS",
                "HOME",
                "PATH"
            ]
        );
        assert_eq!(env["ENABLE_CLAUDEAI_MCP_SERVERS"], "false");
        assert_eq!(env["CLAUDE_CODE_DISABLE_AUTO_MEMORY"], "1");
        let path: Vec<PathBuf> = std::env::split_paths(&env["PATH"]).collect();
        assert_eq!(path[0], layout.node);
        assert_eq!(path.len(), 5);
    }
}
