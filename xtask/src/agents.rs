//! `cargo xtask agents`: the agent benchmarks, which ask Claude Code questions about a repository in four setups and score the answers. Specified in `openspec/changes/add-agent-benchmarks/`.
//!
//! `questions` draws them from the repositories that ask for them in `bench/bench.toml` and writes `bench/agents/questions.json`, the file the runs read. The answers come from the TypeScript language service for the users of a function, from it and the `graphql` package for the clients of a method a resolver calls, and from Vitest's own report for the tests a function breaks, so no tool under test marks its own work.
//!
//! `run` asks them, one run at a time, and writes `bench/results/agents.json` after every run. `table` prints the means of a results file.

mod args;
pub(crate) mod checkout;
mod draw;
mod exec;
mod functions;
mod guard;
mod indexes;
mod init;
mod oracle;
mod process;
pub(crate) mod questions;
mod results;
mod runs;
mod score;
mod sessions;
mod setups;
mod stream;
mod table;

use std::fs::{File, TryLockError};
use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context as _, bail, ensure};

use self::args::Task;
use self::checkout::Purpose;
use self::functions::Candidate;
use self::oracle::Oracle;
use self::process::Stop;
use self::questions::{Question, Questions};
use crate::bench::pins::{Pins, Repository, RepositoryAgents};
use crate::bench::plan::Programs;
use crate::bench::results::write_atomically;
use crate::bench::{pinned, setup};
use crate::root::repository_root;

const USAGE: &str = "usage:
  cargo xtask agents tools
  cargo xtask agents questions [--repository <name>]... [--out <file>]
  cargo xtask agents run --gsttt <binary> [--repository <name>]... [--question <id>]... [--repeats <n>] [--out <file>]
  cargo xtask agents table [<results file>]";

/// Where the questions are written, relative to the root of the repository.
pub(crate) const QUESTIONS: &str = "bench/agents/questions.json";

/// Where the results of the runs are written, relative to the root of the repository.
pub(crate) const RESULTS: &str = "bench/results/agents.json";

/// Writes a line of progress to the standard error, where the standard output stays the table or the path a script reads.
fn say(text: &str) -> anyhow::Result<()> {
    writeln!(std::io::stderr().lock(), "{text}")?;
    Ok(())
}

/// Runs `cargo xtask agents <task>`. `tools`, `questions` and `run` hold the lock of [`one_task`] to their end; `table` only reads and takes none.
pub(crate) fn run(args: &[String]) -> anyhow::Result<ExitCode> {
    match args.split_first() {
        Some((task, rest)) if task == "tools" => {
            ensure!(rest.is_empty(), "`agents tools` takes no argument");
            let _held = one_task(&std::env::temp_dir())?;
            tools()
        }
        Some((task, rest)) if task == "questions" => {
            let _held = one_task(&std::env::temp_dir())?;
            draw(rest)
        }
        Some((task, rest)) if task == "run" => {
            let _held = one_task(&std::env::temp_dir())?;
            runs::run(rest)
        }
        Some((task, rest)) if task == "table" => table::run(rest),
        _ => bail!("{USAGE}"),
    }
}

