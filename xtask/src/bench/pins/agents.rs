//! The pins of the agent runs: the `agents` table of `bench/bench.toml`, and the `agents` table a repository carries when questions are drawn about it.

use anyhow::ensure;
use serde::Deserialize;

use super::{Language, Repository, is_inside};

/// The fewest seconds a run can be given: less than a minute is not enough for Claude Code to start, read and answer.
const MIN_SECONDS: u64 = 60;

/// What every agent run shares.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Agents {
    /// The model Claude Code is started with, as the alias it takes.
    pub(crate) model: String,
    /// How many times every question is asked in every setup.
    pub(crate) repeats: u32,
    /// Orders the candidates of the questions: another seed draws other questions.
    pub(crate) seed: String,
    /// The wall-clock limit of one run, in seconds.
    pub(crate) seconds: u64,
    /// The limit of turns of one run.
    pub(crate) turns: u32,
}

impl Agents {
    pub(super) fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            !self.model.is_empty() && !self.model.contains(char::is_whitespace),
            "the model of the agent runs must be one word, not `{}`",
            self.model
        );
        ensure!(self.repeats >= 1, "the agent runs need at least one repeat");
        ensure!(!self.seed.trim().is_empty(), "the agent runs need a seed");
        ensure!(
            self.seconds >= MIN_SECONDS,
            "the limit of an agent run is {} seconds, but it must be at least {MIN_SECONDS}",
            self.seconds
        );
        ensure!(
            self.turns >= 1,
            "the agent runs need a limit of at least one turn"
        );
        Ok(())
    }
}

/// What the questions about one repository are drawn from, and how the repository is prepared for them.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepositoryAgents {
    /// The directory the questions are about, relative to the checkout: `.` for all of it.
    pub(crate) scope: String,
    /// How many questions about the files that refer to a function are drawn.
    pub(crate) users: u32,
    /// How many questions about the test files that break with a function are drawn; none when absent.
    pub(crate) tests: Option<u32>,
    /// The command that installs the checkout, as a program and its arguments.
    pub(crate) install: Option<Vec<String>>,
    /// The command that builds what the unit tests import, for a repository whose tests start only from a built package or import their own build output: the draw runs it once after the install, and the oracle runs it from the tree as it is right before every run of the test command, a run with a `throw` included, so that the tests read a build that holds the throw.
    pub(crate) build: Option<Vec<String>>,
    /// The command that runs the unit tests from the root of the checkout; the task appends the reporter flags.
    pub(crate) test: Option<Vec<String>>,
}

impl RepositoryAgents {
    /// How many questions about tests are drawn.
    pub(crate) fn tests(&self) -> u32 {
        self.tests.unwrap_or(0)
    }

    pub(super) fn validate(&self, repository: &Repository) -> anyhow::Result<()> {
        let name = &repository.name;
        ensure!(
            repository.language == Language::Typescript && repository.tsconfig.is_some(),
            "`{name}` asks for agent questions, which need a TypeScript repository with a tsconfig"
        );
        ensure!(
            is_scope(&self.scope),
            "the scope `{}` of `{name}` is not a directory inside the checkout",
            self.scope
        );
        ensure!(
            self.users >= 1,
            "`{name}` must ask for at least one question about users"
        );
        if let Some(tests) = self.tests {
            ensure!(
                tests >= 1,
                "`{name}` asks for no question about tests; leave `tests` out instead"
            );
            ensure!(
                self.test.is_some(),
                "`{name}` asks for questions about tests and names no test command"
            );
        }
        for (what, command) in [
            ("install", &self.install),
            ("build", &self.build),
            ("test", &self.test),
        ] {
            if let Some(command) = command {
                ensure!(
                    !command.is_empty() && command.iter().all(|word| !word.is_empty()),
                    "the {what} command of `{name}` must be a program and its arguments, none of them empty"
                );
            }
        }
        Ok(())
    }
}

/// Whether `text` is `.` or a relative directory with no empty or `.` part.
fn is_scope(text: &str) -> bool {
    text == "." || (is_inside(text) && text.split('/').all(|part| !part.is_empty() && part != "."))
}
