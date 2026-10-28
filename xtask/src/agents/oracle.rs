//! The test files a function breaks: the JSON report of the test command with a `throw` as the first statement of the function, against the same command without it. Written for Vitest, whose `--reporter=json --outputFile=<file>` flags it appends.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail, ensure};
use serde::Deserialize;

use super::draw::{BROKEN, Verdict};
use super::functions::Candidate;
use super::process::kill_group;
use crate::dist::git::git;

/// The wall-clock limit of each of the two runs without a throw.
const BASELINE_LIMIT: Duration = Duration::from_mins(15);

/// The shortest wall-clock limit of a run with a throw.
const MIN_LIMIT: Duration = Duration::from_mins(1);

/// How many times the slower run without a throw a run with a throw may take: a function that makes a test wait for something that never comes is not worth the wait.
const SLACK: u32 = 3;

/// How often a running test command is looked at.
const POLL: Duration = Duration::from_millis(50);

const NO_REPORT: &str = "a run wrote no report";
const NO_TEST_BREAKS: &str = "no test file breaks";
const TOO_MANY_BREAK: &str = "more than 30 test files break";
const NOT_TWICE: &str = "no test file breaks in both runs";

/// The test files one run reported, relative to the checkout.
#[derive(Debug, Default, PartialEq, Eq)]
struct Files {
    all: BTreeSet<String>,
    failing: BTreeSet<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    test_results: Vec<FileResult>,
}

#[derive(Debug, Deserialize)]
struct FileResult {
    name: String,
    status: String,
}

/// The test files of a Vitest JSON report, and the ones that did not pass. `checkout` is canonical, as the paths of the report are.
fn files_of(report: &str, checkout: &Path) -> anyhow::Result<Files> {
    let report: Report = serde_json::from_str(report).context("the report is not Vitest's JSON")?;
    let mut files = Files::default();
    for result in report.test_results {
        let relative = Path::new(&result.name)
            .strip_prefix(checkout)
            .ok()
            .and_then(Path::to_str)
            .with_context(|| {
                format!(
                    "the test file {} is not under {}",
                    result.name,
                    checkout.display()
                )
            })?
            .to_owned();
        if result.status != "passed" {
            files.failing.insert(relative.clone());
        }
        files.all.insert(relative);
    }
    Ok(files)
}

/// What the two runs without a throw agree on.
#[derive(Debug, PartialEq, Eq)]
struct Baseline {
    /// The test files both runs reported.
    all: BTreeSet<String>,
    /// The test files that failed in either run: they fail without any throw, so a throw proves nothing about them.
    failing: BTreeSet<String>,
}

impl Baseline {
    fn of(first: &Files, second: &Files) -> Self {
        Self {
            all: first.all.intersection(&second.all).cloned().collect(),
            failing: first.failing.union(&second.failing).cloned().collect(),
        }
    }
}

/// The test files that failed in both runs with the throw and passed in both runs without it.
fn broken(
    baseline: &Baseline,
    first: &BTreeSet<String>,
    second: &BTreeSet<String>,
) -> BTreeSet<String> {
    first
        .intersection(second)
        .filter(|file| baseline.all.contains(*file) && !baseline.failing.contains(*file))
        .cloned()
        .collect()
}

/// The test command run with and without a throw in one checkout.
#[derive(Debug)]
pub(super) struct Oracle<'a> {
    checkout: &'a Path,
    command: &'a [String],
    /// Where the reports and the logs of the runs go: outside the checkout, so that they are no trace in it.
    work: PathBuf,
    baseline: Baseline,
    /// The wall-clock limit of a run with a throw.
    limit: Duration,
    slowest: Duration,
}

impl<'a> Oracle<'a> {
    /// Runs the test command twice without a throw in `checkout`, which is canonical. Both runs must write a report; a failing test is no reason to stop.
    pub(super) fn new(
        checkout: &'a Path,
        command: &'a [String],
        work: &Path,
    ) -> anyhow::Result<Self> {
        fs::create_dir_all(work).with_context(|| format!("cannot create {}", work.display()))?;
        let mut oracle = Self {
            checkout,
            command,
            work: work.to_path_buf(),
            baseline: Baseline {
                all: BTreeSet::new(),
                failing: BTreeSet::new(),
            },
            limit: MIN_LIMIT,
            slowest: Duration::ZERO,
        };
        let (first, took_first) = oracle.baseline_run("baseline-1")?;
        let (second, took_second) = oracle.baseline_run("baseline-2")?;
        oracle.baseline = Baseline::of(&first, &second);
        oracle.slowest = took_first.max(took_second);
        oracle.limit = oracle.slowest.saturating_mul(SLACK).max(MIN_LIMIT);
        Ok(oracle)
    }

