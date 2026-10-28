//! `cargo xtask agents`: the questions the agent benchmarks ask. Specified in `openspec/changes/add-agent-benchmarks/`.
//!
//! `questions` draws them from the repositories that ask for them in `bench/bench.toml` and writes `bench/agents/questions.json`, the file the runs read. The answers come from the TypeScript language service for the users of a function and from Vitest's own report for the tests it breaks, so no tool under test marks its own work.

mod args;
pub(crate) mod checkout;
mod draw;
mod functions;
mod oracle;
pub(crate) mod questions;

use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context as _, bail, ensure};

use self::functions::Candidate;
use self::oracle::Oracle;
use self::questions::{Question, Questions};
use crate::bench::pins::{Pins, Repository};
use crate::bench::plan::Programs;
use crate::bench::results::write_atomically;
use crate::bench::{select, setup};
use crate::root::repository_root;

const USAGE: &str = "usage: cargo xtask agents questions [--repository <name>]... [--out <file>]";

/// Where the questions are written, relative to the root of the repository.
pub(crate) const QUESTIONS: &str = "bench/agents/questions.json";

/// Runs `cargo xtask agents <task>`.
pub(crate) fn run(args: &[String]) -> anyhow::Result<ExitCode> {
    match args.split_first() {
        Some((task, rest)) if task == "questions" => draw(rest),
        _ => bail!("{USAGE}"),
    }
}

/// Draws the questions of the repositories that ask for them and writes them.
fn draw(args: &[String]) -> anyhow::Result<ExitCode> {
    let args = args::parse(args)?;
    let root = repository_root()?;
    let pins = Pins::load(&root)?;
    let repositories = asking(&pins, &args.repositories)?;

    // The Node tools come first: a missing pnpm should fail before the first clone.
    let programs = Programs::new(&root.join("bench/tools"), Path::new(""));
    setup::node_tools(&programs, &pins)?;
    let script = root.join("bench/agents/functions.mjs");

    let mut questions = Vec::new();
    for repository in repositories {
        questions.extend(questions_of(
            &pins.agents.seed,
            repository,
            &programs,
            &script,
        )?);
    }
    let questions = Questions {
        seed: pins.agents.seed.clone(),
        questions,
    };
    let out = args.out.unwrap_or_else(|| root.join(QUESTIONS));
    write_atomically(&out, &questions.render()?)?;
    // What was written is read back through the loader the runs use.
    ensure!(
        Questions::load(&out)? == questions,
        "{} was read back as other questions than the ones drawn",
        out.display()
    );
    writeln!(
        std::io::stdout().lock(),
        "{} questions written to {}",
        questions.questions.len(),
        out.display()
    )?;
    Ok(ExitCode::SUCCESS)
}

/// The repositories that ask for questions: the named ones, or every one that has an `agents` table.
fn asking<'a>(pins: &'a Pins, names: &[String]) -> anyhow::Result<Vec<&'a Repository>> {
    let chosen = select(pins, names)?;
    if let Some(repository) = chosen
        .iter()
        .find(|repository| !names.is_empty() && repository.agents.is_none())
    {
        bail!(
            "`{}` asks for no agent questions: it has no `agents` table in bench/bench.toml",
            repository.name
        );
    }
    let asking: Vec<&Repository> = chosen
        .into_iter()
        .filter(|repository| repository.agents.is_some())
        .collect();
    ensure!(
        !asking.is_empty(),
        "no repository asks for agent questions in bench/bench.toml"
    );
    Ok(asking)
}

/// The questions about one repository: its checkout is prepared, its functions are listed, and the users questions and then the tests questions are drawn.
fn questions_of(
    seed: &str,
    repository: &Repository,
    programs: &Programs,
    script: &Path,
) -> anyhow::Result<Vec<Question>> {
    let name = repository.name.as_str();
    let agents = repository
        .agents
        .as_ref()
        .with_context(|| format!("`{name}` has no `agents` table"))?;
    let tsconfig = repository
        .tsconfig
        .as_deref()
        .with_context(|| format!("`{name}` has no tsconfig"))?;
    let checkout = checkout::prepare(repository, agents)?;
    let candidates = functions::list(script, &programs.node, &checkout, &agents.scope, tsconfig)?;
    writeln!(
        std::io::stderr().lock(),
        "{name}: {} exported functions under {}",
        candidates.len(),
        agents.scope
    )?;

    let order = draw::order(seed, name, &candidates);
    let mut questions = draw::users(name, &agents.scope, agents.users, &order)?;
    if agents.tests() == 0 {
        return Ok(questions);
    }

    let test = agents.test.as_deref().with_context(|| {
        format!("`{name}` asks for questions about tests and names no test command")
    })?;
    let oracle = Oracle::new(&checkout, test, &checkout::scratch(name))?;
    writeln!(std::io::stderr().lock(), "{name}: {}", oracle.describe())?;
    let with_body: Vec<&Candidate> = order
        .iter()
        .copied()
        .filter(|candidate| candidate.body_offset.is_some())
        .collect();
    questions.extend(draw::tests(
        name,
        &agents.scope,
        agents.tests(),
        test,
        oracle.ignored(),
        &with_body,
        |candidate| {
            let verdict = oracle.answer(candidate)?;
            let said = match &verdict {
                Ok(answer) => format!("{} test files", answer.len()),
                Err(why) => (*why).to_owned(),
            };
            writeln!(
                std::io::stderr().lock(),
                "{name} tests: {} {}: {said}",
                candidate.file,
                candidate.name
            )?;
            Ok(verdict)
        },
    )?);
    Ok(questions)
}
