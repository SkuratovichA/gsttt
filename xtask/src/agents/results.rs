//! The results of `cargo xtask agents run`, in the shape the site decodes, and what decides whether a task started over may add to them.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context as _, ensure};
use serde::{Deserialize, Serialize};

use super::init::Init;
use super::questions::Kind;
use super::setups::{Arm, Setup};
use crate::bench::results::{is_utc_time, write_json};
use crate::digest::is_hex;

/// The results of the runs. The order of the fields is the order of the keys in the file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Results {
    /// When the measurement started: an RFC 3339 time in UTC.
    pub(super) measured_at: String,
    /// The CPU, the cores, the memory and the operating system of the machine that measured.
    pub(super) machine: String,
    /// The version of Claude Code that made the runs, as the init event of a run says.
    pub(super) claude_version: String,
    /// The id of the model that answered, as the init event of a run says, not the alias the runs were asked for.
    pub(super) model: String,
    /// The SHA-256 of the questions the runs were asked, as the task renders them.
    pub(super) questions_sha256: String,
    pub(super) tools: Vec<ToolVersion>,
    pub(super) runs: Vec<Run>,
}

/// The version of a tool the runs used.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToolVersion {
    pub(super) tool: String,
    pub(super) version: String,
}

/// How a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Outcome {
    /// The final message has a block marked `files`.
    Answered,
    /// The final message has none.
    NoAnswer,
    /// The run was stopped at the wall-clock limit.
    Timeout,
    /// The run used its turns.
    TurnLimit,
}

impl Outcome {
    /// The word the file uses for the outcome.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Answered => "answered",
            Self::NoAnswer => "no-answer",
            Self::Timeout => "timeout",
            Self::TurnLimit => "turn-limit",
        }
    }
}

/// One run: a question asked in a setup, in one of its arms. The order of the fields is the order of the keys in the file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Run {
    /// The id of the question.
    pub(super) question: String,
    /// The kind of the question, which the table groups by.
    pub(super) kind: Kind,
    pub(super) setup: Setup,
    /// Whether the model was told about the tools of the setup or found them installed.
    pub(super) arm: Arm,
    /// From 1.
    pub(super) repeat: u32,
    pub(super) outcome: Outcome,
    /// The paths of the answer, relative to the checkout, each once and in byte order; a path outside the checkout is a stand-in.
    pub(super) answer: Vec<String>,
    pub(super) precision: f64,
    pub(super) recall: f64,
    pub(super) f1: f64,
    pub(super) input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) cache_read_tokens: u64,
    pub(super) cache_write_tokens: u64,
    /// What Claude Code says the run cost; `null` when the run ended without saying.
    pub(super) cost_usd: Option<f64>,
    pub(super) turns: u32,
    /// How many times the run called each tool, by the name the stream gives it, every call counted once. Kept for every outcome, a run that ended without a result included.
    pub(super) tool_calls: BTreeMap<String, u32>,
    pub(super) duration_ms: u64,
    /// Whether the run left the checkout, its HEAD, or the index of its setup other than the preparation left them. They were put back before the next run.
    pub(super) changed_checkout: bool,
}

impl Run {
    /// The tokens of the four kinds together.
    pub(super) fn tokens(&self) -> u64 {
        [
            self.input_tokens,
            self.output_tokens,
            self.cache_read_tokens,
            self.cache_write_tokens,
        ]
        .into_iter()
        .fold(0, u64::saturating_add)
    }
}

impl Results {
    /// Results with no run yet. The model and the version of Claude Code are the ones of the first run, which tells them in its init event.
    pub(super) fn start(
        measured_at: String,
        machine: String,
        questions_sha256: String,
        tools: Vec<ToolVersion>,
    ) -> Self {
        Self {
            measured_at,
            machine,
            claude_version: String::new(),
            model: String::new(),
            questions_sha256,
            tools,
            runs: Vec::new(),
        }
    }

    /// Reads the results in `file`.
    pub(super) fn load(file: &Path) -> anyhow::Result<Self> {
        let text =
            fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
        let results: Self = serde_json::from_str(&text)
            .with_context(|| format!("{} does not hold agent results", file.display()))?;
        results
            .check()
            .with_context(|| format!("{} is not valid", file.display()))?;
        Ok(results)
    }

