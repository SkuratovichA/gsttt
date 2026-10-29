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
use super::setups::{self, Context, Invocation, RunDirs, Setup};
use super::stream::{self, End, Observed};
use super::table;
use super::{QUESTIONS, RESULTS, asking, say};
use crate::bench::pins::{Pins, Repository, Tool};
use crate::bench::plan::{self, Programs};
use crate::bench::{hyperfine, machine, setup};
use crate::digest::sha256_hex;
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

/// One run of the task: a question in a setup at a repeat.
#[derive(Clone, Copy, Debug)]
struct Planned<'a> {
    question: &'a Question,
    setup: Setup,
    repeat: u32,
}

/// The runs in the order they are made: by repeat, then by question, then by setup. A slow hour of the API, or a limit that stops the task, then falls on every setup alike.
fn order<'a>(questions: &[&'a Question], repeats: u32) -> Vec<Planned<'a>> {
    let mut planned = Vec::new();
    for repeat in 1..=repeats {
        for &question in questions {
            for setup in Setup::ALL {
                planned.push(Planned {
                    question,
                    setup,
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
struct Invoke<'a>(&'a dyn Fn(Setup, RunDirs<'_>) -> anyhow::Result<Invocation>);

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
    user: &'a UserEntries,
    /// The indexes as the runs began, which the run of their setup puts back.
    indexes: &'a Indexes,
    /// The wall-clock limit of one run.
    limit: Duration,
    /// Ctrl-C and SIGTERM on the task: a run that waits for one is ended, and not recorded.
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
            if results.holds(&run.question.id, run.setup, run.repeat) {
                continue;
            }
            let label = format!(
                "[{}/{total}] {} {} repeat {}",
                index + 1,
                run.question.id,
                run.setup,
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
            repeat,
        } = planned;
        let what = format!(
            "the run of {} in the `{setup}` setup (repeat {repeat})",
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
        let invocation = (self.invoke.0)(setup, dirs)
            .with_context(|| format!("cannot make the command line of {what}"))?;
        let transcript = self
            .transcripts
            .join(format!("{}-{setup}-{repeat}.jsonl", question.id));

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
        // The init event of the run is held against the setup and against the runs before it, before the run has spent anything.
        let executed = exec::execute(&launch, &mut |init| {
            init::check(setup, init, self.user)?;
            results.accept(init)
        });
        // The checkout and the index of the setup are put back whatever became of the run.
        let restored = guard.finish();
        let index = self.indexes.restore(setup, &question.repository);
        let finished = executed.with_context(|| what.clone())?;
        let changed_checkout = restored.with_context(|| what.clone())?;
        let changed_index = index.with_context(|| what.clone())?;

        let bytes = fs::read(&transcript)
            .with_context(|| format!("cannot read {}", transcript.display()))?;
        // A run that was killed may have been cut in the middle of a character.
        let observed = stream::observe(&String::from_utf8_lossy(&bytes));
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

/// The versions of the tools the runs use, by name: the binary under test, codebase-memory-mcp, and the language server with its TypeScript.
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
    tools.sort_by(|first, second| first.tool.cmp(&second.tool));
    Ok(tools)
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

/// Throws the cache of `tool` away from `dir` with the line that every cold run of the speed benchmark starts with.
fn forget(tool: Tool, dir: &Path, programs: &Programs) -> anyhow::Result<()> {
    let Some(line) = plan::reset(tool, programs) else {
        return Ok(());
    };
    let shell = Shell::new()?;
    shell.change_dir(dir);
    shell
        .cmd("sh")
        .args(["-c", &line])
        .quiet()
        .run()
        .with_context(|| format!("cannot throw the cache of {} away", tool.name()))
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
    // Where earlier versions of the task kept the copy of the binary and the files of the setups: in the directory of the checkouts.
    for earlier in [".bin", ".setups"] {
        let earlier = runs_root.join(earlier);
        if earlier.exists() {
            fs::remove_dir_all(&earlier)
                .with_context(|| format!("cannot remove {}", earlier.display()))?;
        }
    }
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
    // What no run may read: this repository, which holds the sources of the tool under test, and the configuration of the user.
    let private = vec![root.clone(), config.clone()];
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
    let invoke = |setup: Setup, dirs: RunDirs<'_>| setups::invocation(setup, &context, dirs, &host);
    // Every setup is tried once before the first run: a PATH that holds a program no run may find stops the task before it spends anything.
    for setup in Setup::ALL {
        let dirs = RunDirs {
            checkout: &runs_root,
            tmp: &tmp_root,
        };
        invoke(setup, dirs)?;
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
    let session = Session {
        claude: &claude,
        questions: &questions,
        repeats,
        checkouts: &checkouts,
        invoke: Invoke(&invoke),
        tmp_root: &tmp_root,
        user: &user,
        indexes: &indexes,
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
    use crate::agents::exec::fake_claude;
    use crate::agents::guard::repo;
    use crate::agents::init::{event_of, expected};
    use crate::agents::questions::Kind;
    use crate::agents::results::{sample, started};
    use crate::agents::stream::events::{assistant, lines, result};

    const ANSWER: &str = "Found them.\n\n```files\na.ts\n```";

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
        /// The indexes kept aside, for the tests of the runs that change one.
        indexes: Indexes,
    }

    impl Bed {
        /// A bed whose runs answer with `stream`. Every setup prints the init event it should.
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
cat "$d/init-$2.json"
if [ -f "$d/hold" ]; then sleep 30; fi
if [ -f "$d/dirty" ]; then rm "$d/dirty"; echo notes > notes.md; fi
if [ -f "$d/touch-$2" ]; then echo run >> "$(cat "$d/touch-$2")"; fi
if [ -f "$d/stream-$n.jsonl" ]; then cat "$d/stream-$n.jsonl"; else cat "$d/stream.jsonl"; fi
if [ -f "$d/slow-$n" ]; then sleep 30; fi
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
                indexes: Indexes::default(),
            };
            bed.stream(stream);
            bed
        }

        /// A bed whose runs end with `text` as the final message.
        fn answering(text: &str) -> Self {
            Self::new(&lines(&[
                assistant("m1", 5, "looking"),
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

        /// Makes the stand-in do something extra: `hold` and `dirty` in its first call, or `slow-<n>` to stay on after its stream in its `n`th.
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
            let invoke = |setup: Setup, dirs: RunDirs<'_>| -> anyhow::Result<Invocation> {
                self.seen
                    .borrow_mut()
                    .push((dirs.checkout.to_path_buf(), dirs.tmp.to_path_buf()));
                Ok(Invocation {
                    args: vec!["--setup".into(), setup.name().into()],
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
                user: &user,
                indexes: &self.indexes,
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

        let marked: Vec<(Setup, bool)> = results
            .runs
            .iter()
            .map(|run| (run.setup, run.changed_checkout))
            .collect();
        assert_eq!(
            marked,
            [
                (Setup::Plain, false),
                (Setup::Lsp, false),
                (Setup::Cbm, true),
                (Setup::Gsttt, true)
            ]
        );
        for (dir, file) in [(&gsttt, "snapshot.gsttt"), (&cache, "project.db")] {
            assert_eq!(fs::read_to_string(dir.join(file)).unwrap(), "as indexed\n");
        }
    }

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

    #[gsttt_scenario::scenario("agent-benchmarks", "The setups take turns")]
    #[test]
    fn the_setups_take_turns() {
        let first = question("repo-users-1", Kind::Users);
        let second = question("repo-users-2", Kind::Users);
        let planned = order(&[&first, &second], 2);
        let names: Vec<String> = planned
            .iter()
            .map(|run| format!("{} {} {}", run.question.id, run.setup, run.repeat))
            .collect();

        assert_eq!(names.len(), 16);
        assert_eq!(
            names[..4],
            [
                "repo-users-1 plain 1",
                "repo-users-1 lsp 1",
                "repo-users-1 cbm 1",
                "repo-users-1 gsttt 1"
            ]
        );
        assert_eq!(
            names[12..],
            [
                "repo-users-2 plain 2",
                "repo-users-2 lsp 2",
                "repo-users-2 cbm 2",
                "repo-users-2 gsttt 2"
            ]
        );
        // The first repeat is over before the second begins.
        assert!(planned[..8].iter().all(|run| run.repeat == 1));
        assert!(planned[8..].iter().all(|run| run.repeat == 2));
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "A run without an answer scores zero")]
    #[test]
    fn a_run_without_an_answer_scores_zero() {
        let bed = Bed::answering("Done, but I have no list for you.");
        let mut results = started();
        assert_eq!(bed.run(&mut results).unwrap(), 4);

        assert_eq!(results.runs.len(), 4);
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
        let setups: Vec<Setup> = written.runs.iter().map(|run| run.setup).collect();
        assert_eq!(setups, Setup::ALL);
        for run in &written.runs {
            assert_eq!(
                (run.question.as_str(), run.kind, run.repeat),
                ("repo-users-1", Kind::Users, 1)
            );
            assert_eq!(run.outcome, Outcome::Answered);
            assert_eq!(run.answer, ["a.ts"]);
            assert_eq!((run.precision, run.recall), (1.0, 0.5));
            assert!(!run.changed_checkout);
        }
        // The stream of every run is kept.
        let kept = bed
            .dir
            .path()
            .join("results/agents/repo-users-1-gsttt-1.jsonl");
        assert!(fs::read_to_string(kept).unwrap().contains("\"init\""));
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
        let setups: Vec<Setup> = written.runs.iter().map(|run| run.setup).collect();
        assert_eq!(
            setups,
            [Setup::Plain, Setup::Lsp],
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
        results
            .runs
            .push(sample("repo-users-1", Kind::Users, Setup::Plain, 1));
        let first = results.runs[0].clone();

        assert_eq!(bed.run(&mut results).unwrap(), 3);
        assert_eq!(bed.calls(), 3, "every planned run but that one");
        assert_eq!(results.runs.len(), 4);
        assert_eq!(results.runs[0], first, "the finished run is as it was");
        let setups: Vec<Setup> = results.runs.iter().map(|run| run.setup).collect();
        assert_eq!(setups, Setup::ALL);

        // Started over once more, nothing is left to run.
        assert_eq!(bed.run(&mut results).unwrap(), 0);
        assert_eq!(bed.calls(), 3);
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
            "the run was ended before the model was asked"
        );
        assert_eq!(results.runs.len(), 1, "nothing was added");
        assert_eq!(
            fs::read(bed.out()).unwrap(),
            before,
            "the file is as it was"
        );
    }

    #[test]
    fn a_run_that_loaded_something_else_stops_the_task_before_it_spends_anything() {
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
        assert_eq!(marks, [true, false, false, false]);
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
        bed.go(&mut results, Duration::from_secs(8)).unwrap();

        let first = &results.runs[0];
        assert_eq!(first.outcome, Outcome::Answered);
        assert_eq!(first.answer, ["a.ts"]);
        // The time of the run ends at the result, which was written at once; the wait for the exit is no part of it.
        assert!(first.duration_ms < 4000, "{}", first.duration_ms);
        assert!(started_at.elapsed() < Duration::from_secs(60));
    }

    #[test]
    fn a_run_stopped_at_the_limit_keeps_the_tokens_it_streamed() {
        let bed = Bed::answering(ANSWER);
        // Only the first run is slow, and has streamed one message when it is stopped. The limit leaves the stand-in room to start on a busy machine.
        bed.mark("slow-1");
        bed.stream_of_call(1, &lines(&[assistant("m1", 5, "still looking")]));
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
        assert!(first.duration_ms >= 4000);
        // The runs after it are made and judged as usual.
        assert_eq!(results.runs.len(), 4);
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
        assert_eq!(bed.run(&mut results).unwrap(), 4);

        // Each of the four runs was given a directory of its own, made in the root of the temporary directories and outside the checkout, and the stand-in found it in `TMPDIR` and made a file in it.
        let seen = bed.seen.borrow().clone();
        assert_eq!(seen.len(), 4);
        let given: BTreeSet<&PathBuf> = seen.iter().map(|(_, tmp)| tmp).collect();
        assert_eq!(given.len(), 4, "a directory was shared by two runs");
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
        assert!(error.contains("repo-users-1-plain-1.jsonl"), "{error}");
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
        assert_eq!(stream::observe(&stream).end, End::Success);
    }
}
