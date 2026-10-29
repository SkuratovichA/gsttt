//! The plan of the runs: for each tool of a repository's language, the command, how its cold run is made cold and how its warm run is made warm. Nothing here touches the disk, the network or hyperfine, so the tests run on the plan itself.

use std::path::{Path, PathBuf};

use super::pins::{Language, Repository, Tool};

/// gsttt keeps its state in the `.git/gsttt` of a git repository (`crates/core/src/engine/workspace.rs`), so that is what a cold run removes.
const GSTTT_CACHE: &str = ".git/gsttt";

/// The directory the `--cache` of dependency-cruiser writes to, in the checkout.
const DEPENDENCY_CRUISER_CACHE: &str = ".depcruise-cache";

/// The directory the `cache_dir` of grimp writes to, in the checkout.
const GRIMP_CACHE: &str = ".grimp_cache";

/// Runs hyperfine makes without timing them before a warm measurement: one is enough to leave the cache in place.
const WARMUP_RUNS: u32 = 1;

/// Where the programs of the tools are on this machine: the layout of `bench/tools`, and the binary of gsttt.
#[derive(Debug)]
pub(crate) struct Programs {
    /// The directory pnpm installs the Node tools into.
    pub(crate) node: PathBuf,
    /// The virtual environment uv installs grimp into.
    pub(crate) venv: PathBuf,
    /// The directory codebase-memory-mcp is unpacked into, with its home and its cache beside the binary.
    pub(crate) cbm: PathBuf,
    gsttt: PathBuf,
}

impl Programs {
    /// The programs as the task installs them under `tools`, and `gsttt` as the task was given it.
    pub(crate) fn new(tools: &Path, gsttt: &Path) -> Self {
        Self {
            node: tools.join("node"),
            venv: tools.join("python"),
            cbm: tools.join("cbm"),
            gsttt: gsttt.to_path_buf(),
        }
    }

    /// The program of `tool`.
    pub(crate) fn of(&self, tool: Tool) -> PathBuf {
        match tool {
            Tool::Gsttt => self.gsttt.clone(),
            Tool::DependencyCruiser => self.node.join("node_modules/.bin/depcruise"),
            Tool::Madge => self.node.join("node_modules/.bin/madge"),
            Tool::Grimp => self.venv.join("bin/python"),
            Tool::CodebaseMemory => self.cbm.join("codebase-memory-mcp"),
        }
    }

    /// The `HOME` codebase-memory-mcp runs with, so that it writes nothing into the home of the user.
    pub(crate) fn cbm_home(&self) -> PathBuf {
        self.cbm.join("home")
    }

    /// The `CBM_CACHE_DIR` codebase-memory-mcp runs with.
    pub(crate) fn cbm_cache(&self) -> PathBuf {
        self.cbm.join("cache")
    }
}

/// A command, in the two forms it is needed in: as a reader types it in the checkout, and as hyperfine runs it.
#[derive(Debug)]
pub(crate) struct Command {
    pub(crate) tool: Tool,
    /// The program by its path on this machine.
    pub(crate) program: PathBuf,
    /// The arguments, relative to the checkout the command runs in.
    pub(crate) args: Vec<String>,
    /// Variables that are part of the command a reader types.
    pub(crate) env: Vec<(&'static str, &'static str)>,
}