/// Lets one `questions` or `run` task at a time use the fixed directories under the system's temporary directory, which a second task would empty under the first and sweep for processes; the system frees the lock when the process ends, however it ends.
fn one_task(dir: &Path) -> anyhow::Result<File> {
    let path = dir.join("gsttt-agents.lock");
    let file = File::create(&path).with_context(|| format!("cannot create {}", path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => {
            bail!("another `cargo xtask agents` task holds {}", path.display())
        }
        Err(TryLockError::Error(error)) => {
            Err(error).with_context(|| format!("cannot lock {}", path.display()))
        }
    }
}

/// The programs of the Node tools in `bench/tools`, which the scripts of `bench/agents/` and their tests read; no binary under test.
fn node_programs(root: &Path) -> Programs {
    Programs::new(&root.join("bench/tools"), Path::new(""))
}

/// Installs the Node tools of the pins into `bench/tools/node` and nothing else, so that CI and the tests that run the scripts have them without a clone or a draw.
fn tools() -> anyhow::Result<ExitCode> {
    let root = repository_root()?;
    let pins = Pins::load(&root)?;
    setup::node_tools(&node_programs(&root), &pins)?;
    Ok(ExitCode::SUCCESS)
}

/// Draws the questions of the repositories that ask for them and writes them.
fn draw(args: &[String]) -> anyhow::Result<ExitCode> {
    let args = args::parse(Task::Questions, args)?;
    let root = repository_root()?;
    let pins = Pins::load(&root)?;
    let repositories = asking(&pins, &args.repositories)?;

    // The Node tools come first: a missing pnpm should fail before the first clone.
    let programs = node_programs(&root);
    setup::node_tools(&programs, &pins)?;
    let scripts = root.join("bench/agents");

    let mut questions = Vec::new();
    for repository in repositories {
        questions.extend(questions_of(
            &pins.agents.seed,
            repository,
            &programs,
            &scripts,
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
    let chosen = pinned(pins, names)?;
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

/// The questions about one repository: its checkout is prepared, and the questions of each kind it asks for are drawn, `users` first, then `tests`, then `clients`.
fn questions_of(
    seed: &str,
    repository: &Repository,
    programs: &Programs,
    scripts: &Path,
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
    let checkout = checkout::prepare(repository, agents, Purpose::Draw)?;

    let mut questions = Vec::new();
    if agents.users() > 0 || agents.tests() > 0 {
        let script = scripts.join("functions.mjs");
        let candidates =
            functions::list(&script, &programs.node, &checkout, &agents.scope, tsconfig)?;
        say(&format!(
            "{name}: {} exported functions under {}",
            candidates.len(),
            agents.scope
        ))?;
        let order = draw::order(seed, name, &candidates);
        if agents.users() > 0 {
            questions.extend(draw::users(name, &agents.scope, agents.users(), &order)?);
        }
        if agents.tests() > 0 {
            questions.extend(tests_questions(name, agents, &checkout, &order)?);
        }
    }
    if agents.clients() > 0 {
        let script = scripts.join("clients.mjs");
        let methods = functions::list(&script, &programs.node, &checkout, &agents.scope, tsconfig)?;
        say(&format!(
            "{name}: {} methods that a resolver calls under {}",
            methods.len(),
            agents.scope
        ))?;
        let order = draw::order(seed, name, &methods);
        questions.extend(draw::clients(
            name,
            &agents.scope,
            agents.clients(),
            &order,
        )?);
    }
    Ok(questions)
}

/// The questions about the tests the functions in `order` break, which the oracle finds by running the tests of the checkout with a throw in each.
fn tests_questions(
    name: &str,
    agents: &RepositoryAgents,
    checkout: &Path,
    order: &[&Candidate],
) -> anyhow::Result<Vec<Question>> {
    let test = agents.test.as_deref().with_context(|| {
        format!("`{name}` asks for questions about tests and names no test command")
    })?;
    // Ctrl-C is caught from the first wait on: before it, a signal ends the task at once, as it always did.
    let stop = Stop::catch()?;
    let work = checkout::scratch(name);
    let oracle = Oracle::new(checkout, agents.build.as_deref(), test, &work, &stop)?;
    say(&format!("{name}: {}", oracle.describe()))?;
    let with_body: Vec<&Candidate> = order
        .iter()
        .copied()
        .filter(|candidate| candidate.body_offset.is_some())
        .collect();
    draw::tests(
        name,
        &agents.scope,
        agents.tests(),
        test,
        oracle.ignored(),
        &with_body,
        |candidate| {
            let verdict = oracle.answer(candidate)?;
            let said = match &verdict {
                Ok(answer) => format!("{} test files", answer.files.len()),
                Err(why) => (*why).to_owned(),
            };
            say(&format!(
                "{name} tests: {} {}: {said}",
                candidate.file, candidate.name
            ))?;
            Ok(verdict)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{Pins, USAGE, asking, one_task, repository_root, run};

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A second task is refused while one holds the lock"
    )]
    #[test]
    fn a_second_task_is_refused_while_one_holds_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let lock = dir.path().join("gsttt-agents.lock");
        let first = one_task(dir.path()).unwrap();

        let error = one_task(dir.path()).unwrap_err().to_string();
        assert!(error.contains(&lock.display().to_string()), "{error}");
        assert!(error.contains("another"), "{error}");

        drop(first);
        one_task(dir.path()).unwrap();
    }

    #[test]
    #[test]
    fn the_tools_task_takes_no_argument_and_the_usage_names_it() {
        let error = run(&["tools".to_owned(), "--out".to_owned()])
            .unwrap_err()
            .to_string();
        assert!(error.contains("takes no argument"), "{error}");
        assert!(USAGE.contains("cargo xtask agents tools"));
    }

    fn the_agent_benchmarks_ask_about_a_repository_the_speed_benchmark_leaves_out() {
        let pins = Pins::load(&repository_root().unwrap()).unwrap();
        let asking_names: Vec<&str> = asking(&pins, &[])
            .unwrap()
            .iter()
            .map(|repository| repository.name.as_str())
            .collect();
        assert!(asking_names.contains(&"hoppscotch"), "{asking_names:?}");

        // A repository without an `agents` table cannot be named, and says so.
        let error = asking(&pins, &["django".to_owned()])
            .unwrap_err()
            .to_string();
        assert!(error.contains("`django`"), "{error}");
    }
}