    fn baseline_run(&self, label: &str) -> anyhow::Result<(Files, Duration)> {
        let started = Instant::now();
        let files = self.run(label, BASELINE_LIMIT)?.with_context(|| {
            format!(
                "the test command wrote no usable report without a throw; see {}",
                self.work.join(format!("{label}.log")).display()
            )
        })?;
        Ok((files, started.elapsed()))
    }

    /// The test files that fail without any throw.
    pub(super) fn ignored(&self) -> &BTreeSet<String> {
        &self.baseline.failing
    }

    /// One line about the runs without a throw.
    pub(super) fn describe(&self) -> String {
        format!(
            "{} test files, {} of them fail without a throw; the slower run took {} s, a run with a throw may take {} s",
            self.baseline.all.len(),
            self.baseline.failing.len(),
            self.slowest.as_secs(),
            self.limit.as_secs()
        )
    }

    /// The test files the function breaks, or why it is not drawn. One to 30 files must fail that passed in both runs without a throw, and then fail again in a second run with it; the answer is the files that failed in both.
    pub(super) fn answer(&self, candidate: &Candidate) -> anyhow::Result<Verdict> {
        let Some(first) = self.thrown(candidate, "throw-1")? else {
            return Ok(Err(NO_REPORT));
        };
        let once = broken(&self.baseline, &first.failing, &first.failing);
        if once.is_empty() {
            return Ok(Err(NO_TEST_BREAKS));
        }
        if once.len() > *BROKEN.end() {
            return Ok(Err(TOO_MANY_BREAK));
        }
        let Some(second) = self.thrown(candidate, "throw-2")? else {
            return Ok(Err(NO_REPORT));
        };
        let twice = broken(&self.baseline, &first.failing, &second.failing);
        if twice.is_empty() {
            return Ok(Err(NOT_TWICE));
        }
        Ok(Ok(twice.into_iter().collect()))
    }

    /// One run with the throw in the function. The file is restored with git whatever happened, and the run fails if `git status --porcelain` is not what it was before the throw.
    fn thrown(&self, candidate: &Candidate, label: &str) -> anyhow::Result<Option<Files>> {
        let offset = candidate.body_offset.with_context(|| {
            format!("{} in {} has no block body", candidate.name, candidate.file)
        })?;
        let before = git(self.checkout, &["status", "--porcelain"])?;
        let run = insert_throw(
            &self.checkout.join(&candidate.file),
            offset,
            &candidate.name,
        )
        .and_then(|()| self.run(label, self.limit));
        let restored = git(
            self.checkout,
            &["checkout", "--quiet", "--", candidate.file.as_str()],
        );
        let after = git(self.checkout, &["status", "--porcelain"])?;
        restored.with_context(|| format!("cannot restore {}", candidate.file))?;
        ensure!(
            after == before,
            "after the throw in {} the checkout is not what it was: `git status --porcelain` printed\n{before}\nand prints now\n{after}",
            candidate.file
        );
        run
    }

    /// Runs the test command once and reads its report; `None` when the limit ran out or no report was written.
    fn run(&self, label: &str, limit: Duration) -> anyhow::Result<Option<Files>> {
        let report = self.work.join(format!("{label}.json"));
        match fs::remove_file(&report) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(error).with_context(|| format!("cannot remove {}", report.display()));
            }
            _ => {}
        }
        let flags = [
            "--reporter=json".to_owned(),
            format!("--outputFile={}", report.display()),
        ];
        let log = self.work.join(format!("{label}.log"));
        if !run_limited(self.command, &flags, self.checkout, &log, limit)? {
            return Ok(None);
        }
        Ok(fs::read_to_string(&report)
            .ok()
            .and_then(|text| files_of(&text, self.checkout).ok()))
    }
}

/// Inserts `throw new Error("gsttt agents: <name>");` at `offset`, which must be just after the `{` of a body.
fn insert_throw(file: &Path, offset: usize, name: &str) -> anyhow::Result<()> {
    let bytes = fs::read(file).with_context(|| format!("cannot read {}", file.display()))?;
    let Some((head, tail)) = bytes.split_at_checked(offset) else {
        bail!("offset {offset} is past the end of {}", file.display());
    };
    ensure!(
        head.last() == Some(&b'{'),
        "the byte before offset {offset} of {} is not a `{{`; the file is not the one the offset was taken from",
        file.display()
    );
    let throw = format!("throw new Error(\"gsttt agents: {name}\");");
    fs::write(file, [head, throw.as_bytes(), tail].concat())
        .with_context(|| format!("cannot write {}", file.display()))
}

