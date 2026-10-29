//! `cargo xtask agents run`: every question asked in every setup, one run at a time, with the results written after each run.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context as _, bail, ensure};
use xshell::Shell;

use super::args::{self, Task};
use super::checkout::{self, Purpose};
use super::exec::{self, Launch};
use super::guard::Guard;
use super::indexes::Indexes;
use super::init::{self, UserEntries};
use super::process::{Stop, which};
use super::questions::{Question, Questions};
use super::results::{Outcome, Results, Run, ToolVersion};
use super::score::{Score, last_files_block, normalize, score};
use super::sessions;
use super::setups::{self, Arm, Context, Invocation, RunDirs, Setup};
use super::stream::{self, End, Observed};
use super::table;
use super::{QUESTIONS, RESULTS, asking, say};
use crate::bench::pins::{Pins, Repository, Tool};
use crate::bench::plan::{self, Programs};
use crate::bench::{hyperfine, machine, setup};
use crate::digest::{sha256_file, sha256_hex};
use crate::dist::git::git;
use crate::root::repository_root;

/// The packages whose versions are recorded beside the tools: the language server and the TypeScript it reads with.
const NODE_PACKAGES: [&str; 2] = ["typescript", "typescript-language-server"];

/// A checkout the runs are made in.
#[derive(Debug)]
struct Checkout {
    dir: PathBuf,
    /// The names the checkout has. An answer may use any of them, and the temporary directory of macOS is behind a link.
    roots: Vec<PathBuf>,
}

/// One run of the task: a question in a setup and an arm of it at a repeat.
#[derive(Clone, Copy, Debug)]
struct Planned<'a> {
    question: &'a Question,
    setup: Setup,
    arm: Arm,
    repeat: u32,
}

/// The runs in the order they are made: by repeat, then by question, then by setup and its arms. A slow hour of the API, or a limit that stops the task, then falls on every setup and arm alike.
fn order<'a>(questions: &[&'a Question], repeats: u32) -> Vec<Planned<'a>> {
    let mut planned = Vec::new();
    for repeat in 1..=repeats {
        for &question in questions {
            for (setup, arm) in setups::every_arm() {
                planned.push(Planned {
                    question,
                    setup,
                    arm,
                    repeat,
                });
            }
        }
    }
    planned
}

/// How a run ended, what it answered, and how right that was.
#[derive(Debug)]
struct Judged {
    outcome: Outcome,
    answer: Vec<String>,
    score: Score,
}

/// Judges a run from its stream. `None` when it failed on its own: it ended in an error, or without a result and not at our limit. That is not a result of the setup but a fault of the harness or of the API, and the task stops on it.
fn judge(
    question: &Question,
    roots: &[PathBuf],
    timed_out: bool,
    observed: &Observed,
) -> Option<Judged> {
    let nothing = |outcome| Judged {
        outcome,
        answer: Vec::new(),
        score: Score::ZERO,
    };
    if timed_out {
        return Some(nothing(Outcome::Timeout));
    }
    match observed.end {
        End::MaxTurns => Some(nothing(Outcome::TurnLimit)),
        End::Success => Some(
            match observed.message.as_deref().and_then(last_files_block) {
                Some(lines) => {
                    let answer = normalize(&lines, roots);
                    let scored = score(&answer, &question.answer, &question.ignored);
                    Judged {
                        outcome: Outcome::Answered,
                        answer,
                        score: scored,
                    }
                }
                None => nothing(Outcome::NoAnswer),
            },
        ),
        End::Failed | End::Unfinished => None,
    }
}

/// How the command line of a run is made once the directories of the run are known: the sandbox of a run names its checkout and its temporary directory, so it cannot be made before them.
struct Invoke<'a>(&'a dyn Fn(Setup, Arm, RunDirs<'_>) -> anyhow::Result<Invocation>);

impl fmt::Debug for Invoke<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Invoke").finish_non_exhaustive()
    }
}

/// What the runs of a task share.
#[derive(Debug)]
struct Session<'a> {
    claude: &'a Path,
    questions: &'a [&'a Question],
    repeats: u32,
    checkouts: &'a BTreeMap<String, Checkout>,
    /// Makes the command line of a run.
    invoke: Invoke<'a>,
    /// The directory the temporary directory of every run is made in.
    tmp_root: &'a Path,
    /// The temporary root that every Claude Code session of the user shares: the directory the session of a run made in it is removed after the run.
    sessions: &'a Path,
    user: &'a UserEntries,
    /// The indexes as the runs began, which the run of their setup puts back.
    indexes: &'a Indexes,
    /// Where the programs of the tools are: the daemon of codebase-memory-mcp is stopped with them before its cache is put back.
    programs: &'a Programs,
    /// The wall-clock limit of one run.
    limit: Duration,
    /// Ctrl-C, SIGTERM and SIGHUP on the task: a run that waits for one is ended, and not recorded.
    stop: &'a Stop,
    /// The results file.
    out: &'a Path,
    /// The directory the stream of every run is kept in.
    transcripts: &'a Path,
    /// Paths of this machine that must not reach the results.
    local: &'a [String],
}

impl Session<'_> {
    /// Makes the runs the results do not hold yet, in order, and writes the results after each. Returns how many it made. A run that ends on the usage limit of the account is not recorded, and stops the task.
    fn run_all(&self, results: &mut Results) -> anyhow::Result<usize> {
        let planned = order(self.questions, self.repeats);
        let total = planned.len();
        let local: Vec<&str> = self.local.iter().map(String::as_str).collect();
        let mut made = 0;
        for (index, run) in planned.into_iter().enumerate() {
            if results.holds(&run.question.id, run.setup, run.arm, run.repeat) {
                continue;
            }
            let label = format!(
                "[{}/{total}] {} {} {} repeat {}",
                index + 1,
                run.question.id,
                run.setup,
                run.arm,
                run.repeat
            );
            say(&format!("{label}: started"))?;
            let done = self.one(run, results)?;
            let cost = done
                .cost_usd
                .map_or_else(|| "no cost".to_owned(), |cost| format!("${cost:.3}"));
            let report = format!(
                "{label}: {}, recall {:.2}, {} turns, {} s, {cost}",
                done.outcome.name(),
                done.recall,
                done.turns,
                done.duration_ms / 1000
            );
            // A run is kept and reported once the results with it in them are written: one that cannot be written down is not kept, and the task stops on it.
            results.runs.push(done);
            if let Err(error) = results.write(self.out, &local) {
                results.runs.pop();
                return Err(error);
            }
            say(&report)?;
            made += 1;
        }
        Ok(made)
    }

    /// One run: the process, then the checkout put back, then the stream judged.
    fn one(&self, planned: Planned, results: &mut Results) -> anyhow::Result<Run> {
        let Planned {
            question,
            setup,
            arm,
            repeat,
        } = planned;
        let what = format!(
            "the run of {} in the `{setup}` setup, {arm} (repeat {repeat})",
            question.id
        );
        let checkout = self
            .checkouts
            .get(&question.repository)
            .with_context(|| format!("`{}` has no checkout", question.repository))?;
        // The temporary directory of the run is removed when this function returns, whatever became of the run.
        let tmp = tempfile::Builder::new()
            .prefix("r-")
            .tempdir_in(self.tmp_root)
            .with_context(|| {
                format!(
                    "cannot make a temporary directory in {}",
                    self.tmp_root.display()
                )
            })?;
        let dirs = RunDirs {
            checkout: &checkout.dir,
            tmp: tmp.path(),
        };
        let invocation = (self.invoke.0)(setup, arm, dirs)
            .with_context(|| format!("cannot make the command line of {what}"))?;
        let transcript = self
            .transcripts
            .join(format!("{}-{setup}-{arm}-{repeat}.jsonl", question.id));

        let guard = Guard::new(&checkout.dir)?;
        let launch = Launch {
            claude: self.claude,
            invocation: &invocation,
            dir: &checkout.dir,
            tmp: tmp.path(),
            prompt: &question.prompt,
            transcript: &transcript,
            limit: self.limit,
            stop: self.stop,
        };
        // The init event of the run is held against the setup and against the runs before it as soon as it is written, which is after the first request of the run is sent: a run it stops has spent that request. Its session is noted first, so that the run the task stops on is swept as well.
        let mut session = None;
        let executed = exec::execute(&launch, &mut |init| {
            session = Some(init.session_id.clone());
            init::check(setup, init, self.user)?;
            results.accept(init)
        });
        // The checkout and the index of the setup are put back, and the directory of the session is removed, whatever became of the run.
        let restored = guard.finish();
        let index = put_back_index(self.indexes, self.programs, setup, &question.repository);
        let swept = session.map_or(Ok(()), |id| sessions::remove(self.sessions, &id));
        let finished = executed.with_context(|| what.clone())?;
        let changed_checkout = restored.with_context(|| what.clone())?;
        let changed_index = index.with_context(|| what.clone())?;
        swept.with_context(|| what.clone())?;

        let bytes = fs::read(&transcript)
            .with_context(|| format!("cannot read {}", transcript.display()))?;
        // A run that was killed may have been cut in the middle of a character.
        let observed = stream::observe(&String::from_utf8_lossy(&bytes))
            .with_context(|| format!("{what}, whose stream is in {}", transcript.display()))?;
        ensure!(
            !observed.usage_limit,
            "the usage limit of the account was reached in {what}, which is not recorded; the {} runs before it are in {}, and the same command continues from there once the limit is over",
            results.runs.len(),
            self.out.display()
        );
        let Some(judged) = judge(question, &checkout.roots, finished.timed_out, &observed) else {
            bail!(
                "{what} ended {} without a result; its stream is in {} and its standard error says: {}",
                finished.exit.map_or_else(
                    || "at our hands".to_owned(),
                    |status| format!("with {status}")
                ),
                transcript.display(),
                exec::stderr_tail(&transcript, 400)
            );
        };
        Ok(Run {
            question: question.id.clone(),
            kind: question.kind,
            setup,
            arm,
            repeat,
            outcome: judged.outcome,
            answer: judged.answer,
            precision: judged.score.precision,
            recall: judged.score.recall,
            f1: judged.score.f1,
            input_tokens: observed.tokens.input,
            output_tokens: observed.tokens.output,
            cache_read_tokens: observed.tokens.cache_read,
            cache_write_tokens: observed.tokens.cache_write,
            cost_usd: observed.cost_usd,
            turns: observed.turns,
            tool_calls: observed.tool_calls,
            tool_errors: observed.tool_errors,
            duration_ms: u64::try_from(finished.elapsed.as_millis()).unwrap_or(u64::MAX),
            changed_checkout: changed_checkout || changed_index,
        })
    }
}

