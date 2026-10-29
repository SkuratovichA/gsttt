//! The test files a function breaks: the JSON report of the test command with a `throw` as the first statement of the function, against the same command without it. Written for Vitest, whose `--reporter=json --outputFile=<file>` flags it appends.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail, ensure};
use serde::Deserialize;

use super::draw::{Answer, BROKEN, Verdict};
use super::functions::Candidate;
use super::process::{self, Job, Look, Stop};
use crate::dist::git::git;

/// The wall-clock limit of each of the two runs without a throw.
const BASELINE_LIMIT: Duration = Duration::from_mins(15);

/// The shortest wall-clock limit of a run with a throw.
const MIN_LIMIT: Duration = Duration::from_mins(1);

/// How many times the slower run without a throw a run with a throw may take: a function that makes a test wait for something that never comes is not worth the wait.
const SLACK: u32 = 3;

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

    /// Whether both runs without a throw reported `file` and it passed in both.
    fn passes(&self, file: &str) -> bool {
        self.all.contains(file) && !self.failing.contains(file)
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
        .filter(|file| baseline.passes(file))
        .cloned()
        .collect()
}

/// The test files that passed in both runs without a throw and failed in only one of the two runs with it: the throw may break them or the run may, so they are neither in the answer nor wrong to name.
fn unstable(
    baseline: &Baseline,
    first: &BTreeSet<String>,
    second: &BTreeSet<String>,
) -> BTreeSet<String> {
    first
        .symmetric_difference(second)
        .filter(|file| baseline.passes(file))
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
    stop: Stop,
}

impl<'a> Oracle<'a> {
    /// Runs the test command twice without a throw in `checkout`, which is canonical. Both runs must write a report of at least one test file; a failing test is no reason to stop.
    pub(super) fn new(
        checkout: &'a Path,
        command: &'a [String],
        work: &Path,
        stop: &Stop,
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
            stop: stop.clone(),
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
        let log = self.work.join(format!("{label}.log"));
        let files = self.run(label, BASELINE_LIMIT)?.with_context(|| {
            format!(
                "the test command wrote no report without a throw, or ran out of time; see {}",
                log.display()
            )
        })?;
        ensure!(
            !files.all.is_empty(),
            "the test command ran no test file without a throw; see {}",
            log.display()
        );
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

    /// The test files the function breaks, or why it is not drawn. `BROKEN` many files must fail that passed in both runs without a throw, and then fail again in a second run with it; the answer is the files that failed in both, and the files that failed in only one of the two runs with the throw are returned beside it as ignored.
    pub(super) fn answer(&self, candidate: &Candidate) -> anyhow::Result<Verdict> {
        let Some(first) = self.thrown(candidate, "throw-1")? else {
            return Ok(Err(NO_REPORT));
        };
        let once = broken(&self.baseline, &first.failing, &first.failing);
        if !BROKEN.contains(&once.len()) {
            return Ok(Err(if once.len() < *BROKEN.start() {
                NO_TEST_BREAKS
            } else {
                TOO_MANY_BREAK
            }));
        }
        let Some(second) = self.thrown(candidate, "throw-2")? else {
            return Ok(Err(NO_REPORT));
        };
        let twice = broken(&self.baseline, &first.failing, &second.failing);
        if !BROKEN.contains(&twice.len()) {
            return Ok(Err(NOT_TWICE));
        }
        Ok(Ok(Answer {
            files: twice.into_iter().collect(),
            ignored: unstable(&self.baseline, &first.failing, &second.failing)
                .into_iter()
                .collect(),
        }))
    }

    /// One run with the throw in the function. The file is restored with git whatever happened, a panic included, and the run fails if `git status --porcelain` is not what it was before the throw.
    fn thrown(&self, candidate: &Candidate, label: &str) -> anyhow::Result<Option<Files>> {
        let offset = candidate.body_offset.with_context(|| {
            format!("{} in {} has no block body", candidate.name, candidate.file)
        })?;
        let before = git(self.checkout, &["status", "--porcelain"])?;
        let restore = Restore::new(self.checkout, &candidate.file);
        let run = insert_throw(
            &self.checkout.join(&candidate.file),
            offset,
            &candidate.name,
        )
        .and_then(|()| self.run(label, self.limit));
        let restored = restore.now();
        let after = git(self.checkout, &["status", "--porcelain"])?;
        restored.with_context(|| format!("cannot restore {}", candidate.file))?;
        ensure!(
            after == before,
            "after the throw in {} the checkout is not what it was: `git status --porcelain` printed\n{before}\nand prints now\n{after}",
            candidate.file
        );
        run
    }

    /// Runs the test command once and reads its report: `None` when the limit ran out or no report was written, and an error when one was written that cannot be used.
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
        if !run_limited(self.command, &flags, self.checkout, &log, limit, &self.stop)? {
            return Ok(None);
        }
        let text = match fs::read_to_string(&report) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| format!("cannot read {}", report.display()));
            }
        };
        let files = files_of(&text, self.checkout)
            .with_context(|| format!("cannot use the report {}", report.display()))?;
        Ok(Some(files))
    }
}