impl Command {
    /// The command as a reader types it in the checkout: the name of the program and no path of this machine.
    pub(super) fn shown(&self) -> String {
        let env = self
            .env
            .iter()
            .map(|(key, value)| format!("{key}={}", quote(value)));
        let program = std::iter::once(self.tool.program().to_owned());
        env.chain(program)
            .chain(self.args.iter().map(|arg| quote(arg)))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The command as hyperfine runs it, with its program by its path. hyperfine runs it without a shell, so the variables of `env` are set on the process.
    pub(super) fn executed(&self) -> String {
        let program = std::iter::once(quote(&self.program.to_string_lossy()));
        program
            .chain(self.args.iter().map(|arg| quote(arg)))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// What hyperfine does around the timed runs of one measurement.
#[derive(Debug)]
pub(super) struct Measure {
    /// A shell line run in the checkout before every run.
    pub(super) prepare: Option<String>,
    /// How many runs come first without being timed.
    pub(super) warmup: u32,
}

/// One tool on one repository.
#[derive(Debug)]
pub(crate) struct Run {
    pub(crate) tool: Tool,
    pub(crate) command: Command,
    /// Every run starts without the cache of the tool.
    pub(super) cold: Measure,
    /// The runs follow one that left the cache in place.
    pub(super) warm: Measure,
    /// Variables the tool runs with that a reader does not type.
    pub(crate) isolation: Vec<(&'static str, PathBuf)>,
}

/// The runs of `repository`: one per tool of its language.
pub(crate) fn plan(repository: &Repository, programs: &Programs) -> Vec<Run> {
    repository
        .language
        .tools()
        .iter()
        .map(|&tool| Run {
            tool,
            command: command(tool, repository, programs),
            cold: Measure {
                prepare: reset(tool, programs),
                warmup: 0,
            },
            warm: Measure {
                prepare: None,
                warmup: WARMUP_RUNS,
            },
            isolation: isolation(tool, programs),
        })
        .collect()
}

/// The `--ts-config` option for the tsconfig the repository pins, and nothing when it pins none. dependency-cruiser resolves the `extends` and the `include` of a tsconfig below the root of the checkout against the wrong directory (TS5083 and TS18003 on vite), so it is given a tsconfig only from the root; madge takes one from anywhere.
fn ts_config(tool: Tool, repository: &Repository) -> Vec<String> {
    repository
        .tsconfig
        .iter()
        .filter(|file| tool != Tool::DependencyCruiser || !file.contains('/'))
        .flat_map(|file| ["--ts-config".to_owned(), file.clone()])
        .collect()
}

fn command(tool: Tool, repository: &Repository, programs: &Programs) -> Command {
    let mut env = Vec::new();
    let args = match tool {
        Tool::Gsttt => strings(&["index"]),
        Tool::DependencyCruiser => {
            let mut args = strings(&[
                ".",
                "--no-config",
                "--ts-pre-compilation-deps",
                "--output-type",
                "json",
                "--cache",
                DEPENDENCY_CRUISER_CACHE,
            ]);
            args.extend(ts_config(tool, repository));
            args
        }
        Tool::Madge => {
            let mut args = strings(&["--json", "--extensions"]);
            args.push(Language::Typescript.extensions().join(","));
            args.push(".".to_owned());
            args.extend(ts_config(tool, repository));
            args
        }
        Tool::Grimp => {
            env.push(("PYTHONPATH", "."));
            let packages = repository
                .packages
                .iter()
                .map(|package| format!("'{package}'"))
                .collect::<Vec<_>>()
                .join(", ");
            vec![
                "-c".to_owned(),
                format!("import grimp; grimp.build_graph({packages}, cache_dir='{GRIMP_CACHE}')"),
            ]
        }
        Tool::CodebaseMemory => strings(&[
            "cli",
            "--json",
            "index_repository",
            "--repo-path",
            ".",
            "--name",
            repository.name.as_str(),
            "--mode",
            "full",
        ]),
    };
    Command {
        tool,
        program: programs.of(tool),
        args,
        env,
    }
}

/// The shell line that throws the cache of `tool` away, which every cold run starts with; `None` for madge, which keeps no cache.
fn reset(tool: Tool, programs: &Programs) -> Option<String> {
    match tool {
        Tool::Gsttt => Some(remove(GSTTT_CACHE)),
        Tool::DependencyCruiser => Some(remove(DEPENDENCY_CRUISER_CACHE)),
        Tool::Madge => None,
        Tool::Grimp => Some(remove(GRIMP_CACHE)),
        Tool::CodebaseMemory => {
            let cache = quote(&programs.cbm_cache().to_string_lossy());
            Some(format!("rm -rf {cache} && mkdir -p {cache}"))
        }
    }
}

fn remove(path: &str) -> String {
    format!("rm -rf {}", quote(path))
}

fn isolation(tool: Tool, programs: &Programs) -> Vec<(&'static str, PathBuf)> {
    match tool {
        Tool::CodebaseMemory => vec![
            ("HOME", programs.cbm_home()),
            ("CBM_CACHE_DIR", programs.cbm_cache()),
        ],
        Tool::Gsttt | Tool::DependencyCruiser | Tool::Madge | Tool::Grimp => Vec::new(),
    }
}

fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

/// A word as a shell reads it back: bare when it holds only characters that mean nothing to a shell, else quoted.
fn quote(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./=:,@%+".contains(c))
    {
        word.to_owned()
    } else if word.chars().any(|c| "\"$`\\!".contains(c)) {
        format!("'{}'", word.replace('\'', r"'\''"))
    } else {
        format!("\"{word}\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository(
        name: &str,
        language: Language,
        tsconfig: Option<&str>,
        packages: &[&str],
    ) -> Repository {
        Repository {
            name: name.to_owned(),
            url: format!("https://example.test/{name}"),
            tag: "1.0.0".to_owned(),
            commit: "0".repeat(40),
            language,
            tsconfig: tsconfig.map(str::to_owned),
            packages: strings(packages),
            agents: None,
        }
    }

    /// One repository of each language, as the pins have them.
    fn repositories() -> [Repository; 3] {
        [
            repository("app", Language::Typescript, Some("tsconfig.json"), &[]),
            repository("framework", Language::Python, None, &["django"]),
            repository("tool", Language::Rust, None, &[]),
        ]
    }

    fn programs(home: &Path) -> Programs {
        Programs::new(&home.join("bench/tools"), &home.join("gsttt"))
    }

    #[test]
    fn dependency_cruiser_is_given_a_tsconfig_only_from_the_root() {
        let home = tempfile::tempdir().unwrap();
        let programs = programs(home.path());
        // dependency-cruiser first, then madge.
        let given = |tsconfig: Option<&str>| {
            let repository = repository("app", Language::Typescript, tsconfig, &[]);
            plan(&repository, &programs)
                .iter()
                .filter(|run| matches!(run.tool, Tool::DependencyCruiser | Tool::Madge))
                .map(|run| run.command.args.iter().any(|arg| arg == "--ts-config"))
                .collect::<Vec<_>>()
        };
        assert_eq!(given(Some("tsconfig.json")), [true, true]);
        assert_eq!(given(Some("packages/app/tsconfig.json")), [false, true]);
        assert_eq!(given(None), [false, false]);
    }

    #[gsttt_scenario::scenario(
        "benchmarks",
        "Each repository is measured with the tools of its language"
    )]
    #[test]
    fn each_repository_is_measured_with_the_tools_of_its_language() {
        let home = tempfile::tempdir().unwrap();
        let programs = programs(home.path());
        let [typescript, python, rust] = repositories();
        let names = |repository: &Repository| {
            plan(repository, &programs)
                .iter()
                .map(|run| run.tool.name())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(&typescript),
            [
                "gsttt",
                "dependency-cruiser",
                "madge",
                "codebase-memory-mcp"
            ]
        );
        assert_eq!(names(&python), ["gsttt", "grimp", "codebase-memory-mcp"]);
        assert_eq!(names(&rust), ["gsttt", "codebase-memory-mcp"]);

        for repository in [&typescript, &python, &rust] {
            for run in plan(repository, &programs) {
                // gsttt indexes the directory it runs in; the others are told to read `.`; grimp is told the packages.
                let whole_checkout =
                    run.tool == Tool::Gsttt || run.command.args.iter().any(|arg| arg == ".");
                assert_eq!(
                    whole_checkout,
                    run.tool != Tool::Grimp,
                    "{}",
                    run.command.shown()
                );
            }
        }
        let grimp = plan(&python, &programs).remove(1);
        assert!(
            grimp
                .command
                .args
                .iter()
                .any(|arg| arg.contains("build_graph('django',"))
        );
    }

    #[gsttt_scenario::scenario("benchmarks", "A cold run starts without the tool's cache")]
    #[test]
    fn a_cold_run_starts_without_the_tools_cache() {
        let home = tempfile::tempdir().unwrap();
        let programs = programs(home.path());
        let [typescript, python, rust] = repositories();
        let cache = programs.cbm_cache().to_string_lossy().into_owned();
        for repository in [&typescript, &python, &rust] {
            for run in plan(repository, &programs) {
                let prepare = run.cold.prepare.as_deref();
                match run.tool {
                    Tool::Gsttt => assert_eq!(prepare, Some("rm -rf .git/gsttt")),
                    Tool::DependencyCruiser => assert_eq!(prepare, Some("rm -rf .depcruise-cache")),
                    Tool::Grimp => assert_eq!(prepare, Some("rm -rf .grimp_cache")),
                    // madge keeps no cache, so there is nothing to remove.
                    Tool::Madge => assert_eq!(prepare, None),
                    Tool::CodebaseMemory => {
                        let prepare = prepare.unwrap();
                        assert!(prepare.contains(&format!("rm -rf {cache} ")), "{prepare}");
                    }
                }
                assert_eq!(run.cold.warmup, 0, "{}", run.tool.name());
            }
        }
    }

    #[gsttt_scenario::scenario("benchmarks", "A warm run keeps the cache")]
    #[test]
    fn a_warm_run_keeps_the_cache() {
        let home = tempfile::tempdir().unwrap();
        let programs = programs(home.path());
        for repository in repositories() {
            for run in plan(&repository, &programs) {
                assert_eq!(run.warm.prepare, None, "{}", run.tool.name());
                assert!(run.warm.warmup >= 1, "{}", run.tool.name());
            }
        }
    }

    #[test]
    fn the_commands_are_the_ones_a_reader_types() {
        let home = tempfile::tempdir().unwrap();
        let programs = programs(home.path());
        let [typescript, python, rust] = repositories();
        let shown = |repository: &Repository| {
            plan(repository, &programs)
                .iter()
                .map(|run| run.command.shown())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            shown(&typescript),
            [
                "gsttt index",
                "depcruise . --no-config --ts-pre-compilation-deps --output-type json --cache .depcruise-cache --ts-config tsconfig.json",
                "madge --json --extensions ts,tsx,mts,cts,js,jsx,mjs,cjs . --ts-config tsconfig.json",
                "codebase-memory-mcp cli --json index_repository --repo-path . --name app --mode full",
            ]
        );
        assert_eq!(
            shown(&python)[1],
            "PYTHONPATH=. python -c \"import grimp; grimp.build_graph('django', cache_dir='.grimp_cache')\""
        );
        assert_eq!(shown(&rust)[0], "gsttt index");
    }

    #[test]
    fn the_executed_command_names_the_program_by_its_path() {
        let home = tempfile::tempdir().unwrap();
        let programs = programs(home.path());
        let [typescript, ..] = repositories();
        let runs = plan(&typescript, &programs);
        let depcruise = runs
            .iter()
            .find(|run| run.tool == Tool::DependencyCruiser)
            .unwrap();
        let program = programs
            .of(Tool::DependencyCruiser)
            .to_string_lossy()
            .into_owned();
        assert!(
            depcruise
                .command
                .executed()
                .starts_with(&format!("{program} . --no-config"))
        );
        assert!(!depcruise.command.shown().contains(&program));
    }

    #[test]
    fn a_word_is_quoted_only_when_a_shell_would_read_it_differently() {
        assert_eq!(quote("--ts-config"), "--ts-config");
        assert_eq!(quote("a b"), "\"a b\"");
        assert_eq!(quote(""), "\"\"");
        assert_eq!(quote("it's $HOME"), "'it'\\''s $HOME'");
    }
}