/// The questions of the repositories named, in the order of the questions file, or only those named by `ids`. An id that no question has, or that is about another repository, is an error.
fn choose<'a>(
    all: &'a [Question],
    repositories: &[&str],
    ids: &[String],
) -> anyhow::Result<Vec<&'a Question>> {
    for id in ids {
        let question = all
            .iter()
            .find(|question| &question.id == id)
            .with_context(|| format!("no question is called `{id}` in {QUESTIONS}"))?;
        ensure!(
            repositories.contains(&question.repository.as_str()),
            "the question `{id}` is about `{}`, which is not among the repositories chosen",
            question.repository
        );
    }
    let chosen: Vec<&Question> = all
        .iter()
        .filter(|question| repositories.contains(&question.repository.as_str()))
        .filter(|question| ids.is_empty() || ids.contains(&question.id))
        .collect();
    ensure!(
        !chosen.is_empty(),
        "no question of the repositories chosen is in {QUESTIONS}"
    );
    Ok(chosen)
}

/// The versions of the tools the runs use, by name: the binary under test with the sum of its bytes, codebase-memory-mcp, and the language server with its TypeScript.
fn tool_versions(programs: &Programs) -> anyhow::Result<Vec<ToolVersion>> {
    let mut tools = Vec::new();
    for tool in [Tool::Gsttt, Tool::CodebaseMemory] {
        tools.push(ToolVersion {
            tool: tool.name().to_owned(),
            version: setup::installed_version(tool, programs)?,
        });
    }
    for package in NODE_PACKAGES {
        let version = setup::node_version(&programs.node, package).with_context(|| {
            format!("{package} is not installed in {}", programs.node.display())
        })?;
        tools.push(ToolVersion {
            tool: package.to_owned(),
            version,
        });
    }
    tools.push(binary_sum(&programs.of(Tool::Gsttt))?);
    tools.sort_by(|first, second| first.tool.cmp(&second.tool));
    Ok(tools)
}

/// The SHA-256 of the bytes of `binary`, as a tool of its own: two builds of one version answer `--version` alike, and the results of one must not be continued with the other.
fn binary_sum(binary: &Path) -> anyhow::Result<ToolVersion> {
    Ok(ToolVersion {
        tool: "gsttt-sha256".to_owned(),
        version: sha256_file(binary)?,
    })
}

/// The directories no run may read: `root`, the main worktree it was made from when it is a linked one, which holds the same sources, and `config`. The main worktree is the parent of the common directory of git.
fn private_dirs(root: &Path, config: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let common = git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let mut private = vec![root.to_path_buf(), config.to_path_buf()];
    if let Some(main) = Path::new(&common).parent().filter(|main| *main != root) {
        private.push(main.to_path_buf());
    }
    Ok(private)
}

/// The results to add to: the ones in `out` when it exists, which must be for the same questions and the same tools, or new ones.
fn open(out: &Path, questions_sha256: &str, tools: Vec<ToolVersion>) -> anyhow::Result<Results> {
    if !out.exists() {
        return Ok(Results::start(
            machine::now_utc()?,
            machine::describe()?,
            questions_sha256.to_owned(),
            tools,
        ));
    }
    let results = Results::load(out)?;
    results.ensure_continues(out, questions_sha256, &tools)?;
    Ok(results)
}

/// Throws the cache of `tool` away from `dir` with the line that every cold run of the speed benchmark starts with, in the environment `tool` runs with. The daemon of codebase-memory-mcp is stopped first and a stop that fails is an error: the line goes on after one, and the cache would be thrown away under a live daemon.
fn forget(tool: Tool, dir: &Path, programs: &Programs) -> anyhow::Result<()> {
    let Some(line) = plan::reset(tool, programs) else {
        return Ok(());
    };
    if tool == Tool::CodebaseMemory {
        stop_daemon(programs)?;
    }
    shell_line(tool, &line, dir, programs)
        .with_context(|| format!("cannot throw the cache of {} away", tool.name()))
}

/// Puts back the index that a run of `setup` in `repository` could have changed. The daemon of codebase-memory-mcp keeps its index open, so it is stopped before its cache is put back.
fn put_back_index(
    indexes: &Indexes,
    programs: &Programs,
    setup: Setup,
    repository: &str,
) -> anyhow::Result<bool> {
    if setup == Setup::Cbm {
        stop_daemon(programs)?;
    }
    indexes.restore(setup, repository)
}

/// Stops the daemon of codebase-memory-mcp that has the cache of `programs` open. Stopping it when none runs is not an error (`daemon stop` says so and ends with 0), so a stop that fails is one.
fn stop_daemon(programs: &Programs) -> anyhow::Result<()> {
    shell_line(
        Tool::CodebaseMemory,
        &plan::stop_daemon(programs),
        &programs.cbm,
        programs,
    )
    .context("cannot stop the daemon of codebase-memory-mcp")
}