/// Runs `command` with `extra` arguments in `dir`, its output in `log`, and kills its whole process group after `limit`. Returns whether it finished in time.
fn run_limited(
    command: &[String],
    extra: &[String],
    dir: &Path,
    log: &Path,
    limit: Duration,
) -> anyhow::Result<bool> {
    let (program, args) = command.split_first().context("the test command is empty")?;
    let out = File::create(log).with_context(|| format!("cannot create {}", log.display()))?;
    let mut child = Command::new(program)
        .args(args)
        .args(extra)
        .current_dir(dir)
        .env("CI", "true")
        .stdin(Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out)
        .process_group(0)
        .spawn()
        .with_context(|| format!("cannot run `{program}` in {}", dir.display()))?;
    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            return Ok(true);
        }
        if started.elapsed() >= limit {
            kill_group(&mut child)?;
            return Ok(false);
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set<const N: usize>(files: [&str; N]) -> BTreeSet<String> {
        files.into_iter().map(str::to_owned).collect()
    }

    fn files(all: &[&str], failing: &[&str]) -> Files {
        Files {
            all: all.iter().map(|f| (*f).to_owned()).collect(),
            failing: failing.iter().map(|f| (*f).to_owned()).collect(),
        }
    }

    #[test]
    fn a_report_names_its_files_relative_to_the_checkout() {
        let report = r#"{"numTotalTests":3,"testResults":[
            {"name":"/c/a.spec.ts","status":"passed","assertionResults":[]},
            {"name":"/c/b/b.spec.ts","status":"failed"},
            {"name":"/c/s.spec.ts","status":"skipped"}]}"#;
        let found = files_of(report, Path::new("/c")).unwrap();
        assert_eq!(found.all, set(["a.spec.ts", "b/b.spec.ts", "s.spec.ts"]));
        assert_eq!(found.failing, set(["b/b.spec.ts", "s.spec.ts"]));
        let error = files_of(report, Path::new("/elsewhere"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("/c/a.spec.ts"), "{error}");
        assert!(files_of("{", Path::new("/c")).is_err());
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "Only the files the throw breaks are the answer"
    )]
    #[test]
    fn only_the_files_that_fail_with_the_throw_twice_and_never_without_it_are_the_answer() {
        let all = ["a.spec.ts", "b.spec.ts", "c.spec.ts"];
        // c.spec.ts fails in one of the two runs without the throw.
        let baseline = Baseline::of(&files(&all, &["c.spec.ts"]), &files(&all, &[]));
        // a.spec.ts fails in both runs with the throw, b.spec.ts in one, c.spec.ts in both.
        let first = set(["a.spec.ts", "b.spec.ts", "c.spec.ts"]);
        let second = set(["a.spec.ts", "c.spec.ts"]);
        assert_eq!(broken(&baseline, &first, &second), set(["a.spec.ts"]));
        assert_eq!(broken(&baseline, &second, &first), set(["a.spec.ts"]));
        assert_eq!(baseline.failing, set(["c.spec.ts"]));
    }

    #[test]
    fn a_file_the_baselines_did_not_both_report_is_no_answer() {
        let baseline = Baseline::of(&files(&["a.spec.ts"], &[]), &files(&["a.spec.ts"], &[]));
        let thrown = set(["a.spec.ts", "new.spec.ts"]);
        assert_eq!(broken(&baseline, &thrown, &thrown), set(["a.spec.ts"]));
    }

    #[test]
    fn a_run_that_outlives_its_limit_is_killed() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let sleeper = ["sh", "-c", "sleep 30"].map(str::to_owned);
        let started = Instant::now();
        let finished =
            run_limited(&sleeper, &[], dir.path(), &log, Duration::from_millis(300)).unwrap();
        assert!(!finished);
        assert!(started.elapsed() < Duration::from_secs(20));
        let quick = ["sh", "-c", "echo hi"].map(str::to_owned);
        assert!(run_limited(&quick, &[], dir.path(), &log, Duration::from_secs(30)).unwrap());
        assert_eq!(fs::read_to_string(&log).unwrap(), "hi\n");
    }

    /// The first statement of the body is inserted at the offset and nowhere else.
    #[test]
    fn the_throw_goes_right_after_the_brace() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("lib.ts");
        fs::write(&file, "export function f() {\n  return 1\n}\n").unwrap();
        insert_throw(&file, 21, "f").unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "export function f() {throw new Error(\"gsttt agents: f\");\n  return 1\n}\n"
        );
        assert!(insert_throw(&file, 20, "f").is_err(), "not after a brace");
        assert!(insert_throw(&file, 10_000, "f").is_err(), "past the end");
    }

    /// A git repository with `lib.ts`, whose test command is a shell script that reports `a.spec.ts` as failed while `lib.ts` holds a throw. `extra` runs in the script when it does.
    struct Repo {
        _guard: tempfile::TempDir,
        dir: PathBuf,
        work: PathBuf,
        command: Vec<String>,
    }

    const LIB: &str = "export function f() {\n  return 1\n}\n";

    fn repo(under_throw: &str) -> Repo {
        let guard = tempfile::tempdir().unwrap();
        let dir = guard.path().join("repo");
        fs::create_dir(&dir).unwrap();
        let dir = fs::canonicalize(dir).unwrap();
        fs::write(dir.join("lib.ts"), LIB).unwrap();
        for args in [
            &["init", "--quiet"][..],
            &["config", "user.email", "t@example.test"],
            &["config", "user.name", "t"],
            &["add", "lib.ts"],
            &[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "init",
            ],
        ] {
            git(&dir, args).unwrap();
        }
        let script = format!(
            r#"out="${{2#--outputFile=}}"
status=passed
if grep -q 'gsttt agents' lib.ts; then status=failed; {under_throw}; fi
printf '{{"testResults":[{{"name":"%s/a.spec.ts","status":"%s"}}]}}' "$(pwd -P)" "$status" > "$out""#
        );
        Repo {
            work: guard.path().join("work"),
            _guard: guard,
            dir,
            command: ["sh", "-c", script.as_str(), "sh"]
                .map(str::to_owned)
                .to_vec(),
        }
    }

    fn function() -> Candidate {
        Candidate {
            file: "lib.ts".to_owned(),
            name: "f".to_owned(),
            line: 1,
            body_offset: Some(21),
            users: Vec::new(),
        }
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "A throw leaves no trace in the checkout")]
    #[test]
    fn a_throw_leaves_no_trace_in_the_checkout() {
        let repo = repo(":");
        let oracle = Oracle::new(&repo.dir, &repo.command, &repo.work).unwrap();
        assert!(oracle.ignored().is_empty());
        let answer = oracle.answer(&function()).unwrap();
        assert_eq!(
            answer,
            Ok(vec!["a.spec.ts".to_owned()]),
            "the report of the throw run is read"
        );
        assert_eq!(git(&repo.dir, &["status", "--porcelain"]).unwrap(), "");
        assert_eq!(fs::read_to_string(repo.dir.join("lib.ts")).unwrap(), LIB);
    }

    #[test]
    fn a_run_that_writes_no_report_rejects_the_candidate_and_leaves_no_trace() {
        let repo = repo("exit 1");
        let oracle = Oracle::new(&repo.dir, &repo.command, &repo.work).unwrap();
        assert_eq!(oracle.answer(&function()).unwrap(), Err(NO_REPORT));
        assert_eq!(git(&repo.dir, &["status", "--porcelain"]).unwrap(), "");
        assert_eq!(fs::read_to_string(repo.dir.join("lib.ts")).unwrap(), LIB);
    }

    #[test]
    fn a_test_command_that_dirties_the_checkout_fails_the_run_and_the_file_is_restored() {
        let repo = repo("echo stray > stray.txt");
        let oracle = Oracle::new(&repo.dir, &repo.command, &repo.work).unwrap();
        let error = oracle.answer(&function()).unwrap_err().to_string();
        assert!(error.contains("not what it was"), "{error}");
        assert!(error.contains("stray.txt"), "{error}");
        assert_eq!(fs::read_to_string(repo.dir.join("lib.ts")).unwrap(), LIB);
    }

    #[test]
    fn a_function_no_test_reaches_is_rejected_without_a_second_run() {
        // The script reports `a.spec.ts` as passed whatever `lib.ts` holds.
        let mut repo = repo(":");
        repo.command = ["sh", "-c", r#"printf '{"testResults":[{"name":"%s/a.spec.ts","status":"passed"}]}' "$(pwd -P)" > "${2#--outputFile=}""#, "sh"].map(str::to_owned).to_vec();
        let oracle = Oracle::new(&repo.dir, &repo.command, &repo.work).unwrap();
        assert_eq!(oracle.answer(&function()).unwrap(), Err(NO_TEST_BREAKS));
        assert!(!repo.work.join("throw-2.json").exists());
    }
}
