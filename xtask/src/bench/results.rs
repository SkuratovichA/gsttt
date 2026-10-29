//! The results file, in the shape the site decodes, and the guard that keeps the paths of this machine out of it.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, ensure};
use serde::{Deserialize, Serialize};

/// The results of a run of `cargo xtask bench`. The order of the fields is the order of the keys in the file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Results {
    /// When the measurement started: an RFC 3339 time in UTC.
    pub(super) measured_at: String,
    /// The CPU, the cores, the memory and the operating system of the machine that measured.
    pub(super) machine: String,
    pub(super) repositories: Vec<RepositoryRuns>,
}

/// What was measured on one repository.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct RepositoryRuns {
    pub(super) name: String,
    pub(super) url: String,
    /// The tracked files of the checkout with an extension of its language.
    pub(super) files: u64,
    pub(super) tools: Vec<ToolRun>,
}

/// The medians of one tool on one repository.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ToolRun {
    pub(super) tool: String,
    pub(super) version: String,
    /// The median of the cold runs, in whole milliseconds.
    pub(super) cold_ms: u64,
    /// The median of the warm runs, in whole milliseconds.
    pub(super) warm_ms: u64,
    /// The command as a reader types it in the checkout.
    pub(super) command: String,
}

impl Results {
    /// Fails on whatever the site would refuse to decode.
    fn check(&self) -> anyhow::Result<()> {
        ensure!(
            is_utc_time(&self.measured_at),
            "measuredAt `{}` is not an RFC 3339 time in UTC",
            self.measured_at
        );
        ensure!(!self.machine.is_empty(), "machine is empty");
        ensure!(!self.repositories.is_empty(), "no repository was measured");
        for repository in &self.repositories {
            let name = &repository.name;
            ensure!(
                !name.is_empty() && repository.url.starts_with("https://"),
                "the name or the URL of a repository is wrong: `{name}`"
            );
            ensure!(repository.files >= 1, "{name} has no files");
            ensure!(
                !repository.tools.is_empty(),
                "no tool was measured on {name}"
            );
            for run in &repository.tools {
                ensure!(
                    ![&run.tool, &run.version, &run.command]
                        .iter()
                        .any(|text| text.is_empty()),
                    "a result of {name} has an empty field"
                );
            }
        }
        Ok(())
    }
}

/// Writes `results` to `out` through a temporary file beside it, so that a failed run leaves no half of a file. Fails, writing nothing, when the text holds one of the paths of `local`.
pub(super) fn write(out: &Path, results: &Results, local: &[&str]) -> anyhow::Result<()> {
    results.check()?;
    write_json(out, results, local)
}

/// Writes `value` to `out` as pretty JSON and a newline, through a temporary file beside it, so that a failed run leaves no half of a file. Fails, writing nothing, when the text holds one of the paths of `local`.
pub(crate) fn write_json<T: Serialize>(
    out: &Path,
    value: &T,
    local: &[&str],
) -> anyhow::Result<()> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    refuse_local_paths(&text, local)?;
    write_atomically(out, &text)
}

/// Fails when `text` holds one of the paths of `local`: the results are published, and a path of this machine is not.
pub(crate) fn refuse_local_paths(text: &str, local: &[&str]) -> anyhow::Result<()> {
    for path in local {
        ensure!(
            path.is_empty() || !text.contains(path),
            "the results hold the local path {path}, which must not be published"
        );
    }
    Ok(())
}

/// Writes `text` to `out` through a temporary file beside it, so that a failed run leaves no half of a file.
pub(crate) fn write_atomically(out: &Path, text: &str) -> anyhow::Result<()> {
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let mut name = out.as_os_str().to_owned();
    name.push(".tmp");
    let temporary = PathBuf::from(name);
    fs::write(&temporary, text).with_context(|| format!("cannot write {}", temporary.display()))?;
    fs::rename(&temporary, out)
        .with_context(|| format!("cannot move {} to {}", temporary.display(), out.display()))
}

