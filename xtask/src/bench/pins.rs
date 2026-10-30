//! The pins of `bench/bench.toml`: the repositories the benchmarks measure, each at a commit, and the tools that measure them, each at a version.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context as _, ensure};
use serde::Deserialize;

use crate::digest::is_hex;

mod agents;

pub(crate) use self::agents::{Agents, RepositoryAgents};

/// Where the pins are, relative to the repository root.
const FILE: &str = "bench/bench.toml";

/// A tool the benchmarks time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Tool {
    /// The binary the task is given: the only tool that is not pinned.
    Gsttt,
    /// Reads TypeScript and JavaScript.
    DependencyCruiser,
    /// Reads TypeScript and JavaScript.
    Madge,
    /// Reads Python packages.
    Grimp,
    /// Reads every language.
    CodebaseMemory,
}

impl Tool {
    /// The name the results give the tool.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Gsttt => "gsttt",
            Self::DependencyCruiser => "dependency-cruiser",
            Self::Madge => "madge",
            Self::Grimp => "grimp",
            Self::CodebaseMemory => "codebase-memory-mcp",
        }
    }

    /// The program a reader types to run the tool.
    pub(crate) fn program(self) -> &'static str {
        match self {
            Self::Gsttt => "gsttt",
            Self::DependencyCruiser => "depcruise",
            Self::Madge => "madge",
            Self::Grimp => "python",
            Self::CodebaseMemory => "codebase-memory-mcp",
        }
    }
}

/// The language of a repository, which decides the tools that measure it.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Language {
    Typescript,
    Python,
    Rust,
}

impl Language {
    /// The tools that measure a repository of this language, gsttt first.
    pub(crate) fn tools(self) -> &'static [Tool] {
        match self {
            Self::Typescript => &[
                Tool::Gsttt,
                Tool::DependencyCruiser,
                Tool::Madge,
                Tool::CodebaseMemory,
            ],
            Self::Python => &[Tool::Gsttt, Tool::Grimp, Tool::CodebaseMemory],
            Self::Rust => &[Tool::Gsttt, Tool::CodebaseMemory],
        }
    }

    /// The extensions, without the dot, of the files that count as source of this language.
    pub(crate) fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Typescript => &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"],
            Self::Python => &["py"],
            Self::Rust => &["rs"],
        }
    }
}

/// A repository the benchmarks measure.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Repository {
    /// The directory under `bench/repos` and the name in the results.
    pub(crate) name: String,
    pub(crate) url: String,
    /// The release the commit belongs to. It is for readers: the commit is what is fetched.
    pub(crate) tag: String,
    /// The full commit, 40 lower-case hexadecimal digits.
    pub(crate) commit: String,
    pub(crate) language: Language,
    /// The `tsconfig.json` the TypeScript tools read, relative to the checkout.
    pub(crate) tsconfig: Option<String>,
    /// The packages grimp reads.
    #[serde(default)]
    pub(crate) packages: Vec<String>,
    /// Whether the speed benchmark measures this repository; `false` for one that only the agent benchmarks ask about, which then needs an `agents` table.
    #[serde(default = "measured")]
    pub(crate) speed: bool,
    /// What the agent benchmarks ask about this repository; absent when they ask nothing.
    pub(crate) agents: Option<RepositoryAgents>,
}

/// The default of `speed`: a repository is measured unless it says it is not.
fn measured() -> bool {
    true
}

/// A tool that is pinned by its version alone.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Version {
    pub(crate) version: String,
}

/// An archive of codebase-memory-mcp.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Archive {
    /// The file name in the release.
    pub(crate) file: String,
    /// The SHA-256 of the file, 64 lower-case hexadecimal digits.
    pub(crate) sha256: String,
}

/// codebase-memory-mcp, which is downloaded as a release archive.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Cbm {
    pub(crate) version: String,
    /// The base of the download URLs: it is followed by `v<version>/<file>`.
    pub(crate) releases: String,
    /// The archives by platform, such as `darwin-arm64`.
    pub(crate) archives: BTreeMap<String, Archive>,
}