/// Takes a file of the checkout back from git when it is dropped, so that a throw inserted into it outlives no way out of the run, a panic included. A signal is no way out: the wait ends at it and the run returns like any other.
#[derive(Debug)]
struct Restore<'a> {
    checkout: &'a Path,
    file: &'a str,
    restored: bool,
}

impl<'a> Restore<'a> {
    fn new(checkout: &'a Path, file: &'a str) -> Self {
        Self {
            checkout,
            file,
            restored: false,
        }
    }

    fn checkout(&self) -> anyhow::Result<String> {
        git(self.checkout, &["checkout", "--quiet", "--", self.file])
    }

    /// Restores the file now, and whether git could is the result; the drop then has nothing left to do.
    fn now(mut self) -> anyhow::Result<()> {
        self.restored = true;
        self.checkout().map(drop)
    }
}

impl Drop for Restore<'_> {
    fn drop(&mut self) {
        if self.restored {
            return;
        }
        // The run is already ending in a panic or an error, so a message is all there is to report a failure to.
        if let Err(error) = self.checkout() {
            writeln!(
                std::io::stderr().lock(),
                "cannot restore {}: {error:#}",
                self.file
            )
            .ok();
        }
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

/// Runs `command` with `extra` arguments in `dir`, its output in `log`, and ends its whole process group, and every process that works in `dir`, when it exits or after `limit`. Returns whether it finished in time.
fn run_limited(
    command: &[String],
    extra: &[String],
    dir: &Path,
    log: &Path,
    limit: Duration,
    stop: &Stop,
) -> anyhow::Result<bool> {
    let (program, args) = command.split_first().context("the test command is empty")?;
    let out = File::create(log).with_context(|| format!("cannot create {}", log.display()))?;
    let mut test = Command::new(program);
    test.args(args)
        .args(extra)
        .current_dir(dir)
        .env("CI", "true")
        .stdin(Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out);
    let finished = process::run(
        Job {
            command: test,
            input: None,
            dirs: &[dir],
            limit,
            stop,
        },
        &mut || Ok(Look::Working),
    )
    .with_context(|| format!("cannot run `{program}` in {}", dir.display()))?;
    Ok(!finished.timed_out)
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
    fn a_file_is_unstable_when_one_throw_run_fails_it_and_the_baseline_passes_it() {
        let all = ["a.spec.ts", "b.spec.ts", "c.spec.ts", "d.spec.ts"];
        // c.spec.ts fails without the throw in one of the two baseline runs.
        let baseline = Baseline::of(&files(&all, &["c.spec.ts"]), &files(&all, &[]));
        // b.spec.ts fails in the first run with the throw only, d.spec.ts in the second only, c.spec.ts in the first only, and new.spec.ts, which the baselines did not report, in the first only.
        let first = set(["a.spec.ts", "b.spec.ts", "c.spec.ts", "new.spec.ts"]);
        let second = set(["a.spec.ts", "d.spec.ts"]);
        let expected = set(["b.spec.ts", "d.spec.ts"]);
        assert_eq!(unstable(&baseline, &first, &second), expected);
        assert_eq!(unstable(&baseline, &second, &first), expected);
        assert_eq!(broken(&baseline, &first, &second), set(["a.spec.ts"]));
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
        let finished = run_limited(
            &sleeper,
            &[],
            dir.path(),
            &log,
            Duration::from_millis(300),
            &Stop::default(),
        )
        .unwrap();
        assert!(!finished);
        assert!(started.elapsed() < Duration::from_secs(20));
        let quick = ["sh", "-c", "echo hi"].map(str::to_owned);
        assert!(
            run_limited(
                &quick,
                &[],
                dir.path(),
                &log,
                Duration::from_secs(30),
                &Stop::default()
            )
            .unwrap()
        );
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

    /// A git repository with `lib.ts`, whose test command is a shell script that reports `a.spec.ts` as failed while `lib.ts` holds a throw. `under_throw` runs in the script when it does.
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
        let body = format!(
            r#"out="${{2#--outputFile=}}"
status=passed
if grep -q 'gsttt agents' lib.ts; then status=failed; {under_throw}; fi
printf '{{"testResults":[{{"name":"%s/a.spec.ts","status":"%s"}}]}}' "$(pwd -P)" "$status" > "$out""#
        );
        Repo {
            work: guard.path().join("work"),
            _guard: guard,
            dir,
            command: script(&body),
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

    /// The test command of a repository, as a shell script that reads the flag `--outputFile=<report>` as `$2`.
    fn script(body: &str) -> Vec<String> {
        ["sh", "-c", body, "sh"].map(str::to_owned).to_vec()
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "A throw leaves no trace in the checkout")]
    #[test]
    fn a_throw_leaves_no_trace_in_the_checkout() {
        let repo = repo(":");
        let oracle = Oracle::new(&repo.dir, &repo.command, &repo.work, &Stop::default()).unwrap();
        assert!(oracle.ignored().is_empty());
        let answer = oracle.answer(&function()).unwrap();
        assert_eq!(
            answer,
            Ok(Answer {
                files: vec!["a.spec.ts".to_owned()],
                ignored: Vec::new()
            }),
            "the report of the throw run is read"
        );
        assert_eq!(git(&repo.dir, &["status", "--porcelain"]).unwrap(), "");
        assert_eq!(fs::read_to_string(repo.dir.join("lib.ts")).unwrap(), LIB);
    }

    #[test]
    fn a_run_that_writes_no_report_rejects_the_candidate_and_leaves_no_trace() {
        let repo = repo("exit 1");
        let oracle = Oracle::new(&repo.dir, &repo.command, &repo.work, &Stop::default()).unwrap();
        assert_eq!(oracle.answer(&function()).unwrap(), Err(NO_REPORT));
        assert_eq!(git(&repo.dir, &["status", "--porcelain"]).unwrap(), "");
        assert_eq!(fs::read_to_string(repo.dir.join("lib.ts")).unwrap(), LIB);
    }

    #[test]
    fn a_test_command_that_dirties_the_checkout_fails_the_run_and_the_file_is_restored() {
        let repo = repo("echo stray > stray.txt");
        let oracle = Oracle::new(&repo.dir, &repo.command, &repo.work, &Stop::default()).unwrap();
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
        let oracle = Oracle::new(&repo.dir, &repo.command, &repo.work, &Stop::default()).unwrap();
        assert_eq!(oracle.answer(&function()).unwrap(), Err(NO_TEST_BREAKS));
        assert!(!repo.work.join("throw-2.json").exists());
    }

    #[test]
    fn more_than_thirty_broken_files_reject_the_function_without_a_second_run() {
        let mut repo = repo(":");
        repo.command = script(
            r#"out="${2#--outputFile=}"
status=passed
if grep -q 'gsttt agents' lib.ts; then status=failed; fi
{ printf '{"testResults":['
  for n in $(seq 1 31); do
    [ "$n" -gt 1 ] && printf ','
    printf '{"name":"%s/f%s.spec.ts","status":"%s"}' "$(pwd -P)" "$n" "$status"
  done
  printf ']}'; } > "$out""#,
        );
        let oracle = Oracle::new(&repo.dir, &repo.command, &repo.work, &Stop::default()).unwrap();
        assert_eq!(oracle.answer(&function()).unwrap(), Err(TOO_MANY_BREAK));
        assert!(!repo.work.join("throw-2.json").exists());
    }

    /// `both.spec.ts` fails in every run with the throw, `first.spec.ts` in the first only, and `second.spec.ts` in the second only; all three pass without it.
    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A file the throw breaks in one run only is ignored"
    )]
    #[test]
    fn a_file_that_fails_in_one_of_the_two_runs_with_the_throw_is_ignored_and_not_the_answer() {
        let mut repo = repo(":");
        // The counter of the runs with a throw is outside the checkout: a file in it would be a trace the oracle refuses.
        repo.command = script(
            r#"out="${2#--outputFile=}"
here="$(pwd -P)"
both=passed; first=passed; second=passed
if grep -q 'gsttt agents' lib.ts; then
  echo . >> ../throws
  both=failed
  case "$(wc -l < ../throws | tr -d ' ')" in 1) first=failed ;; 2) second=failed ;; esac
fi
printf '{"testResults":[{"name":"%s/both.spec.ts","status":"%s"},{"name":"%s/first.spec.ts","status":"%s"},{"name":"%s/second.spec.ts","status":"%s"}]}' "$here" "$both" "$here" "$first" "$here" "$second" > "$out""#,
        );
        let oracle = Oracle::new(&repo.dir, &repo.command, &repo.work, &Stop::default()).unwrap();
        assert!(oracle.ignored().is_empty());
        let answer = oracle.answer(&function()).unwrap();
        assert_eq!(
            answer,
            Ok(Answer {
                files: vec!["both.spec.ts".to_owned()],
                ignored: vec!["first.spec.ts".to_owned(), "second.spec.ts".to_owned()]
            })
        );
        assert_eq!(git(&repo.dir, &["status", "--porcelain"]).unwrap(), "");
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "A report that cannot be read stops the draw")]
    #[test]
    fn a_report_that_cannot_be_used_is_an_error_that_says_why() {
        // Without a throw: the report is not JSON.
        let mut repo_without = repo(":");
        repo_without.command = script(r#"printf '{' > "${2#--outputFile=}""#);
        let error = Oracle::new(
            &repo_without.dir,
            &repo_without.command,
            &repo_without.work,
            &Stop::default(),
        )
        .unwrap_err();
        let error = format!("{error:#}");
        assert!(error.contains("baseline-1.json"), "{error}");
        assert!(error.contains("not Vitest's JSON"), "{error}");

        // With the throw: the same, and the file is restored all the same.
        let repo = repo(r#"printf '{' > "$out"; exit 0"#);
        let oracle = Oracle::new(&repo.dir, &repo.command, &repo.work, &Stop::default()).unwrap();
        let error = format!("{:#}", oracle.answer(&function()).unwrap_err());
        assert!(error.contains("throw-1.json"), "{error}");
        assert!(error.contains("not Vitest's JSON"), "{error}");
        assert_eq!(git(&repo.dir, &["status", "--porcelain"]).unwrap(), "");
        assert_eq!(fs::read_to_string(repo.dir.join("lib.ts")).unwrap(), LIB);
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A run without a throw that reports no test file stops the draw"
    )]
    #[test]
    fn a_test_command_that_runs_no_test_file_is_refused_naming_its_log() {
        let mut repo = repo(":");
        repo.command = script(r#"printf '{"testResults":[]}' > "${2#--outputFile=}""#);
        let error = Oracle::new(&repo.dir, &repo.command, &repo.work, &Stop::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("ran no test file"), "{error}");
        assert!(error.contains("baseline-1.log"), "{error}");
    }

    /// The throw is in the file while the guard lives, and not after it is dropped, whether the run ended in an error or in a panic.
    #[gsttt_scenario::scenario("agent-benchmarks", "A throw is taken back when the run fails")]
    #[test]
    fn a_throw_is_taken_back_when_the_run_ends_in_an_error_or_a_panic() {
        let repo = repo(":");
        let file = repo.dir.join("lib.ts");
        let thrown = || {
            let restore = Restore::new(&repo.dir, "lib.ts");
            insert_throw(&file, 21, "f").unwrap();
            assert!(fs::read_to_string(&file).unwrap().contains("gsttt agents"));
            restore
        };
        let failing = || -> anyhow::Result<()> {
            let _restore = thrown();
            bail!("the run failed")
        };
        assert!(failing().is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), LIB);

        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _restore = thrown();
            panic!("the run panicked");
        }));
        assert!(panicked.is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), LIB);
        assert_eq!(git(&repo.dir, &["status", "--porcelain"]).unwrap(), "");
    }
}