    /// Fails unless runs may be added: the results of `file`, which these are, were written for the same questions and with the same versions of the tools. The model and the version of Claude Code are held against the runs themselves, by [`Results::accept`].
    pub(super) fn ensure_continues(
        &self,
        file: &Path,
        questions_sha256: &str,
        tools: &[ToolVersion],
    ) -> anyhow::Result<()> {
        ensure!(
            self.questions_sha256 == questions_sha256,
            "{} was written for other questions than the ones in the questions file; start a new results file with --out",
            file.display()
        );
        ensure!(
            self.tools == tools,
            "{} was written with other versions of the tools ({}) than these ({}); start a new results file with --out",
            file.display(),
            versions(&self.tools),
            versions(tools)
        );
        Ok(())
    }

    /// Whether the results hold the run of `question` in `setup` and `arm` at `repeat`.
    pub(super) fn holds(&self, question: &str, setup: Setup, arm: Arm, repeat: u32) -> bool {
        self.runs.iter().any(|run| {
            run.question == question && run.setup == setup && run.arm == arm && run.repeat == repeat
        })
    }

    /// Takes the model and the version of Claude Code of a run as the ones of the results when they have none yet, and fails when they have others. Called as soon as the init event of a run is written, which is after the first request of the run is sent.
    pub(super) fn accept(&mut self, init: &Init) -> anyhow::Result<()> {
        adopt(&mut self.model, &init.model, "the model")?;
        adopt(
            &mut self.claude_version,
            &init.claude_code_version,
            "Claude Code",
        )
    }

    /// Writes the results to `out` through a temporary file, and fails, writing nothing, when they do not check or hold one of the paths of `local`.
    pub(super) fn write(&self, out: &Path, local: &[&str]) -> anyhow::Result<()> {
        self.check()?;
        write_json(out, self, local)
    }

    /// Fails on whatever the site would refuse to decode.
    fn check(&self) -> anyhow::Result<()> {
        ensure!(
            is_utc_time(&self.measured_at),
            "measuredAt `{}` is not an RFC 3339 time in UTC",
            self.measured_at
        );
        for (name, value) in [
            ("machine", &self.machine),
            ("claudeVersion", &self.claude_version),
            ("model", &self.model),
        ] {
            ensure!(!value.is_empty(), "{name} is empty");
        }
        ensure!(
            is_hex(&self.questions_sha256, 64),
            "questionsSha256 `{}` is not 64 lowercase hexadecimal digits",
            self.questions_sha256
        );
        ensure!(!self.tools.is_empty(), "no version of a tool is recorded");
        ensure!(
            self.tools
                .iter()
                .all(|tool| !tool.tool.is_empty() && !tool.version.is_empty()),
            "a tool has an empty name or version"
        );

        let mut seen = BTreeSet::new();
        for run in &self.runs {
            let name = format!(
                "{} in {} ({}) at repeat {}",
                run.question, run.setup, run.arm, run.repeat
            );
            ensure!(
                !run.question.is_empty() && run.repeat >= 1,
                "a run has no question or repeat 0"
            );
            ensure!(
                seen.insert((&run.question, run.setup, run.arm, run.repeat)),
                "{name} is there twice"
            );
            for (what, score) in [
                ("precision", run.precision),
                ("recall", run.recall),
                ("f1", run.f1),
            ] {
                ensure!(
                    (0.0..=1.0).contains(&score),
                    "{what} of {name} is {score}, not a number from 0 to 1"
                );
            }
            ensure!(
                run.cost_usd
                    .is_none_or(|cost| cost.is_finite() && cost >= 0.0),
                "the cost of {name} is not a sum of money"
            );
        }
        Ok(())
    }
}

/// Takes `seen` as the value when nothing is recorded, and otherwise insists on it.
fn adopt(recorded: &mut String, seen: &str, what: &str) -> anyhow::Result<()> {
    if recorded.is_empty() {
        seen.clone_into(recorded);
        return Ok(());
    }
    ensure!(
        recorded == seen,
        "the results were written with {what} {recorded}, and this run uses {what} {seen}; start a new results file with --out"
    );
    Ok(())
}