/// The tools, each at a version.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Tools {
    #[serde(rename = "dependency-cruiser")]
    dependency_cruiser: Version,
    madge: Version,
    pub(crate) typescript: Version,
    /// The language server of the `lsp` setup of the agent benchmarks, installed beside TypeScript.
    #[serde(rename = "typescript-language-server")]
    pub(crate) typescript_language_server: Version,
    /// The parser of GraphQL documents of the oracle of the questions about clients, installed beside TypeScript.
    pub(crate) graphql: Version,
    pub(crate) grimp: Version,
    #[serde(rename = "codebase-memory-mcp")]
    pub(crate) codebase_memory: Cbm,
}

impl Tools {
    /// The pinned version of `tool`; `None` for gsttt, which is the binary the task is given.
    pub(crate) fn version(&self, tool: Tool) -> Option<&str> {
        match tool {
            Tool::Gsttt => None,
            Tool::DependencyCruiser => Some(&self.dependency_cruiser.version),
            Tool::Madge => Some(&self.madge.version),
            Tool::Grimp => Some(&self.grimp.version),
            Tool::CodebaseMemory => Some(&self.codebase_memory.version),
        }
    }

    /// The packages pnpm installs into `bench/tools/node`, each with its version. TypeScript is there so that the other tools find their parser.
    pub(crate) fn node_packages(&self) -> [(&'static str, &str); 5] {
        [
            ("dependency-cruiser", &self.dependency_cruiser.version),
            ("madge", &self.madge.version),
            ("typescript", &self.typescript.version),
            (
                "typescript-language-server",
                &self.typescript_language_server.version,
            ),
            ("graphql", &self.graphql.version),
        ]
    }
}

/// Everything `bench/bench.toml` pins.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Pins {
    pub(crate) repository: Vec<Repository>,
    pub(crate) tools: Tools,
    /// What the agent runs share.
    pub(crate) agents: Agents,
}

impl Pins {
    /// Reads and checks the pins of the repository at `root`.
    pub(crate) fn load(root: &Path) -> anyhow::Result<Self> {
        let file = root.join(FILE);
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("cannot read {}", file.display()))?;
        Self::parse(&text).with_context(|| format!("{} does not hold valid pins", file.display()))
    }

    /// Parses pins and refuses the ones that do not pin: a commit that is not a full commit, a version that is not a version, an archive without a checksum.
    pub(crate) fn parse(text: &str) -> anyhow::Result<Self> {
        let pins: Self = toml::from_str(text)?;
        pins.validate()?;
        Ok(pins)
    }

    fn validate(&self) -> anyhow::Result<()> {
        ensure!(!self.repository.is_empty(), "no repository is pinned");
        self.agents.validate()?;
        let mut names = BTreeSet::new();
        for repository in &self.repository {
            let name = &repository.name;
            ensure!(
                is_name(name),
                "the repository name `{name}` is not made of letters, digits, `-`, `_` and `.`"
            );
            ensure!(
                names.insert(name.as_str()),
                "the repository `{name}` is pinned twice"
            );
            ensure!(
                is_https(&repository.url),
                "the URL of `{name}` is not an https URL"
            );
            ensure!(!repository.tag.trim().is_empty(), "`{name}` has no tag");
            ensure!(
                is_hex(&repository.commit, 40),
                "the commit of `{name}` is not 40 lower-case hexadecimal digits"
            );
            if let Some(tsconfig) = &repository.tsconfig {
                ensure!(
                    repository.language == Language::Typescript && is_inside(tsconfig),
                    "the tsconfig of `{name}` must be a path inside a TypeScript checkout"
                );
            }
            ensure!(
                repository.speed || repository.agents.is_some(),
                "`{name}` is left out of the speed benchmark and has no `agents` table: nothing uses it"
            );
            if let Some(agents) = &repository.agents {
                agents.validate(repository)?;
            }
            let python = repository.language == Language::Python;
            ensure!(
                python != repository.packages.is_empty(),
                "`{name}` must name the packages grimp reads if and only if it is Python"
            );
            for package in &repository.packages {
                ensure!(
                    is_name(package),
                    "the package `{package}` of `{name}` is not a package name"
                );
            }
        }

        let tools = &self.tools;
        let mut versions = tools.node_packages().to_vec();
        versions.push(("grimp", &tools.grimp.version));
        versions.push(("codebase-memory-mcp", &tools.codebase_memory.version));
        for (tool, version) in versions {
            ensure!(
                is_version(version),
                "the version of {tool} is `{version}`, which is not a version"
            );
        }

        let cbm = &tools.codebase_memory;
        ensure!(
            is_https(&cbm.releases),
            "the releases URL of codebase-memory-mcp is not an https URL"
        );
        ensure!(
            !cbm.archives.is_empty(),
            "no archive of codebase-memory-mcp is pinned"
        );
        for (platform, archive) in &cbm.archives {
            ensure!(
                is_name(&archive.file),
                "the archive file of codebase-memory-mcp for {platform} is not a file name"
            );
            ensure!(
                is_hex(&archive.sha256, 64),
                "the SHA-256 of the {platform} archive is not 64 lower-case hexadecimal digits"
            );
        }
        Ok(())
    }
}