/// Whether `text` is `YYYY-MM-DDTHH:MM:SSZ`.
pub(crate) fn is_utc_time(text: &str) -> bool {
    text.len() == 20
        && text.bytes().enumerate().all(|(at, byte)| match at {
            4 | 7 => byte == b'-',
            10 => byte == b'T',
            13 | 16 => byte == b':',
            19 => byte == b'Z',
            _ => byte.is_ascii_digit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::pins::{Language, Repository};
    use crate::bench::plan::{Programs, plan};

    fn sample() -> Results {
        let tool = |name: &str, cold_ms, warm_ms| ToolRun {
            tool: name.to_owned(),
            version: "1.2.3".to_owned(),
            cold_ms,
            warm_ms,
            command: format!("{name} index"),
        };
        Results {
            measured_at: "2026-10-28T12:30:00Z".to_owned(),
            machine: "Some CPU, 8 cores, 16 GiB, SomeOS 1.0".to_owned(),
            repositories: vec![RepositoryRuns {
                name: "vite".to_owned(),
                url: "https://example.test/vite".to_owned(),
                files: 42,
                tools: vec![tool("gsttt", 120, 30), tool("madge", 900, 800)],
            }],
        }
    }

    #[gsttt_scenario::scenario("benchmarks", "The results have the published shape")]
    #[test]
    fn the_results_have_the_published_shape() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("nested/benchmarks.json");
        let results = sample();
        write(&out, &results, &[]).unwrap();

        let text = fs::read_to_string(&out).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let keys = |value: &serde_json::Value| {
            value
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(keys(&value), ["measuredAt", "machine", "repositories"]);
        let repository = &value["repositories"][0];
        assert_eq!(keys(repository), ["name", "url", "files", "tools"]);
        assert_eq!(
            keys(&repository["tools"][0]),
            ["tool", "version", "coldMs", "warmMs", "command"]
        );
        assert!(repository["files"].is_u64() && repository["tools"][0]["coldMs"].is_u64());

        assert_eq!(serde_json::from_str::<Results>(&text).unwrap(), results);
        assert!(
            !out.with_file_name("benchmarks.json.tmp").exists(),
            "the temporary file is renamed"
        );
    }

    #[gsttt_scenario::scenario("benchmarks", "No local path reaches the results")]
    #[test]
    fn no_local_path_reaches_the_results() {
        let home = tempfile::tempdir().unwrap();
        let user = "quincy-bench";
        let root = home.path().join(user).join("git/gsttt");
        let programs = Programs::new(
            &root.join("bench/tools"),
            &root.join("target/release/gsttt"),
        );
        let repository = Repository {
            name: "vite".to_owned(),
            url: "https://example.test/vite".to_owned(),
            tag: "1.0.0".to_owned(),
            commit: "0".repeat(40),
            language: Language::Typescript,
            tsconfig: Some("tsconfig.json".to_owned()),
            packages: Vec::new(),
            agents: None,
        };
        let tools = plan(&repository, &programs)
            .iter()
            .map(|run| ToolRun {
                tool: run.tool.name().to_owned(),
                version: "1.0.0".to_owned(),
                cold_ms: 2,
                warm_ms: 1,
                command: run.command.shown(),
            })
            .collect();
        let results = Results {
            repositories: vec![RepositoryRuns {
                name: "vite".to_owned(),
                url: repository.url.clone(),
                files: 3,
                tools,
            }],
            ..sample()
        };

        let checkout = root.join("bench/repos/vite");
        let local = [
            home.path().to_string_lossy().into_owned(),
            checkout.to_string_lossy().into_owned(),
        ];
        let local: Vec<&str> = local.iter().map(String::as_str).collect();
        let out = home.path().join("benchmarks.json");
        write(&out, &results, &local).unwrap();
        let text = fs::read_to_string(&out).unwrap();
        for forbidden in [local[0], local[1], user] {
            assert!(!text.contains(forbidden), "{forbidden} reached the results");
        }

        // The guard: a command that names a path of this machine is refused and nothing is written.
        let mut leaky = results;
        leaky.repositories[0].tools[0].command = format!(
            "{} index",
            programs.of(crate::bench::pins::Tool::Gsttt).display()
        );
        let leaked = home.path().join("leaked.json");
        let error = write(&leaked, &leaky, &local).unwrap_err().to_string();
        assert!(error.contains("local path"), "{error}");
        assert!(!leaked.exists() && !leaked.with_file_name("leaked.json.tmp").exists());
    }

    #[test]
    fn what_the_site_would_refuse_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut results = sample();
        results.repositories[0].files = 0;
        assert!(write(&dir.path().join("a.json"), &results, &[]).is_err());
        let mut results = sample();
        results.measured_at = "2026-10-28".to_owned();
        assert!(write(&dir.path().join("b.json"), &results, &[]).is_err());
        let mut results = sample();
        results.repositories[0].tools.clear();
        assert!(write(&dir.path().join("c.json"), &results, &[]).is_err());
    }
}