/// Runs `line` with `sh -c` in `dir`, in the environment `tool` is isolated in, which is how a tool finds its daemon and its cache. What the line prints is held back and goes into the error when it fails.
fn shell_line(tool: Tool, line: &str, dir: &Path, programs: &Programs) -> anyhow::Result<()> {
    let shell = Shell::new()?;
    shell.change_dir(dir);
    let mut command = shell.cmd("sh").args(["-c", line]).quiet().ignore_status();
    for (key, value) in plan::isolation(tool, programs) {
        command = command.env(key, value);
    }
    let output = command.output()?;
    ensure!(
        output.status.success(),
        "`{line}` ended with {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

/// Indexes the checkout with the two tools that keep an index. The index of gsttt in the checkout is from an earlier task and may be of another binary, so it is thrown away first.
fn index(repository: &Repository, dir: &Path, programs: &Programs) -> anyhow::Result<()> {
    forget(Tool::Gsttt, dir, programs)?;
    let shell = Shell::new()?;
    shell.change_dir(dir);
    let indexing = plan::plan(repository, programs)
        .into_iter()
        .filter(|run| matches!(run.tool, Tool::Gsttt | Tool::CodebaseMemory));
    for run in indexing {
        let command = shell.cmd(&run.command.program).args(&run.command.args);
        hyperfine::environment(command, &run)
            .quiet()
            .ignore_stdout()
            .run()
            .with_context(|| {
                format!("{} could not index `{}`", run.tool.name(), repository.name)
            })?;
    }
    Ok(())
}

/// The paths of this machine the results must not hold: the repository, the checkouts of the runs, the temporary directory and the home of the user. A path with a single directory in it, such as `/tmp`, is left out, since it would match too much.
fn local_paths(root: &Path, checkouts: &BTreeMap<String, Checkout>) -> Vec<String> {
    let mut paths = vec![
        root.to_path_buf(),
        checkout::root(Purpose::Runs),
        std::env::temp_dir(),
    ];
    paths.extend(std::env::var_os("HOME").map(PathBuf::from));
    paths.extend(
        checkouts
            .values()
            .flat_map(|checkout| checkout.roots.iter().cloned()),
    );
    let mut paths: Vec<String> = paths
        .into_iter()
        .filter(|path| {
            path.components()
                .filter(|part| matches!(part, Component::Normal(_)))
                .count()
                >= 2
        })
        .map(|path| path.to_string_lossy().trim_end_matches('/').to_owned())
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

/// Runs `cargo xtask agents run`.
pub(super) fn run(args: &[String]) -> anyhow::Result<ExitCode> {
    let args = args::parse(Task::Run, args)?;
    let binary = args.gsttt.context("--gsttt is needed")?;
    ensure!(
        binary.is_file(),
        "{} is not a file; build the binary under test first, with `cargo build --release -p gsttt`",
        binary.display()
    );
    let root = repository_root()?;
    let pins = Pins::load(&root)?;
    let repositories = asking(&pins, &args.repositories)?;
    let all = Questions::load(&root.join(QUESTIONS))?;
    let names: Vec<&str> = repositories
        .iter()
        .map(|repository| repository.name.as_str())
        .collect();
    let questions = choose(&all.questions, &names, &args.questions)?;
    let repeats = args.repeats.unwrap_or(pins.agents.repeats);
    ensure!(
        repeats <= pins.agents.repeats,
        "--repeats {repeats} asks for more than the {} repeats bench/bench.toml pins",
        pins.agents.repeats
    );

    let out = args.out.unwrap_or_else(|| root.join(RESULTS));
    // The streams of the runs go in the directory that has the name of the results file without its extension.
    let transcripts = out.with_extension("");
    ensure!(
        transcripts != out,
        "--out needs a file name with an extension, such as agents.json: the streams of the runs go in the directory without it"
    );
    fs::create_dir_all(&transcripts)
        .with_context(|| format!("cannot create {}", transcripts.display()))?;

    let path = std::env::var_os("PATH").unwrap_or_default();
    let claude = which("claude", &path).context("`claude` is not on the PATH")?;
    let node = which("node", &path).context(
        "`node` is not on the PATH, and the language server of the `lsp` setup needs it",
    )?;
    let runs_root = checkout::root(Purpose::Runs);
    if let Some(file) = checkout::claude_md_above(&runs_root) {
        bail!(
            "{} is a CLAUDE.md or an AGENTS.md above the checkouts of the runs, and every run would read it",
            file.display()
        );
    }

    // What the task keeps for itself is beside the checkouts and not in them, where a run could list it from the parent directory of its checkout.
    let task_dir = checkout::task_dir();
    let bin = task_dir.join(".bin");
    let copy = setups::place_binary(&binary, &bin)?;
    let node_dir = checkout::node_dir();
    setups::place_node(&node, &node_dir)?;
    let tmp_root = checkout::tmp_root();
    setups::fresh_dir(&tmp_root)?;
    let programs = Programs::new(&root.join("bench/tools"), &copy);
    setup::node_tools(&programs, &pins)?;
    setup::install(&programs, &pins, &BTreeSet::from([Tool::CodebaseMemory]))?;
    let tools = tool_versions(&programs)?;
    let digest = sha256_hex(all.render()?.as_bytes());
    let mut results = open(&out, &digest, tools)?;

    let files = setups::write_files(&task_dir.join(".setups"), &programs)?;
    let plugin = root.join("plugin");
    let host = |name: &str| std::env::var_os(name);
    let config = init::config_dir(&host).context(
        "neither CLAUDE_CONFIG_DIR nor HOME is set, so the directory of the configuration of Claude Code, which no run may read, is not known",
    )?;
    // What no run may read: this repository and the main worktree it was made from, which hold the sources of the tool under test, and the configuration of the user.
    let private = private_dirs(&root, &config)?;
    let context = Context {
        model: &pins.agents.model,
        turns: pins.agents.turns,
        plugin: &plugin,
        files: &files,
        bin: &bin,
        node: &node_dir,
        task: &task_dir,
        private: &private,
    };
    let invoke = |setup: Setup, arm: Arm, dirs: RunDirs<'_>| {
        setups::invocation(setup, arm, &context, dirs, &host)
    };
    // Every setup in each of its arms is tried once before the first run: a PATH that holds a program no run may find stops the task before it spends anything.
    for (setup, arm) in setups::every_arm() {
        let dirs = RunDirs {
            checkout: &runs_root,
            tmp: &tmp_root,
        };
        invoke(setup, arm, dirs)?;
    }

    // The cache of codebase-memory-mcp holds the index of every checkout, so it is thrown away once, and not with each.
    forget(Tool::CodebaseMemory, &root, &programs)?;
    let aside = files.dir.join("indexes");
    let mut indexes = Indexes::default();
    let mut checkouts = BTreeMap::new();
    for repository in repositories {
        if !questions
            .iter()
            .any(|question| question.repository == repository.name)
        {
            continue;
        }
        let agents = repository
            .agents
            .as_ref()
            .with_context(|| format!("`{}` has no `agents` table", repository.name))?;
        let dir = checkout::prepare(repository, agents, Purpose::Runs)?;
        index(repository, &dir, &programs)?;
        indexes.keep_gsttt(
            &repository.name,
            &dir,
            &aside.join("gsttt").join(&repository.name),
        )?;
        checkouts.insert(
            repository.name.clone(),
            Checkout {
                roots: vec![
                    dir.clone(),
                    checkout::directory(Purpose::Runs, &repository.name),
                ],
                dir,
            },
        );
    }

    // The cache of codebase-memory-mcp holds every checkout, so it is kept once they are all indexed.
    indexes.keep_cbm(&programs.cbm_cache(), &aside.join("cbm"))?;
    let user = init::user_entries(&config);
    let local = local_paths(&root, &checkouts);
    // Ctrl-C is caught from here, where the first run starts: before it, a signal ends the task at once, as it always did.
    let stop = Stop::catch()?;
    let sessions_root = sessions::root();
    let session = Session {
        claude: &claude,
        questions: &questions,
        repeats,
        checkouts: &checkouts,
        invoke: Invoke(&invoke),
        tmp_root: &tmp_root,
        sessions: &sessions_root,
        user: &user,
        indexes: &indexes,
        programs: &programs,
        limit: Duration::from_secs(pins.agents.seconds),
        stop: &stop,
        out: &out,
        transcripts: &transcripts,
        local: &local,
    };
    let made = session.run_all(&mut results)?;
    say(&format!(
        "{made} runs made, {} in all in {}",
        results.runs.len(),
        out.display()
    ))?;
    write!(std::io::stdout().lock(), "{}", table::render(&results))?;
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use serde_json::Value;
    use tempfile::TempDir;

    use super::*;
    use crate::agents::exec::{fake_claude, fake_program};
    use crate::agents::guard::repo;
    use crate::agents::init::{event_of, expected};
    use crate::agents::process::GRACE;
    use crate::agents::questions::Kind;
    use crate::agents::results::{sample, started};
    use crate::agents::stream::events::{assistant, lines, result, tool_call, tool_result};

    const ANSWER: &str = "Found them.\n\n```files\na.ts\n```";

    /// How long the stand-in stays on after its stream when a test marks it `slow-<n>`. The bounds of the tests that wait for it are made from this.
    const SLOW_SECONDS: u64 = 60;

    fn question(id: &str, kind: Kind) -> Question {
        Question {
            id: id.to_owned(),
            repository: "repo".to_owned(),
            kind,
            scope: ".".to_owned(),
            file: "a.ts".to_owned(),
            name: "f".to_owned(),
            line: 1,
            prompt: format!("Which files use f? ({id})"),
            answer: vec!["a.ts".to_owned(), "b.ts".to_owned()],
            ignored: Vec::new(),
        }
    }

    /// The programs under `dir`, with a stand-in for codebase-memory-mcp over a cache that holds a `project.db`. The stand-in adds a line to `daemon` in `dir` for each call: its arguments, its `CBM_CACHE_DIR` and what `project.db` held then.
    fn programs_with_cbm(dir: &Path) -> Programs {
        let programs = Programs::new(&dir.join("tools"), &dir.join("gsttt"));
        fs::create_dir_all(programs.cbm_cache()).unwrap();
        fs::write(programs.cbm_cache().join("project.db"), "as indexed\n").unwrap();
        let body = format!(
            r#"echo "$* $CBM_CACHE_DIR $(cat "$CBM_CACHE_DIR/project.db" 2>/dev/null | tr '\n' ' ')" >> '{}/daemon'"#,
            dir.display()
        );
        fake_program(&programs.cbm, "codebase-memory-mcp", &body);
        programs
    }

    /// A stand-in for `claude` that prints an init event and a stream, in a directory of its own, against a git repository as the checkout.
    struct Bed {
        dir: TempDir,
        _repo: TempDir,
        checkouts: BTreeMap<String, Checkout>,
        /// The directory the temporary directories of the runs are made in.
        tmp_root: PathBuf,
        /// The checkout and the temporary directory each run was given, in order.
        seen: std::cell::RefCell<Vec<(PathBuf, PathBuf)>>,
        questions: Vec<Question>,
        claude: PathBuf,
        local: Vec<String>,
        /// The indexes kept aside: the ones the runs of the gsttt and cbm setups find put back, and the tests of the runs that change one.
        indexes: Indexes,
        /// The programs of the task, whose codebase-memory-mcp is the stand-in of `programs_with_cbm`.
        programs: Programs,
        /// The temporary root of the Claude Code sessions, where the stand-in makes the directory of its session as the real one does.
        sessions: PathBuf,
    }

    impl Bed {
        /// A bed whose runs answer with `stream`. Every setup prints the init event it should, in both its arms unless a test writes `init-<setup>-<arm>.json` to tell another.
        fn new(stream: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let (repo_guard, checkout) = repo();
            let script = format!(
                r#"d='{}'
cat > /dev/null
echo x >> "$d/calls"
n=$(wc -l < "$d/calls" | tr -d ' ')
echo "$TMPDIR" >> "$d/tmpdirs"
touch "$TMPDIR/inside"
mkdir -p "$d/sessions/-project/session-$2" && echo x > "$d/sessions/-project/session-$2/note"
if [ -f "$d/init-$2-$4.json" ]; then cat "$d/init-$2-$4.json"; else cat "$d/init-$2.json"; fi
if [ -f "$d/hold" ]; then sleep 30; fi
if [ -f "$d/dirty" ]; then rm "$d/dirty"; echo notes > notes.md; fi
if [ -f "$d/touch-$2" ]; then echo run >> "$(cat "$d/touch-$2")"; fi
if [ -f "$d/stream-$n.jsonl" ]; then cat "$d/stream-$n.jsonl"; else cat "$d/stream.jsonl"; fi
if [ -f "$d/slow-$n" ]; then sleep {SLOW_SECONDS}; fi
touch "$d/finished"
"#,
                dir.path().display()
            );
            let claude = fake_claude(dir.path(), &script);
            for setup in Setup::ALL {
                fs::write(
                    dir.path().join(format!("init-{setup}.json")),
                    format!("{}\n", event_of(&expected(setup))),
                )
                .unwrap();
            }
            let tmp_root = dir.path().join("tmp");
            fs::create_dir_all(&tmp_root).unwrap();
            let sessions = dir.path().join("sessions");
            let programs = programs_with_cbm(dir.path());
            let gsttt = checkout.join(plan::GSTTT_CACHE);
            fs::create_dir_all(&gsttt).unwrap();
            fs::write(gsttt.join("snapshot.gsttt"), "as indexed\n").unwrap();
            let mut indexes = Indexes::default();
            indexes
                .keep_gsttt("repo", &checkout, &dir.path().join("kept/gsttt"))
                .unwrap();
            indexes
                .keep_cbm(&programs.cbm_cache(), &dir.path().join("kept/cbm"))
                .unwrap();
            let bed = Self {
                dir,
                _repo: repo_guard,
                checkouts: BTreeMap::from([(
                    "repo".to_owned(),
                    Checkout {
                        roots: vec![checkout.clone()],
                        dir: checkout,
                    },
                )]),
                tmp_root,
                seen: std::cell::RefCell::default(),
                questions: vec![question("repo-users-1", Kind::Users)],
                claude,
                local: Vec::new(),
                indexes,
                programs,
                sessions,
            };
            bed.stream(stream);
            bed
        }

        /// A bed whose runs end with `text` as the final message.
        fn answering(text: &str) -> Self {
            Self::new(&lines(&[
                assistant("m1", 5, "looking"),
                tool_call("m2", "call-1", "Read"),
                tool_call("m3", "call-2", "Read"),
                tool_call("m3", "call-3", "ToolSearch"),
                result("success", false, text),
            ]))
        }

        fn stream(&self, text: &str) {
            fs::write(self.dir.path().join("stream.jsonl"), text).unwrap();
        }

        /// What the stand-in does in its `number`th call instead of the usual.
        fn stream_of_call(&self, number: usize, text: &str) {
            fs::write(self.dir.path().join(format!("stream-{number}.jsonl")), text).unwrap();
        }

        /// Makes the stand-in do something extra: `hold` and `dirty` in its first call, or `slow-<n>` to stay on after its stream, for `SLOW_SECONDS`, in its `n`th.
        fn mark(&self, what: &str) {
            fs::write(self.dir.path().join(what), "").unwrap();
        }

        /// Makes the run of `setup` add a line to `file`.
        fn touch(&self, setup: &str, file: &Path) {
            fs::write(
                self.dir.path().join(format!("touch-{setup}")),
                file.to_string_lossy().as_bytes(),
            )
            .unwrap();
        }

        fn out(&self) -> PathBuf {
            self.dir.path().join("results/agents.json")
        }

        fn calls(&self) -> usize {
            fs::read_to_string(self.dir.path().join("calls"))
                .map_or(0, |calls| calls.lines().count())
        }

        /// What the stand-in for codebase-memory-mcp logged, a line for each call.
        fn daemon_calls(&self) -> Vec<String> {
            fs::read_to_string(self.dir.path().join("daemon"))
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect()
        }

        fn finished(&self) -> bool {
            self.dir.path().join("finished").exists()
        }

        fn checkout(&self) -> &Path {
            &self.checkouts["repo"].dir
        }

        fn go(&self, results: &mut Results, limit: Duration) -> anyhow::Result<usize> {
            self.go_until(results, limit, &Stop::default())
        }

        /// Like `go`, with a `stop` that the test may raise.
        fn go_until(
            &self,
            results: &mut Results,
            limit: Duration,
            stop: &Stop,
        ) -> anyhow::Result<usize> {
            let questions: Vec<&Question> = self.questions.iter().collect();
            let out = self.out();
            let transcripts = self.dir.path().join("results/agents");
            fs::create_dir_all(&transcripts).unwrap();
            let user = UserEntries::default();
            // Every setup is given the directories of its run, which the stand-in finds in `TMPDIR`.
            let invoke =
                |setup: Setup, arm: Arm, dirs: RunDirs<'_>| -> anyhow::Result<Invocation> {
                    self.seen
                        .borrow_mut()
                        .push((dirs.checkout.to_path_buf(), dirs.tmp.to_path_buf()));
                    Ok(Invocation {
                        args: vec![
                            "--setup".into(),
                            setup.name().into(),
                            "--arm".into(),
                            arm.name().into(),
                        ],
                        env: vec![
                            ("PATH".to_owned(), "/usr/bin:/bin".into()),
                            ("TMPDIR".to_owned(), dirs.tmp.as_os_str().to_owned()),
                        ],
                    })
                };
            Session {
                claude: &self.claude,
                questions: &questions,
                repeats: 1,
                checkouts: &self.checkouts,
                invoke: Invoke(&invoke),
                tmp_root: &self.tmp_root,
                sessions: &self.sessions,
                user: &user,
                indexes: &self.indexes,
                programs: &self.programs,
                limit,
                stop,
                out: &out,
                transcripts: &transcripts,
                local: &self.local,
            }
            .run_all(results)
        }

        fn run(&self, results: &mut Results) -> anyhow::Result<usize> {
            self.go(results, Duration::from_secs(60))
        }
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A run that changes an index is marked and the index is put back"
    )]
    #[test]
    fn a_run_that_changes_the_index_of_its_setup_is_marked_and_the_index_is_put_back() {
        let mut bed = Bed::answering(ANSWER);
        let gsttt = bed.checkout().join(plan::GSTTT_CACHE);
        let cache = bed.dir.path().join("cbm-cache");
        for (dir, file) in [(&gsttt, "snapshot.gsttt"), (&cache, "project.db")] {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join(file), "as indexed\n").unwrap();
        }
        let checkout = bed.checkout().to_path_buf();
        let aside = bed.dir.path().join("aside");
        bed.indexes
            .keep_gsttt("repo", &checkout, &aside.join("gsttt"))
            .unwrap();
        bed.indexes.keep_cbm(&cache, &aside.join("cbm")).unwrap();
        // The run of the gsttt setup writes into the index of gsttt, and the run of the cbm setup into the cache.
        bed.touch("gsttt", &gsttt.join("snapshot.gsttt"));
        bed.touch("cbm", &cache.join("project.db"));

        let mut results = started();
        bed.run(&mut results).unwrap();

        let marked: Vec<(Setup, Arm, bool)> = results
            .runs
            .iter()
            .map(|run| (run.setup, run.arm, run.changed_checkout))
            .collect();
        assert_eq!(
            marked,
            [
                (Setup::Plain, Arm::Installed, false),
                (Setup::Lsp, Arm::Installed, false),
                (Setup::Lsp, Arm::Told, false),
                (Setup::Cbm, Arm::Installed, true),
                (Setup::Cbm, Arm::Told, true),
                (Setup::Gsttt, Arm::Installed, true),
                (Setup::Gsttt, Arm::Told, true)
            ]
        );
        for (dir, file) in [(&gsttt, "snapshot.gsttt"), (&cache, "project.db")] {
            assert_eq!(fs::read_to_string(dir.join(file)).unwrap(), "as indexed\n");
        }
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "The daemon is stopped before its cache is thrown away"
    )]
    #[test]
    fn the_daemon_of_codebase_memory_is_stopped_before_its_cache_is_thrown_away() {
        let bed = Bed::answering(ANSWER);
        let cache = bed.programs.cbm_cache();
        forget(Tool::CodebaseMemory, bed.checkout(), &bed.programs).unwrap();

        let stops = bed.daemon_calls();
        assert!(!stops.is_empty(), "the daemon was not stopped");
        for stop in &stops {
            // `project.db` was still there when the daemon was asked to stop.
            assert_eq!(
                stop,
                &format!("daemon stop {} as indexed ", cache.display())
            );
        }
        assert!(
            fs::read_dir(&cache).unwrap().next().is_none(),
            "the cache is thrown away"
        );
    }

    #[test]
    fn a_cache_is_not_thrown_away_under_a_daemon_that_cannot_be_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let programs = programs_with_cbm(dir.path());
        fake_program(
            &programs.cbm,
            "codebase-memory-mcp",
            "echo it stays >&2; exit 1",
        );
        let error = format!(
            "{:#}",
            forget(Tool::CodebaseMemory, dir.path(), &programs).unwrap_err()
        );
        assert!(
            error.contains("daemon") && error.contains("it stays"),
            "{error}"
        );
        assert!(programs.cbm_cache().join("project.db").exists());
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "The daemon is stopped before its cache is put back"
    )]
    #[test]
    fn the_daemon_of_codebase_memory_is_stopped_before_its_cache_is_put_back() {
        let bed = Bed::answering(ANSWER);
        let cache = bed.programs.cbm_cache();
        // The run of the cbm setup writes into the cache.
        bed.touch("cbm", &cache.join("project.db"));
        let mut results = started();
        bed.run(&mut results).unwrap();

        // One stop for each run of the cbm setup, with the cache as the run left it.
        let stops = bed.daemon_calls();
        assert_eq!(stops.len(), 2, "{stops:?}");
        for stop in &stops {
            assert_eq!(
                stop,
                &format!("daemon stop {} as indexed run ", cache.display())
            );
        }
        assert_eq!(
            fs::read_to_string(cache.join("project.db")).unwrap(),
            "as indexed\n"
        );
    }

    #[test]
    fn a_daemon_that_cannot_be_stopped_stops_the_task_before_the_cache_is_put_back() {
        let bed = Bed::answering(ANSWER);
        fake_program(&bed.programs.cbm, "codebase-memory-mcp", "exit 1");
        let mut results = started();
        let error = format!("{:#}", bed.run(&mut results).unwrap_err());
        assert!(error.contains("cannot stop the daemon"), "{error}");
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A run whose results cannot be written is not kept"
    )]
    #[test]
    fn a_run_whose_results_cannot_be_written_is_not_kept() {
        let mut bed = Bed::answering(ANSWER);
        // The results name the question, so a local path that looks like it keeps them from being written.
        bed.local = vec!["repo-users-1".to_owned()];
        let mut results = started();

        let error = bed.run(&mut results).unwrap_err();

        assert!(
            format!("{error:#}").contains("must not be published"),
            "{error:#}"
        );
        assert!(results.runs.is_empty(), "the run is not kept in memory");
        assert!(!bed.out().exists(), "and not on disk");
        assert_eq!(bed.calls(), 1, "the task stopped at that run");
    }

    /// The versions `started()` records, with the version of gsttt as given.
    fn tools(version: &str) -> Vec<ToolVersion> {
        vec![ToolVersion {
            tool: "gsttt".to_owned(),
            version: version.to_owned(),
        }]
    }

    /// A results file written for the questions and the tools of `started()`.
    fn written() -> (TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("agents.json");
        // The model and the version of Claude Code are the ones of the first init event.
        let mut results = started();
        results.accept(&expected(Setup::Plain)).unwrap();
        results.write(&out, &[]).unwrap();
        (dir, out)
    }

    #[test]
    fn results_of_the_same_questions_and_tools_are_continued() {
        let (_dir, out) = written();
        let results = open(&out, &"a".repeat(64), tools("1.0.0")).unwrap();
        assert_eq!(results.questions_sha256, "a".repeat(64));
        assert_eq!(results.tools, tools("1.0.0"));
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "Results of other questions are refused")]
    #[test]
    fn results_of_other_questions_are_not_continued() {
        let (_dir, out) = written();
        let error = open(&out, &"b".repeat(64), tools("1.0.0")).unwrap_err();
        assert!(error.to_string().contains("other questions"), "{error}");
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "Results of other versions of the tools are refused"
    )]
    #[test]
    fn results_of_another_version_of_a_tool_are_not_continued() {
        let (_dir, out) = written();
        let error = open(&out, &"a".repeat(64), tools("2.0.0")).unwrap_err();
        assert!(
            error.to_string().contains("other versions of the tools"),
            "{error}"
        );
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "Results of another build of the binary are refused"
    )]
    #[test]
    fn results_of_another_build_of_the_binary_are_not_continued() {
        let dir = tempfile::tempdir().unwrap();
        let (first, second) = (dir.path().join("first"), dir.path().join("second"));
        fs::write(&first, "one build\n").unwrap();
        fs::write(&second, "another build of the same version\n").unwrap();
        let out = dir.path().join("agents.json");
        let mut results = Results::start(
            "2026-10-29T10:00:00Z".to_owned(),
            "Test CPU, 8 cores, 16 GiB, TestOS 1".to_owned(),
            "a".repeat(64),
            vec![binary_sum(&first).unwrap()],
        );
        results.accept(&expected(Setup::Plain)).unwrap();
        results.write(&out, &[]).unwrap();

        let same = open(&out, &"a".repeat(64), vec![binary_sum(&first).unwrap()]);
        assert!(same.is_ok(), "the same build continues the results");
        let error = open(&out, &"a".repeat(64), vec![binary_sum(&second).unwrap()]).unwrap_err();
        assert!(
            error.to_string().contains("other versions of the tools"),
            "{error}"
        );
    }

    #[test]
    fn the_tools_hold_the_sum_of_the_binary_beside_its_version() {
        let dir = tempfile::tempdir().unwrap();
        let tools = dir.path().join("tools");
        let cbm = tools.join("cbm");
        fs::create_dir_all(&cbm).unwrap();
        fake_program(
            &cbm,
            "codebase-memory-mcp",
            "echo codebase-memory-mcp 0.6.0",
        );
        for package in NODE_PACKAGES {
            let installed = tools.join("node/node_modules").join(package);
            fs::create_dir_all(&installed).unwrap();
            fs::write(installed.join("package.json"), r#"{"version":"5.0.0"}"#).unwrap();
        }
        // Two builds of one version: the comment makes the bytes differ and the answer to `--version` the same.
        let record = |build: &str| {
            let home = dir.path().join(build);
            fs::create_dir(&home).unwrap();
            let gsttt = fake_program(&home, "gsttt", &format!("echo gsttt 1.2.3 # {build}"));
            tool_versions(&Programs::new(&tools, &gsttt)).unwrap()
        };
        let (first, second) = (record("one"), record("another"));
        let find = |tools: &[ToolVersion], name: &str| {
            tools
                .iter()
                .find(|tool| tool.tool == name)
                .map(|tool| tool.version.clone())
        };

        assert_eq!(find(&first, "gsttt").as_deref(), Some("1.2.3"));
        assert_eq!(find(&first, "gsttt"), find(&second, "gsttt"));
        assert_ne!(find(&first, "gsttt-sha256"), find(&second, "gsttt-sha256"));
        let built = fs::read(dir.path().join("one/gsttt")).unwrap();
        assert_eq!(find(&first, "gsttt-sha256"), Some(sha256_hex(&built)));
        let names: Vec<&str> = first.iter().map(|tool| tool.tool.as_str()).collect();
        assert!(names.is_sorted(), "{names:?}");
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "The session directory of a run is removed")]
    #[test]
    fn the_session_directory_of_every_run_is_removed_and_no_other_in_the_root() {
        let bed = Bed::answering(ANSWER);
        // The session of other work of the user, and a file in the root itself.
        let other = bed.sessions.join("-project/another-session");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("note"), "").unwrap();
        fs::write(bed.sessions.join("stray"), "").unwrap();

        let mut results = started();
        bed.run(&mut results).unwrap();

        // The stand-in made `session-<setup>` in each call, as Claude Code does for the session of a run, and named it in its init event.
        assert_eq!(bed.calls(), setups::every_arm().count());
        for setup in Setup::ALL {
            let session = bed.sessions.join(format!("-project/session-{setup}"));
            assert!(!session.exists(), "{}", session.display());
        }
        assert!(other.join("note").exists());
        assert!(bed.sessions.join("stray").exists());
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "The session directory of a run the task stops on is removed"
    )]
    #[test]
    fn the_session_directory_of_a_run_the_task_stops_on_is_removed_too() {
        let bed = Bed::answering(ANSWER);
        bed.mark("hold");
        let mut leaked = expected(Setup::Plain);
        leaked.mcp_servers.push(init::McpServer {
            name: "telegram".to_owned(),
            status: "connected".to_owned(),
        });
        fs::write(
            bed.dir.path().join("init-plain.json"),
            format!("{}\n", event_of(&leaked)),
        )
        .unwrap();

        let mut results = started();
        bed.run(&mut results).unwrap_err();

        assert_eq!(bed.calls(), 1);
        assert!(bed.sessions.join("-project").is_dir());
        assert!(!bed.sessions.join("-project/session-plain").exists());
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "The main worktree of the repository is out of reach of the runs"
    )]
    #[test]
    fn the_main_worktree_of_a_linked_worktree_is_private_with_the_repository() {
        let (_guard, main) = repo();
        let dir = tempfile::tempdir().unwrap();
        let linked = dir.path().join("linked");
        git(
            &main,
            &[
                "worktree",
                "add",
                "--quiet",
                "--detach",
                linked.to_str().unwrap(),
            ],
        )
        .unwrap();
        let linked = fs::canonicalize(linked).unwrap();
        let config = dir.path().join("config");

        // A linked worktree comes with the worktree it was made from.
        assert_eq!(
            private_dirs(&linked, &config).unwrap(),
            [linked.clone(), config.clone(), main.clone()]
        );
        // The main worktree is the repository itself, which is listed once.
        assert_eq!(private_dirs(&main, &config).unwrap(), [main, config]);
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "The setups take turns")]
    #[test]
    fn the_setups_take_turns() {
        let first = question("repo-users-1", Kind::Users);
        let second = question("repo-users-2", Kind::Users);
        let planned = order(&[&first, &second], 2);
        let names: Vec<String> = planned
            .iter()
            .map(|run| {
                format!(
                    "{} {} {} {}",
                    run.question.id, run.setup, run.arm, run.repeat
                )
            })
            .collect();

        // Seven runs of a question in a repeat: plain, then each other setup as installed and as told.
        assert_eq!(names.len(), 28);
        assert_eq!(
            names[..7],
            [
                "repo-users-1 plain installed 1",
                "repo-users-1 lsp installed 1",
                "repo-users-1 lsp told 1",
                "repo-users-1 cbm installed 1",
                "repo-users-1 cbm told 1",
                "repo-users-1 gsttt installed 1",
                "repo-users-1 gsttt told 1"
            ]
        );
        assert_eq!(
            names[21..],
            [
                "repo-users-2 plain installed 2",
                "repo-users-2 lsp installed 2",
                "repo-users-2 lsp told 2",
                "repo-users-2 cbm installed 2",
                "repo-users-2 cbm told 2",
                "repo-users-2 gsttt installed 2",
                "repo-users-2 gsttt told 2"
            ]
        );
        // The first question is over before the second begins, and the first repeat before the second.
        assert!(
            planned[7..14]
                .iter()
                .all(|run| run.question.id == second.id)
        );
        assert!(planned[..14].iter().all(|run| run.repeat == 1));
        assert!(planned[14..].iter().all(|run| run.repeat == 2));
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "A run without an answer scores zero")]
    #[test]
    fn a_run_without_an_answer_scores_zero() {
        let bed = Bed::answering("Done, but I have no list for you.");
        let mut results = started();
        assert_eq!(bed.run(&mut results).unwrap(), 7);

        assert_eq!(results.runs.len(), 7);
        for run in &results.runs {
            assert_eq!(run.outcome, Outcome::NoAnswer);
            assert_eq!(run.answer, Vec::<String>::new());
            assert_eq!((run.precision, run.recall, run.f1), (0.0, 0.0, 0.0));
            // The run still cost what it cost.
            assert_eq!(run.tokens(), 6 + 7 + 8 + 9);
            assert_eq!((run.cost_usd, run.turns), (Some(0.25), 4));
        }
    }

    #[test]
    fn an_answer_is_scored_and_the_results_are_written_after_every_run() {
        let bed = Bed::answering(ANSWER);
        let mut results = started();
        bed.run(&mut results).unwrap();

        let written = Results::load(&bed.out()).unwrap();
        assert_eq!(written, results);
        assert_eq!(
            (written.model.as_str(), written.claude_version.as_str()),
            ("claude-test-1", "9.9.9")
        );
        let ways: Vec<(Setup, Arm)> = written
            .runs
            .iter()
            .map(|run| (run.setup, run.arm))
            .collect();
        assert_eq!(ways, setups::every_arm().collect::<Vec<_>>());
        for run in &written.runs {
            assert_eq!(
                (run.question.as_str(), run.kind, run.repeat),
                ("repo-users-1", Kind::Users, 1)
            );
            assert_eq!(run.outcome, Outcome::Answered);
            assert_eq!(run.answer, ["a.ts"]);
            assert_eq!((run.precision, run.recall), (1.0, 0.5));
            assert!(!run.changed_checkout);
            // The calls of the stand-in: two of `Read` and one of `ToolSearch`, the last in a message that came twice.
            assert_eq!(
                run.tool_calls,
                BTreeMap::from([("Read".to_owned(), 2), ("ToolSearch".to_owned(), 1)])
            );
        }
        // The stream of every run is kept, a told run beside its installed run.
        for (setup, arm) in setups::every_arm() {
            let kept = bed
                .dir
                .path()
                .join(format!("results/agents/repo-users-1-{setup}-{arm}-1.jsonl"));
            assert!(
                fs::read_to_string(&kept).unwrap().contains("\"init\""),
                "{}",
                kept.display()
            );
        }
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "No local path reaches the agent results")]
    #[test]
    fn no_local_path_reaches_the_agent_results() {
        let mut bed = Bed::answering("```files\na.ts\n/Users/quincy-bench/secrets/notes.ts\n```");
        bed.local = vec!["/Users/quincy-bench".to_owned()];
        let mut results = started();
        bed.run(&mut results).unwrap();

        for run in &results.runs {
            // The path outside the checkout is a wrong answer, and a stand-in is all that is kept of it.
            assert_eq!(run.answer, ["<outside the checkout #1>", "a.ts"]);
            assert_eq!((run.precision, run.recall), (0.5, 0.5));
        }
        let text = fs::read_to_string(bed.out()).unwrap();
        assert!(
            !text.contains("quincy-bench") && !text.contains("/Users/"),
            "{text}"
        );
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "A usage limit stops the task")]
    #[test]
    fn a_usage_limit_stops_the_task() {
        let bed = Bed::answering(ANSWER);
        let limit = "You've hit your limit \u{b7} resets 5pm";
        bed.stream_of_call(
            3,
            &lines(&[assistant("m1", 1, limit), result("success", true, limit)]),
        );
        let mut results = started();
        let error = format!("{:#}", bed.run(&mut results).unwrap_err());

        assert!(
            error.contains("usage limit") && error.contains("repo-users-1"),
            "{error}"
        );
        assert_eq!(bed.calls(), 3, "nothing is asked after the limit");
        let written = Results::load(&bed.out()).unwrap();
        let ways: Vec<(Setup, Arm)> = written
            .runs
            .iter()
            .map(|run| (run.setup, run.arm))
            .collect();
        // The third run is the told run of the lsp setup.
        assert_eq!(
            ways,
            [(Setup::Plain, Arm::Installed), (Setup::Lsp, Arm::Installed)],
            "the runs before it, and not that one"
        );
        assert_eq!(results.runs.len(), 2);
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "Finished runs are skipped")]
    #[test]
    fn finished_runs_are_skipped() {
        let bed = Bed::answering(ANSWER);
        let mut results = started();
        results.accept(&expected(Setup::Plain)).unwrap();
        // The installed run of the lsp setup is finished, and its told run is not.
        results
            .runs
            .push(sample("repo-users-1", Kind::Users, Setup::Plain, 1));
        results
            .runs
            .push(sample("repo-users-1", Kind::Users, Setup::Lsp, 1));
        let finished = results.runs.clone();

        assert_eq!(bed.run(&mut results).unwrap(), 5);
        assert_eq!(bed.calls(), 5, "every planned run but those two");
        assert_eq!(results.runs.len(), 7);
        assert_eq!(
            results.runs[..2],
            finished,
            "the finished runs are as they were"
        );
        let ways: Vec<(Setup, Arm)> = results
            .runs
            .iter()
            .map(|run| (run.setup, run.arm))
            .collect();
        assert_eq!(ways, setups::every_arm().collect::<Vec<_>>());

        // Started over once more, nothing is left to run.
        assert_eq!(bed.run(&mut results).unwrap(), 0);
        assert_eq!(bed.calls(), 5);
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "Results of another model are refused")]
    #[test]
    fn results_of_another_model_are_refused() {
        let bed = Bed::answering(ANSWER);
        // The process stays on after its init event: only the check of the task can end it.
        bed.mark("hold");
        let mut results = started();
        results.accept(&expected(Setup::Plain)).unwrap();
        results.model = "claude-old-0".to_owned();
        results
            .runs
            .push(sample("repo-users-0", Kind::Users, Setup::Plain, 1));
        results.write(&bed.out(), &[]).unwrap();
        let before = fs::read(bed.out()).unwrap();

        let started_at = Instant::now();
        let error = format!("{:#}", bed.run(&mut results).unwrap_err());
        assert!(
            error.contains("claude-old-0") && error.contains("claude-test-1"),
            "{error}"
        );
        assert!(
            started_at.elapsed() < Duration::from_secs(20),
            "the run was stopped at its init event"
        );
        assert_eq!(bed.calls(), 1);
        assert!(
            !bed.finished(),
            "the run was ended as soon as its init event was written"
        );
        assert_eq!(results.runs.len(), 1, "nothing was added");
        assert_eq!(
            fs::read(bed.out()).unwrap(),
            before,
            "the file is as it was"
        );
    }

    #[test]
    fn a_run_that_loaded_something_else_is_ended_as_soon_as_its_init_event_is_written() {
        let bed = Bed::answering(ANSWER);
        bed.mark("hold");
        let mut leaked = expected(Setup::Plain);
        leaked.mcp_servers.push(init::McpServer {
            name: "telegram".to_owned(),
            status: "connected".to_owned(),
        });
        fs::write(
            bed.dir.path().join("init-plain.json"),
            format!("{}\n", event_of(&leaked)),
        )
        .unwrap();
        let mut results = started();
        let error = format!("{:#}", bed.run(&mut results).unwrap_err());

        assert!(
            error.contains("`plain`") && error.contains("`telegram`"),
            "{error}"
        );
        assert_eq!(bed.calls(), 1);
        assert!(!bed.finished());
        assert!(results.runs.is_empty() && !bed.out().exists());
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "The init event of a told run is checked like any other"
    )]
    #[test]
    fn the_init_event_of_a_told_run_is_checked_like_any_other() {
        let bed = Bed::answering(ANSWER);
        // The told run of the lsp setup loads a server that no run may have; its installed run, which comes before it, loads what it should.
        let mut leaked = expected(Setup::Lsp);
        leaked.mcp_servers.push(init::McpServer {
            name: "telegram".to_owned(),
            status: "connected".to_owned(),
        });
        fs::write(
            bed.dir.path().join("init-lsp-told.json"),
            format!("{}\n", event_of(&leaked)),
        )
        .unwrap();
        let mut results = started();
        let error = format!("{:#}", bed.run(&mut results).unwrap_err());

        assert!(
            error.contains("`lsp`") && error.contains("told") && error.contains("`telegram`"),
            "{error}"
        );
        assert_eq!(
            bed.calls(),
            3,
            "the plain run, the installed run, then that one"
        );
        let ways: Vec<(Setup, Arm)> = results
            .runs
            .iter()
            .map(|run| (run.setup, run.arm))
            .collect();
        assert_eq!(
            ways,
            [(Setup::Plain, Arm::Installed), (Setup::Lsp, Arm::Installed)]
        );
    }

    #[test]
    fn a_run_that_changed_the_checkout_is_marked_and_the_checkout_is_restored() {
        let bed = Bed::answering(ANSWER);
        bed.mark("dirty");
        let mut results = started();
        bed.run(&mut results).unwrap();

        let marks: Vec<bool> = results
            .runs
            .iter()
            .map(|run| run.changed_checkout)
            .collect();
        assert_eq!(marks, [true, false, false, false, false, false, false]);
        assert!(!bed.checkout().join("notes.md").exists());
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "A signal stops the task")]
    #[test]
    fn a_signal_ends_the_run_and_stops_the_task_with_nothing_recorded_for_it() {
        let bed = Bed::answering(ANSWER);
        // The stand-in has written its init event and holds on, as a run does while the model works.
        bed.mark("hold");
        let stop = Stop::default();
        // The signal comes once the stand-in has started, however long that takes on a busy machine.
        let calls = bed.dir.path().join("calls");
        let raiser = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let waited = Instant::now();
                while !calls.exists() && waited.elapsed() < Duration::from_secs(30) {
                    std::thread::sleep(Duration::from_millis(20));
                }
                stop.raise(signal_hook::consts::SIGINT);
            })
        };
        let mut results = started();
        let error = format!(
            "{:#}",
            bed.go_until(&mut results, Duration::from_secs(60), &stop)
                .unwrap_err()
        );
        raiser.join().unwrap();
        assert!(
            error.contains("SIGINT") && error.contains("repo-users-1"),
            "{error}"
        );
        assert!(
            results.runs.is_empty(),
            "the run it stopped is not recorded"
        );
        assert!(!bed.finished(), "the process was ended");
        assert_eq!(bed.calls(), 1, "no run is started after the signal");
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "A run ends with its result")]
    #[test]
    fn a_run_that_stays_alive_after_its_result_is_recorded_from_it_and_not_as_a_timeout() {
        let bed = Bed::answering(ANSWER);
        // The stand-in writes its stream, result included, and stays on, as `claude -p` does for a task of the model left in the background.
        bed.mark("slow-1");
        let mut results = started();
        let started_at = Instant::now();
        bed.go(&mut results, Duration::from_secs(40)).unwrap();
        let took = started_at.elapsed();

        let first = &results.runs[0];
        assert_eq!(first.outcome, Outcome::Answered);
        assert_eq!(first.answer, ["a.ts"]);
        // The time of the run ends at the result, which was written at once; the wait for the exit is no part of it. The task waited the grace for a process that stayed on after the result, so it took the time of the run and the grace, and more: a time that held the wait would leave nothing of the grace over. That holds on a machine of any speed, where a bound in seconds would not.
        assert!(
            took >= Duration::from_millis(first.duration_ms) + GRACE,
            "{took:?} against {} ms",
            first.duration_ms
        );
        // The task waits the grace for the stand-in, which stays on for `SLOW_SECONDS`: half of that is more than three times the whole task takes, and leaves the stand-in as long again to end by itself.
        assert!(took < Duration::from_secs(SLOW_SECONDS / 2));
    }

    #[test]
    fn a_run_stopped_at_the_limit_keeps_the_tokens_it_streamed() {
        let bed = Bed::answering(ANSWER);
        // Only the first run is slow, and has streamed one message when it is stopped. The limit leaves the stand-in room to start on a busy machine.
        bed.mark("slow-1");
        bed.stream_of_call(
            1,
            &lines(&[
                assistant("m1", 5, "still looking"),
                // The same message again, grown by a call, which failed.
                tool_call("m1", "call-1", "LSP"),
                tool_result("call-1", true),
            ]),
        );
        let mut results = started();
        bed.go(&mut results, Duration::from_secs(4)).unwrap();

        let first = &results.runs[0];
        assert_eq!(first.outcome, Outcome::Timeout);
        assert_eq!(first.answer, Vec::<String>::new());
        assert_eq!((first.precision, first.recall, first.f1), (0.0, 0.0, 0.0));
        assert_eq!(
            (
                first.input_tokens,
                first.output_tokens,
                first.cache_read_tokens,
                first.cache_write_tokens
            ),
            (3, 5, 100, 10)
        );
        assert_eq!((first.cost_usd, first.turns), (None, 1));
        // A run that never reached its result keeps the calls it made.
        assert_eq!(first.tool_calls, BTreeMap::from([("LSP".to_owned(), 1)]));
        assert_eq!(first.tool_errors, BTreeMap::from([("LSP".to_owned(), 1)]));
        assert!(first.duration_ms >= 4000);
        // The runs after it are made and judged as usual.
        assert_eq!(results.runs.len(), 7);
        assert!(
            results.runs[1..]
                .iter()
                .all(|run| run.outcome == Outcome::Answered)
        );
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "A run has a temporary directory of its own")]
    #[test]
    fn a_run_has_a_temporary_directory_of_its_own() {
        let bed = Bed::answering(ANSWER);
        let mut results = started();
        assert_eq!(bed.run(&mut results).unwrap(), 7);

        // Each of the seven runs was given a directory of its own, made in the root of the temporary directories and outside the checkout, and the stand-in found it in `TMPDIR` and made a file in it.
        let seen = bed.seen.borrow().clone();
        assert_eq!(seen.len(), 7);
        let given: BTreeSet<&PathBuf> = seen.iter().map(|(_, tmp)| tmp).collect();
        assert_eq!(given.len(), 7, "a directory was shared by two runs");
        for (checkout, tmp) in &seen {
            assert_eq!(checkout, bed.checkout());
            assert_eq!(tmp.parent(), Some(bed.tmp_root.as_path()));
            assert!(!tmp.starts_with(checkout) && !checkout.starts_with(tmp));
        }
        let recorded = fs::read_to_string(bed.dir.path().join("tmpdirs")).unwrap();
        let recorded: BTreeSet<String> = recorded.lines().map(str::to_owned).collect();
        let given: BTreeSet<String> = given.iter().map(|tmp| tmp.display().to_string()).collect();
        assert_eq!(recorded, given);

        // Every one of them is gone once its run is over, and nothing is left in the root.
        assert_eq!(fs::read_dir(&bed.tmp_root).unwrap().count(), 0);

        // Also after a run that failed.
        let bed = Bed::new(&lines(&[result("error_during_execution", true, "boom")]));
        let mut results = started();
        bed.run(&mut results).unwrap_err();
        assert_eq!(bed.seen.borrow().len(), 1);
        assert!(bed.dir.path().join("tmpdirs").exists());
        assert_eq!(fs::read_dir(&bed.tmp_root).unwrap().count(), 0);

        // And after a run that was stopped while it worked.
        let bed = Bed::answering(ANSWER);
        bed.mark("hold");
        let stop = Stop::default();
        let raiser = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(2));
                stop.raise(signal_hook::consts::SIGINT);
            })
        };
        let mut results = started();
        bed.go_until(&mut results, Duration::from_secs(60), &stop)
            .unwrap_err();
        raiser.join().unwrap();
        assert_eq!(bed.seen.borrow().len(), 1);
        assert!(bed.dir.path().join("tmpdirs").exists());
        assert_eq!(fs::read_dir(&bed.tmp_root).unwrap().count(), 0);
    }

    #[test]
    fn a_run_that_used_its_turns_is_recorded_and_one_that_failed_stops_the_task() {
        let bed = Bed::new(&lines(&[result("error_max_turns", true, "")]));
        let mut results = started();
        bed.run(&mut results).unwrap();
        assert!(
            results
                .runs
                .iter()
                .all(|run| run.outcome == Outcome::TurnLimit && run.f1 == 0.0)
        );

        let bed = Bed::new(&lines(&[result("error_during_execution", true, "boom")]));
        let mut results = started();
        let error = format!("{:#}", bed.run(&mut results).unwrap_err());
        assert!(
            error.contains("repo-users-1") && error.contains("without a result"),
            "{error}"
        );
        assert!(
            error.contains("repo-users-1-plain-installed-1.jsonl"),
            "{error}"
        );
        assert!(results.runs.is_empty(), "a fault is not a result");
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "A stream with two results stops the task")]
    #[test]
    fn a_stream_with_two_results_stops_the_task() {
        let bed = Bed::new(&lines(&[
            result("success", false, ANSWER),
            result("success", false, ANSWER),
        ]));
        let mut results = started();
        let error = format!("{:#}", bed.run(&mut results).unwrap_err());
        assert!(
            error.contains("repo-users-1") && error.contains("second result event"),
            "{error}"
        );
        assert!(
            error.contains("repo-users-1-plain-installed-1.jsonl"),
            "{error}"
        );
        assert!(results.runs.is_empty(), "a fault is not a result");
    }

    #[test]
    fn the_questions_are_chosen_by_repository_and_by_id() {
        let all = vec![
            question("repo-users-1", Kind::Users),
            question("repo-users-2", Kind::Users),
            Question {
                repository: "other".to_owned(),
                ..question("other-users-1", Kind::Users)
            },
        ];
        let ids = |chosen: &[&Question]| -> Vec<String> {
            chosen.iter().map(|question| question.id.clone()).collect()
        };
        assert_eq!(
            ids(&choose(&all, &["repo", "other"], &[]).unwrap()),
            ["repo-users-1", "repo-users-2", "other-users-1"]
        );
        assert_eq!(
            ids(&choose(&all, &["repo"], &[]).unwrap()),
            ["repo-users-1", "repo-users-2"]
        );
        assert_eq!(
            ids(&choose(&all, &["repo"], &["repo-users-2".to_owned()]).unwrap()),
            ["repo-users-2"]
        );
        let missing = choose(&all, &["repo"], &["repo-users-9".to_owned()]).unwrap_err();
        assert!(missing.to_string().contains("repo-users-9"), "{missing}");
        let elsewhere = choose(&all, &["repo"], &["other-users-1".to_owned()]).unwrap_err();
        assert!(
            elsewhere.to_string().contains("other-users-1"),
            "{elsewhere}"
        );
        assert!(choose(&all, &["none"], &[]).is_err());
    }

    #[test]
    fn the_paths_of_this_machine_are_found_for_the_check_of_the_results() {
        let checkouts = BTreeMap::from([(
            "repo".to_owned(),
            Checkout {
                dir: PathBuf::from("/private/var/folders/xx/yy/T/gsttt-agents-runs/repo"),
                roots: vec![
                    PathBuf::from("/private/var/folders/xx/yy/T/gsttt-agents-runs/repo"),
                    PathBuf::from("/var/folders/xx/yy/T/gsttt-agents-runs/repo"),
                ],
            },
        )]);
        let local = local_paths(Path::new("/work/repo"), &checkouts);
        assert!(local.contains(&"/work/repo".to_owned()));
        assert!(local.contains(&"/private/var/folders/xx/yy/T/gsttt-agents-runs/repo".to_owned()));
        assert!(local.contains(&"/var/folders/xx/yy/T/gsttt-agents-runs/repo".to_owned()));
        assert!(
            local
                .iter()
                .all(|path| path.matches('/').count() >= 2 && !path.ends_with('/'))
        );
        let mut sorted = local.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(local, sorted);
    }

    #[test]
    fn the_stand_in_stream_has_the_shape_of_a_real_one() {
        // The events the tests above feed the task are the ones the stream module reads.
        let stream = lines(&[
            assistant("m1", 5, "looking"),
            result("success", false, ANSWER),
        ]);
        let first: Value = serde_json::from_str(stream.lines().next().unwrap()).unwrap();
        assert_eq!(first["type"], "assistant");
        assert_eq!(stream::observe(&stream).unwrap().end, End::Success);
    }
}