fn is_name(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn is_https(url: &str) -> bool {
    url.starts_with("https://") && !url.contains(char::is_whitespace)
}

fn is_inside(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('/') && !path.split('/').any(|part| part == "..")
}

fn is_version(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_digit())
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest pins that pass, to be broken one rule at a time.
    const MINIMAL: &str = r#"
[[repository]]
name = "ripgrep"
url = "https://example.test/ripgrep"
tag = "15.2.0"
commit = "e89fff89ac9af12e8d4ce9d5fd07beb408ca730f"
language = "rust"

[tools.dependency-cruiser]
version = "1.0.0"

[tools.madge]
version = "1.0.0"

[tools.typescript]
version = "1.0.0"

[tools.typescript-language-server]
version = "1.0.0"

[tools.graphql]
version = "1.0.0"

[tools.grimp]
version = "1.0"

[tools.codebase-memory-mcp]
version = "1.0.0"
releases = "https://example.test/releases"

[tools.codebase-memory-mcp.archives.linux-amd64]
file = "cbm.tar.gz"
sha256 = "8453015145f072c1b9b019cbdd02cea8cc6f18cc5e116036dd20992631c638c7"

[agents]
model = "sonnet"
repeats = 3
seed = "seed"
seconds = 900
turns = 80
"#;

    /// A TypeScript repository that asks for agent questions, to be added to the minimal pins and broken one rule at a time.
    const VITE: &str = r#"
[[repository]]
name = "vite"
url = "https://example.test/vite"
tag = "8.3.5"
commit = "981f07fa4580fb88b333c4450431e4715d2a0b82"
language = "typescript"
tsconfig = "packages/vite/tsconfig.json"

[repository.agents]
scope = "packages/vite"
users = 4
tests = 4
install = ["pnpm", "install"]
test = ["pnpm", "exec", "vitest", "run"]
"#;

    /// A repository that only the agent benchmarks ask about, and only about clients, to be added to the minimal pins.
    const HOPPSCOTCH: &str = r#"
[[repository]]
name = "hoppscotch"
url = "https://example.test/hoppscotch"
tag = "2026.10.0"
commit = "39d25650bdb4e86464b2cd69174c567b86f57892"
language = "typescript"
tsconfig = "packages/hoppscotch-backend/tsconfig.json"
speed = false

[repository.agents]
scope = "packages"
clients = 6
"#;

    /// The pins as they are committed.
    fn committed() -> Pins {
        let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../bench/bench.toml");
        Pins::parse(&std::fs::read_to_string(file).unwrap()).unwrap()
    }

    #[gsttt_scenario::scenario("benchmarks", "Every repository is pinned at a commit")]
    #[test]
    fn every_repository_is_pinned_at_a_commit() {
        let pins = committed();
        assert!(!pins.repository.is_empty());
        let names: BTreeSet<&str> = pins
            .repository
            .iter()
            .map(|repository| repository.name.as_str())
            .collect();
        assert_eq!(names.len(), pins.repository.len(), "a name is pinned twice");
        for repository in &pins.repository {
            assert!(
                repository.url.starts_with("https://"),
                "{}",
                repository.name
            );
            assert!(!repository.tag.is_empty(), "{}", repository.name);
            assert!(is_hex(&repository.commit, 40), "{}", repository.name);
        }
    }

    #[gsttt_scenario::scenario("benchmarks", "Every tool is pinned at a version")]
    #[test]
    fn every_tool_is_pinned_at_a_version() {
        let pins = committed();
        let used: BTreeSet<Tool> = pins
            .repository
            .iter()
            .flat_map(|repository| repository.language.tools())
            .copied()
            .collect();
        assert!(used.len() > 1);
        for tool in used {
            match pins.tools.version(tool) {
                None => assert_eq!(tool, Tool::Gsttt, "only gsttt is not pinned"),
                Some(version) => assert!(
                    version.starts_with(|c: char| c.is_ascii_digit()),
                    "{}: {version}",
                    tool.name()
                ),
            }
        }
        let archives = &pins.tools.codebase_memory.archives;
        assert!(!archives.is_empty());
        for (platform, archive) in archives {
            assert!(is_hex(&archive.sha256, 64), "{platform}");
        }
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "The agent runs are pinned")]
    #[test]
    fn the_agent_runs_are_pinned() {
        let pins = committed();
        let agents = &pins.agents;
        assert!(!agents.model.is_empty() && !agents.model.contains(char::is_whitespace));
        assert!(agents.repeats >= 1);
        assert_ne!(agents.seed.trim(), "");
        assert!(agents.seconds >= 60);
        assert!(agents.turns >= 1);
        assert!(is_version(&pins.tools.typescript_language_server.version));
        assert!(is_version(&pins.tools.graphql.version));
        assert!(
            pins.repository
                .iter()
                .any(|repository| repository.agents.is_some()),
            "no repository asks agent questions"
        );
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "Questions about tests need a test command")]
    #[test]
    fn questions_about_tests_need_a_test_command() {
        Pins::parse(&format!("{MINIMAL}{VITE}")).unwrap();
        let without = VITE.replace("test = [\"pnpm\", \"exec\", \"vitest\", \"run\"]\n", "");
        assert_ne!(without, VITE);
        let error = Pins::parse(&format!("{MINIMAL}{without}"))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("`vite`") && error.contains("test command"),
            "{error}"
        );
    }

    #[test]
    fn the_shared_agent_pins_are_broken_one_rule_at_a_time() {
        for (from, to, message) in [
            ("model = \"sonnet\"", "model = \"two words\"", "model"),
            ("model = \"sonnet\"", "model = \"\"", "model"),
            ("repeats = 3", "repeats = 0", "repeat"),
            ("seed = \"seed\"", "seed = \" \"", "seed"),
            ("seconds = 900", "seconds = 59", "at least 60"),
            ("turns = 80", "turns = 0", "turn"),
        ] {
            let error = Pins::parse(&MINIMAL.replace(from, to))
                .unwrap_err()
                .to_string();
            assert!(error.contains(message), "{to}: {error}");
        }
    }

    #[test]
    fn the_agent_pins_of_a_repository_are_broken_one_rule_at_a_time() {
        for (from, to, message) in [
            (
                "scope = \"packages/vite\"",
                "scope = \"/packages\"",
                "scope",
            ),
            ("scope = \"packages/vite\"", "scope = \"../vite\"", "scope"),
            (
                "scope = \"packages/vite\"",
                "scope = \"packages//vite\"",
                "scope",
            ),
            ("scope = \"packages/vite\"", "scope = \"\"", "scope"),
            ("users = 4", "users = 0", "users"),
            ("tests = 4", "tests = 0", "tests"),
            (
                "install = [\"pnpm\", \"install\"]",
                "install = [\"pnpm\", \"\"]",
                "install command",
            ),
            (
                "install = [\"pnpm\", \"install\"]",
                "install = []",
                "install command",
            ),
        ] {
            let text = format!("{MINIMAL}{}", VITE.replace(from, to));
            let error = Pins::parse(&text).unwrap_err().to_string();
            assert!(
                error.contains(message) && error.contains("`vite`"),
                "{to}: {error}"
            );
        }
        let rust = format!(
            "{MINIMAL}{}",
            VITE.replace("language = \"typescript\"", "language = \"rust\"")
        );
        assert!(
            Pins::parse(&rust).is_err(),
            "only a TypeScript repository asks agent questions"
        );
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A repository can ask only about the clients of methods"
    )]
    #[test]
    fn a_repository_can_ask_only_about_clients() {
        let pins = Pins::parse(&format!("{MINIMAL}{HOPPSCOTCH}")).unwrap();
        let agents = pins
            .repository
            .iter()
            .find_map(|repository| repository.agents.as_ref())
            .unwrap();
        assert_eq!(
            (agents.users(), agents.tests(), agents.clients()),
            (0, 0, 6)
        );

        for (from, to, message) in [
            ("clients = 6", "clients = 0", "no question about clients"),
            ("clients = 6\n", "", "asks for no question"),
        ] {
            let text = format!("{MINIMAL}{}", HOPPSCOTCH.replace(from, to));
            let error = Pins::parse(&text).unwrap_err().to_string();
            assert!(
                error.contains(message) && error.contains("`hoppscotch`"),
                "{to}: {error}"
            );
        }
    }

    #[test]
    fn a_repository_is_measured_for_speed_unless_it_says_it_is_not() {
        let pins = Pins::parse(&format!("{MINIMAL}{HOPPSCOTCH}")).unwrap();
        let measured: Vec<(&str, bool)> = pins
            .repository
            .iter()
            .map(|repository| (repository.name.as_str(), repository.speed))
            .collect();
        assert_eq!(measured, [("ripgrep", true), ("hoppscotch", false)]);

        // Left out of the speed benchmark and asked nothing, a repository is used by nothing.
        let unused = HOPPSCOTCH.split("[repository.agents]").next().unwrap();
        let error = Pins::parse(&format!("{MINIMAL}{unused}"))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("`hoppscotch`") && error.contains("speed benchmark"),
            "{error}"
        );

        // The committed pins leave out only repositories that the agents ask about.
        for repository in committed().repository {
            assert!(
                repository.speed || repository.agents.is_some(),
                "{}",
                repository.name
            );
        }
    }

    #[test]
    fn minimal_pins_pass() {
        Pins::parse(MINIMAL).unwrap();
    }

    #[test]
    fn a_commit_must_be_forty_lower_case_hexadecimal_digits() {
        for commit in [
            "e89fff8",
            "E89FFF89AC9AF12E8D4CE9D5FD07BEB408CA730F",
            "v15.2.0",
        ] {
            let text = MINIMAL.replace("e89fff89ac9af12e8d4ce9d5fd07beb408ca730f", commit);
            let error = Pins::parse(&text).unwrap_err().to_string();
            assert!(
                error.contains("40 lower-case hexadecimal digits"),
                "{commit}: {error}"
            );
        }
    }

    #[test]
    fn a_repository_cannot_be_pinned_twice() {
        let text = format!(
            "{MINIMAL}\n[[repository]]\nname = \"ripgrep\"\nurl = \"https://example.test/other\"\ntag = \"1\"\ncommit = \"{}\"\nlanguage = \"rust\"\n",
            "1".repeat(40)
        );
        let error = Pins::parse(&text).unwrap_err().to_string();
        assert!(error.contains("pinned twice"), "{error}");
    }

    #[test]
    fn an_archive_needs_a_checksum_and_a_version_needs_digits() {
        let short = MINIMAL.replace(
            "8453015145f072c1b9b019cbdd02cea8cc6f18cc5e116036dd20992631c638c7",
            "845301",
        );
        assert!(
            Pins::parse(&short)
                .unwrap_err()
                .to_string()
                .contains("SHA-256")
        );
        let latest = MINIMAL.replace(
            "version = \"1.0.0\"\n\n[tools.madge]",
            "version = \"latest\"\n\n[tools.madge]",
        );
        assert!(
            Pins::parse(&latest)
                .unwrap_err()
                .to_string()
                .contains("not a version")
        );
    }
}