fn versions(tools: &[ToolVersion]) -> String {
    tools
        .iter()
        .map(|tool| format!("{} {}", tool.tool, tool.version))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A run for the tests of the modules that read and write results.
#[cfg(test)]
pub(super) fn sample(question: &str, kind: Kind, setup: Setup, repeat: u32) -> Run {
    Run {
        question: question.to_owned(),
        kind,
        setup,
        arm: Arm::Installed,
        repeat,
        outcome: Outcome::Answered,
        answer: vec!["src/a.ts".to_owned()],
        precision: 1.0,
        recall: 0.5,
        f1: 2.0 / 3.0,
        input_tokens: 3,
        output_tokens: 5,
        cache_read_tokens: 100,
        cache_write_tokens: 10,
        cost_usd: Some(0.25),
        turns: 2,
        tool_calls: BTreeMap::new(),
        duration_ms: 1500,
        changed_checkout: false,
    }
}

/// Results with no run, and the versions of the tools the tests use.
#[cfg(test)]
pub(super) fn started() -> Results {
    Results::start(
        "2026-10-29T10:00:00Z".to_owned(),
        "Test CPU, 8 cores, 16 GiB, TestOS 1".to_owned(),
        "a".repeat(64),
        vec![ToolVersion {
            tool: "gsttt".to_owned(),
            version: "1.0.0".to_owned(),
        }],
    )
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::agents::init::expected;

    /// A change that makes results the site could not decode.
    type Breakage = fn(&mut Results);

    /// Results that can be written: the first run has told its model and its version.
    fn ready() -> Results {
        let mut results = started();
        results.accept(&expected(Setup::Plain)).unwrap();
        results
            .runs
            .push(sample("vite-users-1", Kind::Users, Setup::Plain, 1));
        results
    }

    fn keys(value: &Value) -> String {
        value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(",")
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "The agent results have the published shape")]
    #[test]
    fn the_agent_results_have_the_published_shape() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("agents.json");
        let mut results = ready();
        results.runs[0].tool_calls =
            BTreeMap::from([("Read".to_owned(), 2), ("LSP".to_owned(), 1)]);
        results.write(&file, &[]).unwrap();

        let text = fs::read_to_string(&file).unwrap();
        assert!(text.ends_with("}\n"));
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            keys(&value),
            "measuredAt,machine,claudeVersion,model,questionsSha256,tools,runs"
        );
        assert_eq!(keys(&value["tools"][0]), "tool,version");
        assert_eq!(
            keys(&value["runs"][0]),
            "question,kind,setup,arm,repeat,outcome,answer,precision,recall,f1,inputTokens,outputTokens,cacheReadTokens,cacheWriteTokens,costUsd,turns,toolCalls,durationMs,changedCheckout"
        );
        assert_eq!(value["runs"][0]["kind"], "users");
        assert_eq!(value["runs"][0]["setup"], "plain");
        assert_eq!(value["runs"][0]["arm"], "installed");
        assert_eq!(keys(&value["runs"][0]["toolCalls"]), "LSP,Read");
        assert_eq!(value["runs"][0]["toolCalls"]["Read"], 2);
        assert_eq!(value["runs"][0]["outcome"], "answered");
        assert_eq!(Results::load(&file).unwrap(), results);

        // A run that reported no cost keeps the key.
        results.runs[0].cost_usd = None;
        results.write(&file, &[]).unwrap();
        let value: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(value["runs"][0]["costUsd"], Value::Null);
        assert_eq!(Results::load(&file).unwrap(), results);
    }

    #[test]
    fn results_that_hold_a_path_of_the_machine_are_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("agents.json");
        let mut results = ready();
        results.runs[0].answer = vec!["/Users/quincy-bench/src/a.ts".to_owned()];
        let error = results
            .write(&file, &["/Users/quincy-bench"])
            .unwrap_err()
            .to_string();
        assert!(error.contains("/Users/quincy-bench"), "{error}");
        assert!(!file.exists(), "nothing is written");
    }

    #[test]
    fn results_the_site_could_not_decode_are_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("agents.json");
        let broken: [(&str, Breakage); 6] = [
            ("twice", |results| {
                results.runs.push(results.runs[0].clone());
            }),
            ("not a number from 0 to 1", |results| {
                results.runs[0].recall = 1.5;
            }),
            ("not a sum of money", |results| {
                results.runs[0].cost_usd = Some(-1.0);
            }),
            ("questionsSha256", |results| {
                results.questions_sha256 = "abc".to_owned();
            }),
            ("model is empty", |results| results.model.clear()),
            ("measuredAt", |results| {
                results.measured_at = "yesterday".to_owned();
            }),
        ];
        for (message, break_it) in broken {
            let mut results = ready();
            break_it(&mut results);
            let error = results.write(&file, &[]).unwrap_err().to_string();
            assert!(error.contains(message), "{message}: {error}");
        }
        assert!(!file.exists());
    }

    #[test]
    fn a_file_with_a_key_nobody_reads_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("agents.json");
        ready().write(&file, &[]).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        fs::write(&file, text.replace("\"model\"", "\"mode\"")).unwrap();
        assert!(Results::load(&file).is_err());
    }

    #[test]
    fn the_model_and_the_version_are_taken_from_the_first_run_and_then_insisted_on() {
        let mut results = started();
        let mut init = expected(Setup::Plain);
        results.accept(&init).unwrap();
        assert_eq!(results.model, "claude-test-1");
        assert_eq!(results.claude_version, "9.9.9");
        results.accept(&init).unwrap();

        init.model = "claude-other-2".to_owned();
        let error = results.accept(&init).unwrap_err().to_string();
        assert!(
            error.contains("claude-test-1") && error.contains("claude-other-2"),
            "{error}"
        );
        init.model = "claude-test-1".to_owned();
        init.claude_code_version = "10.0.0".to_owned();
        let error = results.accept(&init).unwrap_err().to_string();
        assert!(
            error.contains("9.9.9") && error.contains("10.0.0"),
            "{error}"
        );
    }

    #[test]
    fn a_task_started_over_adds_only_to_results_of_the_same_questions_and_tools() {
        let results = ready();
        let file = Path::new("agents.json");
        results
            .ensure_continues(file, &results.questions_sha256, &results.tools)
            .unwrap();

        let error = results
            .ensure_continues(file, &"b".repeat(64), &results.tools)
            .unwrap_err()
            .to_string();
        assert!(error.contains("other questions"), "{error}");

        let other = [ToolVersion {
            tool: "gsttt".to_owned(),
            version: "2.0.0".to_owned(),
        }];
        let error = results
            .ensure_continues(file, &results.questions_sha256, &other)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("gsttt 1.0.0") && error.contains("gsttt 2.0.0"),
            "{error}"
        );
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A told run and an installed run of one question are two runs"
    )]
    #[test]
    fn a_run_is_held_by_its_question_setup_arm_and_repeat() {
        let mut results = ready();
        assert!(results.holds("vite-users-1", Setup::Plain, Arm::Installed, 1));
        assert!(!results.holds("vite-users-1", Setup::Lsp, Arm::Installed, 1));
        assert!(!results.holds("vite-users-1", Setup::Plain, Arm::Installed, 2));
        assert!(!results.holds("vite-users-2", Setup::Plain, Arm::Installed, 1));
        assert_eq!(results.runs[0].tokens(), 118);

        // The installed run of a setup does not stand for its told run, and the other way round.
        let mut lsp = sample("vite-users-1", Kind::Users, Setup::Lsp, 1);
        results.runs.push(lsp.clone());
        assert!(results.holds("vite-users-1", Setup::Lsp, Arm::Installed, 1));
        assert!(!results.holds("vite-users-1", Setup::Lsp, Arm::Told, 1));
        lsp.arm = Arm::Told;
        results.runs.push(lsp);
        assert!(results.holds("vite-users-1", Setup::Lsp, Arm::Told, 1));
        assert!(!results.holds("vite-users-1", Setup::Lsp, Arm::Told, 2));

        // Both are kept, because the file refuses only the same run twice.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("agents.json");
        results.write(&file, &[]).unwrap();
        assert_eq!(Results::load(&file).unwrap(), results);
        results.runs.push(results.runs[2].clone());
        let error = results.write(&file, &[]).unwrap_err().to_string();
        assert!(
            error.contains("lsp (told)") && error.contains("twice"),
            "{error}"
        );
    }
}
