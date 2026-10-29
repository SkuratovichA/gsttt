//! The four setups of the agent benchmarks, and what a run of each is started with: its command line, its PATH and its environment, and the files it is given.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::checkout::with_canonical_names;
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

    /// Where Claude Code says the plugin of the setup came from in its init event: a plugin given with `--plugin-dir` is `<name>@inline`, and one of the same name from a marketplace is another plugin.
    pub(super) fn plugin_source(self) -> Option<String> {
        self.plugin().map(|name| format!("{name}@inline"))
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
    /// The directory the files are in.
    pub(super) dir: PathBuf,
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
        dir: work.to_path_buf(),
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
    fresh_dir(dir)?;
    let copy = dir.join("gsttt");
    fs::copy(binary, &copy)
        .with_context(|| format!("cannot copy {} to {}", binary.display(), copy.display()))?;
    Ok(copy)
}

/// Links `node` alone into `dir`, called `node`. The PATH of every run takes `node` from `dir` and not from the directory `node` was found in, which holds every program installed beside it (`npm`, `pnpm`, and on some machines a `claude`). Returns the link.
pub(super) fn place_node(node: &Path, dir: &Path) -> anyhow::Result<PathBuf> {
    let program =
        fs::canonicalize(node).with_context(|| format!("cannot resolve {}", node.display()))?;
    fresh_dir(dir)?;
    let link = dir.join("node");
    std::os::unix::fs::symlink(&program, &link)
        .with_context(|| format!("cannot link {} to {}", link.display(), program.display()))?;
    Ok(link)
}

/// Makes `dir` a directory with nothing in it: what it held is removed.
pub(super) fn fresh_dir(dir: &Path) -> anyhow::Result<()> {
    if dir.exists() {
        fs::remove_dir_all(dir).with_context(|| format!("cannot clear {}", dir.display()))?;
    }
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))
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
    /// The directory that holds a link to `node` and nothing else, which the language server needs.
    pub(super) node: &'a Path,
    /// The directory that holds [`bin`](Self::bin) and the directory of [`files`](Self::files), and nothing else: the runs that must not find the binary under test are denied it.
    pub(super) task: &'a Path,
    /// The directories that no run may read: this repository, and the configuration directory of Claude Code.
    pub(super) private: &'a [PathBuf],
}

/// The directories one run is made in.
#[derive(Clone, Copy, Debug)]
pub(super) struct RunDirs<'a> {
    /// The checkout the run is made in.
    pub(super) checkout: &'a Path,
    /// The temporary directory the run is given.
    pub(super) tmp: &'a Path,
}

/// How a run of one setup is started: the arguments of `claude`, and its whole environment, PATH included.
#[derive(Debug)]
pub(super) struct Invocation {
    pub(super) args: Vec<OsString>,
    pub(super) env: Vec<(String, OsString)>,
}

/// The invocation of `setup`. `host` answers for a variable of the environment of this task.
///
/// Every run gets the same flags: the pinned model; the stream as JSON; no session kept; no permission asked; only the settings of the project, so that no settings, hooks, memory or CLAUDE.md of the user are read; only the tools of the setup; the limit of turns; and the sandbox and the Read rules of [`settings`]. The MCP servers of the machine are dropped by a strict config in every setup but `gsttt`, whose server comes from its plugin and would be dropped with them.
pub(super) fn invocation(
    setup: Setup,
    ctx: &Context,
    run: RunDirs,
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
    // The run that has the binary under test starts it from the directory beside the files of the setups, so it is denied the files and not the directory; every other run is denied both.
    let hidden = if setup == Setup::Gsttt {
        ctx.files.dir.as_path()
    } else {
        ctx.task
    };
    let denied: Vec<&Path> = ctx
        .private
        .iter()
        .map(PathBuf::as_path)
        .chain([hidden])
        .collect();
    let settings = settings(&[run.checkout, run.tmp], &denied)?;
    args.extend(["--settings".into(), settings.to_string().into()]);
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
    ensure_out_of_reach(setup, &path)?;
    let env = environment(host, &path, run.tmp)?;
    Ok(Invocation { args, env })
}

