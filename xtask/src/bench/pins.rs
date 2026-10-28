//! The pins of `bench/bench.toml`: the repositories the benchmarks measure, each at a commit, and the tools that measure them, each at a version.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context as _, ensure};
use serde::Deserialize;

use crate::digest::is_hex;

/// Where the pins are, relative to the repository root.
const FILE: &str = "bench/bench.toml";

/// A tool the benchmarks time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Tool {
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
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Gsttt => "gsttt",
            Self::DependencyCruiser => "dependency-cruiser",
            Self::Madge => "madge",
            Self::Grimp => "grimp",
            Self::CodebaseMemory => "codebase-memory-mcp",
        }
    }

    /// The program a reader types to run the tool.
    pub(super) fn program(self) -> &'static str {
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
pub(super) enum Language {
    Typescript,
    Python,
    Rust,
}

impl Language {
    /// The tools that measure a repository of this language, gsttt first.
    pub(super) fn tools(self) -> &'static [Tool] {
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
    pub(super) fn extensions(self) -> &'static [&'static str] {
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
pub(super) struct Repository {
    /// The directory under `bench/repos` and the name in the results.
    pub(super) name: String,
    pub(super) url: String,
    /// The release the commit belongs to. It is for readers: the commit is what is fetched.
    pub(super) tag: String,
    /// The full commit, 40 lower-case hexadecimal digits.
    pub(super) commit: String,
    pub(super) language: Language,
    /// The `tsconfig.json` the TypeScript tools read, relative to the checkout.
    pub(super) tsconfig: Option<String>,
    /// The packages grimp reads.
    #[serde(default)]
    pub(super) packages: Vec<String>,
}

/// A tool that is pinned by its version alone.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Version {
    pub(super) version: String,
}

/// An archive of codebase-memory-mcp.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Archive {
    /// The file name in the release.
    pub(super) file: String,
    /// The SHA-256 of the file, 64 lower-case hexadecimal digits.
    pub(super) sha256: String,
}

/// codebase-memory-mcp, which is downloaded as a release archive.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cbm {
    pub(super) version: String,
    /// The base of the download URLs: it is followed by `v<version>/<file>`.
    pub(super) releases: String,
    /// The archives by platform, such as `darwin-arm64`.
    pub(super) archives: BTreeMap<String, Archive>,
}

/// The tools, each at a version.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Tools {
    #[serde(rename = "dependency-cruiser")]
    dependency_cruiser: Version,
    madge: Version,
    typescript: Version,
    pub(super) grimp: Version,
    #[serde(rename = "codebase-memory-mcp")]
    pub(super) codebase_memory: Cbm,
}

impl Tools {
    /// The pinned version of `tool`; `None` for gsttt, which is the binary the task is given.
    pub(super) fn version(&self, tool: Tool) -> Option<&str> {
        match tool {
            Tool::Gsttt => None,
            Tool::DependencyCruiser => Some(&self.dependency_cruiser.version),
            Tool::Madge => Some(&self.madge.version),
            Tool::Grimp => Some(&self.grimp.version),
            Tool::CodebaseMemory => Some(&self.codebase_memory.version),
        }
    }

    /// The packages pnpm installs into `bench/tools/node`, each with its version. TypeScript is there so that the other two find their parser.
    pub(super) fn node_packages(&self) -> [(&'static str, &str); 3] {
        [
            ("dependency-cruiser", &self.dependency_cruiser.version),
            ("madge", &self.madge.version),
            ("typescript", &self.typescript.version),
        ]
    }
}

/// Everything `bench/bench.toml` pins.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Pins {
    pub(super) repository: Vec<Repository>,
    pub(super) tools: Tools,
}

impl Pins {
    /// Reads and checks the pins of the repository at `root`.
    pub(super) fn load(root: &Path) -> anyhow::Result<Self> {
        let file = root.join(FILE);
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("cannot read {}", file.display()))?;
        Self::parse(&text).with_context(|| format!("{} does not hold valid pins", file.display()))
    }

    /// Parses pins and refuses the ones that do not pin: a commit that is not a full commit, a version that is not a version, an archive without a checksum.
    pub(super) fn parse(text: &str) -> anyhow::Result<Self> {
        let pins: Self = toml::from_str(text)?;
        pins.validate()?;
        Ok(pins)
    }

    fn validate(&self) -> anyhow::Result<()> {
        ensure!(!self.repository.is_empty(), "no repository is pinned");
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

[tools.grimp]
version = "1.0"

[tools.codebase-memory-mcp]
version = "1.0.0"
releases = "https://example.test/releases"

[tools.codebase-memory-mcp.archives.linux-amd64]
file = "cbm.tar.gz"
sha256 = "8453015145f072c1b9b019cbdd02cea8cc6f18cc5e116036dd20992631c638c7"
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