/// The settings of a run, as the JSON that `--settings` takes: the sandbox of the Bash tool, which may write in `writable` and read nothing in `denied`, and the rules that keep the Read tool out of `denied` too, since the sandbox confines the commands of the Bash tool and not the tools of Claude Code itself. A run whose sandbox is not available does not start, instead of going on without it.
fn settings(writable: &[&Path], denied: &[&Path]) -> anyhow::Result<Value> {
    let writable = names(writable)?;
    let denied = names(denied)?;
    // Two slashes at the start make the path of a rule absolute; one makes it relative to the project.
    let rules: Vec<String> = denied
        .iter()
        .map(|dir| format!("Read(/{dir}/**)"))
        .collect();
    Ok(json!({
        "sandbox": {
            "enabled": true,
            "failIfUnavailable": true,
            "allowUnsandboxedCommands": false,
            "autoAllowBashIfSandboxed": true,
            "filesystem": { "allowWrite": writable, "denyRead": denied },
        },
        "permissions": { "deny": rules },
    }))
}

/// `dirs` by every name a process may open them by: the one given and the canonical one, since the temporary directory of macOS is behind a link and the sandbox matches the path that is opened. A path that is not absolute is an error: a rule of the Read tool would take it for one relative to the project.
fn names(dirs: &[&Path]) -> anyhow::Result<Vec<String>> {
    for dir in dirs {
        ensure!(
            dir.is_absolute(),
            "{} is not an absolute path",
            dir.display()
        );
    }
    let mut names: Vec<String> = Vec::new();
    for name in with_canonical_names(dirs.iter().map(|&dir| dir.to_path_buf())) {
        let name = name.to_string_lossy().into_owned();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    Ok(names)
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

/// The programs a run must not find on its PATH: the ones that start another Claude Code or one of the tools under test, which a run reaches through its own configuration or not at all. `gsttt` is out of reach of every setup but the one given the binary under test.
const OUT_OF_REACH: [&str; 5] = [
    "gsttt",
    "claude",
    "codebase-memory-mcp",
    "typescript-language-server",
    "tsc",
];

/// Fails when a directory of the PATH of a run holds a program the run must not find.
fn ensure_out_of_reach(setup: Setup, path: &[PathBuf]) -> anyhow::Result<()> {
    let hidden = OUT_OF_REACH
        .iter()
        .filter(|&&name| name != "gsttt" || setup != Setup::Gsttt);
    for name in hidden {
        for dir in path {
            ensure!(
                !dir.join(name).exists(),
                "{} holds a program called {name}, which a `{setup}` run would find on its PATH",
                dir.display()
            );
        }
    }
    Ok(())
}

/// The environment of a run: what [`KEEP`] names from the environment of this task, the PATH, the temporary directory of the run, and the variables that switch off memory, the connectors of claude.ai and the updater of Claude Code.
///
/// `TMPDIR` is where the process of Claude Code puts its compile cache and `CLAUDE_CODE_TMPDIR` where it puts its temporary directory and its sockets. Neither moves the `TMPDIR` of the Bash tool, which is `/tmp/claude-<uid>` whatever they say (measured, see the design).
fn environment(
    host: &dyn Fn(&str) -> Option<OsString>,
    path: &[PathBuf],
    tmp: &Path,
) -> anyhow::Result<Vec<(String, OsString)>> {
    let mut env: Vec<(String, OsString)> = KEEP
        .iter()
        .filter_map(|&name| host(name).map(|value| (name.to_owned(), value)))
        .collect();
    env.push((
        "PATH".to_owned(),
        std::env::join_paths(path).context("a directory of the PATH of a run holds a colon")?,
    ));
    env.push(("TMPDIR".to_owned(), tmp.as_os_str().to_owned()));
    env.push(("CLAUDE_CODE_TMPDIR".to_owned(), tmp.as_os_str().to_owned()));
    env.push(("CLAUDE_CODE_DISABLE_AUTO_MEMORY".to_owned(), "1".into()));
    env.push(("DISABLE_AUTOUPDATER".to_owned(), "1".into()));
    env.push(("ENABLE_CLAUDEAI_MCP_SERVERS".to_owned(), "false".into()));
    Ok(env)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    /// A layout of the files a task has, under `dir`.
    struct Layout {
        programs: Programs,
        files: Files,
        bin: PathBuf,
        node: PathBuf,
        plugin: PathBuf,
        task: PathBuf,
        /// The repository and the configuration directory of Claude Code.
        private: Vec<PathBuf>,
        checkout: PathBuf,
        tmp: PathBuf,
    }

    fn layout(dir: &Path) -> Layout {
        let task = dir.join("task");
        let bin = task.join(".bin");
        let node = dir.join("node-bin");
        let plugin = dir.join("plugin");
        let private = vec![dir.join("repository"), dir.join("config")];
        let checkout = dir.join("checkout");
        let tmp = dir.join("tmp");
        for made in [&plugin, &checkout, &tmp].into_iter().chain(&private) {
            fs::create_dir_all(made).unwrap();
        }
        fs::write(dir.join("installed-node"), "node").unwrap();
        place_node(&dir.join("installed-node"), &node).unwrap();
        let copy = place_binary(&dir.join("built-gsttt"), &bin);
        assert!(copy.is_err(), "there is no binary to copy yet");
        fs::write(dir.join("built-gsttt"), "binary").unwrap();
        let copy = place_binary(&dir.join("built-gsttt"), &bin).unwrap();
        let programs = Programs::new(&dir.join("tools"), &copy);
        let files = write_files(&task.join(".setups"), &programs).unwrap();
        Layout {
            programs,
            files,
            bin,
            node,
            plugin,
            task,
            private,
            checkout,
            tmp,
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

    fn invoke_with(
        layout: &Layout,
        setup: Setup,
        host: &dyn Fn(&str) -> Option<OsString>,
    ) -> anyhow::Result<Invocation> {
        let ctx = Context {
            model: "sonnet",
            turns: 80,
            plugin: &layout.plugin,
            files: &layout.files,
            bin: &layout.bin,
            node: &layout.node,
            task: &layout.task,
            private: &layout.private,
        };
        let run = RunDirs {
            checkout: &layout.checkout,
            tmp: &layout.tmp,
        };
        invocation(setup, &ctx, run, host)
    }

    fn invoke(layout: &Layout, setup: Setup) -> anyhow::Result<Invocation> {
        invoke_with(layout, setup, &|_| None)
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

    /// The strings of an array in the settings.
    fn strings(value: &Value) -> BTreeSet<String> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap().to_owned())
            .collect()
    }

    /// `dir` by the name it has and by its canonical name.
    fn forms(dir: &Path) -> Vec<String> {
        [dir.to_path_buf(), fs::canonicalize(dir).unwrap()]
            .iter()
            .map(|name| name.to_string_lossy().into_owned())
            .collect()
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
                "--settings",
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

    #[gsttt_scenario::scenario("agent-benchmarks", "A run is confined by the sandbox")]
    #[test]
    fn a_run_is_confined_by_the_sandbox() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        for setup in Setup::ALL {
            let args = args(&invoke(&layout, setup).unwrap());
            let settings: Value =
                serde_json::from_str(after(&args, "--settings").unwrap()).unwrap();
            let sandbox = &settings["sandbox"];
            assert_eq!(sandbox["enabled"], true, "{setup}");
            assert_eq!(sandbox["failIfUnavailable"], true, "{setup}");
            assert_eq!(sandbox["allowUnsandboxedCommands"], false, "{setup}");
            assert_eq!(sandbox["autoAllowBashIfSandboxed"], true, "{setup}");

            // Writes: the checkout and the temporary directory of the run, by both names of each, and nothing else.
            let writable: BTreeSet<String> = [&layout.checkout, &layout.tmp]
                .into_iter()
                .flat_map(|dir| forms(dir))
                .collect();
            assert_eq!(
                strings(&sandbox["filesystem"]["allowWrite"]),
                writable,
                "{setup}"
            );

            // Reads: the repository, the configuration directory of Claude Code and the files of the task, for the Bash tool and for the Read tool alike. The run that has the binary under test starts it from its directory, so it is denied the files of the setups beside it and not the directory.
            let hidden = if setup == Setup::Gsttt {
                &layout.files.dir
            } else {
                &layout.task
            };
            let denied: BTreeSet<String> = layout
                .private
                .iter()
                .chain([hidden])
                .flat_map(|dir| forms(dir))
                .collect();
            let rules: BTreeSet<String> = denied
                .iter()
                .map(|name| format!("Read(/{name}/**)"))
                .collect();
            assert_eq!(
                strings(&sandbox["filesystem"]["denyRead"]),
                denied,
                "{setup}"
            );
            assert_eq!(strings(&settings["permissions"]["deny"]), rules, "{setup}");
            assert!(
                rules.iter().all(|rule| rule.starts_with("Read(//")),
                "a rule with one slash is relative to the project: {rules:?}"
            );
            let bin = forms(&layout.bin);
            assert!(
                !denied.iter().any(|name| bin.contains(name)),
                "no run is denied the binary under test itself: {setup}"
            );
            let task = forms(&layout.task);
            assert_eq!(
                denied.iter().any(|name| task.contains(name)),
                setup != Setup::Gsttt,
                "only the gsttt run may read in the directory of the binary: {setup}"
            );
        }

        // A path a rule would take for one relative to the project is refused.
        assert!(settings(&[], &[Path::new("relative/dir")]).is_err());
        assert!(settings(&[Path::new("relative/dir")], &[]).is_err());
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
        for setup in [Setup::Plain, Setup::Lsp, Setup::Cbm] {
            let error = invoke(&layout, setup).unwrap_err().to_string();
            assert!(
                error.contains(&format!("`{setup}`")) && error.contains("called gsttt"),
                "{error}"
            );
        }
        invoke(&layout, Setup::Gsttt).unwrap();
        fs::remove_file(layout.node.join("gsttt")).unwrap();
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "Claude Code and the tools are out of reach of every setup"
    )]
    #[test]
    fn claude_code_and_the_tools_are_out_of_reach_of_every_setup() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        for setup in Setup::ALL {
            invoke(&layout, setup).unwrap();
        }
        // The programs that start another Claude Code or one of the tools under test, the gsttt setup included.
        for name in OUT_OF_REACH.into_iter().filter(|&name| name != "gsttt") {
            fs::write(layout.node.join(name), "installed").unwrap();
            for setup in Setup::ALL {
                let error = invoke(&layout, setup).unwrap_err().to_string();
                assert!(
                    error.contains(&format!("`{setup}`"))
                        && error.contains(&format!("called {name}")),
                    "{error}"
                );
            }
            fs::remove_file(layout.node.join(name)).unwrap();
        }
        // The names the task refuses are these and no others.
        assert_eq!(
            OUT_OF_REACH,
            [
                "gsttt",
                "claude",
                "codebase-memory-mcp",
                "typescript-language-server",
                "tsc"
            ]
        );
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "The directory of node holds node alone")]
    #[test]
    fn the_directory_of_node_holds_node_alone() {
        let dir = tempfile::tempdir().unwrap();
        // The directory `node` was found in holds the programs installed beside it.
        let installed = dir.path().join("installed");
        fs::create_dir_all(&installed).unwrap();
        for name in ["node", "npm", "pnpm", "claude"] {
            fs::write(installed.join(name), name).unwrap();
        }
        let only = dir.path().join("only-node");
        let link = place_node(&installed.join("node"), &only).unwrap();
        // A second call replaces what the first made.
        place_node(&installed.join("node"), &only).unwrap();
        let names: Vec<_> = fs::read_dir(&only)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["node"]);
        assert_eq!(fs::read_to_string(&link).unwrap(), "node");

        // A run takes `node` from a directory like that one and from no other.
        let layout = layout(dir.path());
        for setup in Setup::ALL {
            let path = path_of(&invoke(&layout, setup).unwrap());
            assert!(path.contains(&layout.node), "{setup}");
            assert!(!path.contains(&installed), "{setup}");
        }
        let names: Vec<_> = fs::read_dir(&layout.node)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["node"]);
    }

    #[test]
    fn a_run_inherits_none_of_the_session_it_was_started_from() {
        let dir = tempfile::tempdir().unwrap();
        let layout = layout(dir.path());
        let host = |name: &str| -> Option<OsString> {
            match name {
                "HOME" => Some("/home/quincy".into()),
                "TMPDIR" => Some("/var/folders/quincy/T".into()),
                "CLAUDE_CONFIG_DIR" => Some("/home/quincy/.claude-other".into()),
                "CLAUDE_CODE_ENTRYPOINT" | "ANTHROPIC_MODEL" | "CLAUDECODE" => Some("x".into()),
                _ => None,
            }
        };
        let env: BTreeMap<String, OsString> = invoke_with(&layout, Setup::Plain, &host)
            .unwrap()
            .env
            .into_iter()
            .collect();
        let names: Vec<&str> = env.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            [
                "CLAUDE_CODE_DISABLE_AUTO_MEMORY",
                "CLAUDE_CODE_TMPDIR",
                "CLAUDE_CONFIG_DIR",
                "DISABLE_AUTOUPDATER",
                "ENABLE_CLAUDEAI_MCP_SERVERS",
                "HOME",
                "PATH",
                "TMPDIR"
            ]
        );
        assert_eq!(env["ENABLE_CLAUDEAI_MCP_SERVERS"], "false");
        assert_eq!(env["CLAUDE_CODE_DISABLE_AUTO_MEMORY"], "1");
        assert_eq!(env["DISABLE_AUTOUPDATER"], "1");
        // The temporary directory is the one of the run and not the one of the session.
        assert_eq!(env["TMPDIR"], layout.tmp.as_os_str());
        assert_eq!(env["CLAUDE_CODE_TMPDIR"], layout.tmp.as_os_str());
        let path: Vec<PathBuf> = std::env::split_paths(&env["PATH"]).collect();
        assert_eq!(path[0], layout.node);
        assert_eq!(path.len(), 5);
    }
}
