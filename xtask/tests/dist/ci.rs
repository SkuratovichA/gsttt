//! The `ci` workflow, read as data.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Map, Value, json};

use crate::support::{
    RELEASE_TARGETS, TARGET, commit, git, isolate_git, package, printed, repository,
    workspace_root, write, write_executable,
};

pub(crate) fn workflow() -> Value {
    let text = std::fs::read_to_string(workspace_root().join(".github/workflows/ci.yml")).unwrap();
    serde_norway::from_str(&text).unwrap()
}

fn jobs(workflow: &Value) -> &Map<String, Value> {
    workflow["jobs"].as_object().unwrap()
}

/// The lines of the `run` scripts of the steps of `job`.
fn commands(job: &Value) -> Vec<&str> {
    job["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|step| step["run"].as_str())
        .flat_map(str::lines)
        .collect()
}

/// The step of `job` named `name`.
pub(crate) fn step<'a>(job: &'a Value, name: &str) -> Option<&'a Value> {
    job["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["name"] == name)
}

/// Whether `step` is given a token of the repository: in its own `env`, whatever the variable is called.
fn holds_a_token(step: &Value) -> bool {
    step["env"].as_object().is_some_and(|env| {
        env.values().filter_map(Value::as_str).any(|value| {
            value.contains("github.token")
                || value.contains("secrets.")
                || value.contains("GITHUB_TOKEN")
        })
    })
}

/// The matrix entries of `job`, none for a job without a matrix.
fn matrix(job: &Value) -> Vec<&Value> {
    job.pointer("/strategy/matrix/include")
        .and_then(Value::as_array)
        .map(|entries| entries.iter().collect())
        .unwrap_or_default()
}

/// The runner images `job` can run on: its `runs-on`, or the `os` of every matrix entry when `runs-on` takes it from there.
fn runners(job: &Value) -> Vec<&str> {
    let runs_on = job["runs-on"].as_str().unwrap();
    if runs_on == "${{ matrix.os }}" {
        matrix(job)
            .into_iter()
            .filter_map(|entry| entry["os"].as_str())
            .collect()
    } else {
        vec![runs_on]
    }
}

fn needs(job: &Value) -> Vec<&str> {
    match &job["needs"] {
        Value::String(name) => vec![name.as_str()],
        Value::Array(names) => names.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

/// Whether `job` needs the job `checks`, directly or through the jobs it needs.
fn needs_checks(jobs: &Map<String, Value>, job: &Value) -> bool {
    needs(job).into_iter().any(|name| {
        name == "checks"
            || jobs
                .get(name)
                .is_some_and(|needed| needs_checks(jobs, needed))
    })
}

/// Whether `job` is limited to pushes, so that it does not run on a pull request.
fn only_on_pushes(job: &Value) -> bool {
    job["if"]
        .as_str()
        .is_some_and(|condition| condition.contains("github.event_name == 'push'"))
}

#[gsttt_scenario::scenario("distribution", "Every check runs in CI")]
#[test]
fn every_check_runs_in_ci() {
    let workflow = workflow();

    assert!(
        workflow["on"].get("pull_request").is_some(),
        "the workflow runs on pull requests"
    );
    assert_eq!(
        workflow["on"]["push"]["branches"],
        serde_json::json!(["main"])
    );

    let checks = &workflow["jobs"]["checks"];
    assert!(
        checks["runs-on"].as_str().unwrap().starts_with("ubuntu-"),
        "the checks run on Linux"
    );
    let commands = commands(checks);
    for check in [
        "cargo fmt --all --check",
        "cargo clippy --locked --workspace --all-targets -- -D warnings",
        "cargo nextest run --locked --workspace",
        "cargo xtask spec",
        "cargo xtask check",
        "cargo deny check",
        "typos",
    ] {
        assert!(commands.contains(&check), "no step runs `{check}`");
    }
    // The toolchain is the one of rust-toolchain.toml, not a version named in the workflow.
    assert!(commands.contains(&"rustup toolchain install"));

    let openspec = commands
        .iter()
        .find_map(|command| command.strip_prefix("pnpm dlx @fission-ai/openspec@"))
        .expect("no step runs OpenSpec through `pnpm dlx`");
    let (version, arguments) = openspec.split_once(' ').unwrap();
    assert!(
        version.split('.').count() == 3
            && version
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())),
        "the OpenSpec CLI is pinned to a version, not `{version}`"
    );
    assert!(
        arguments.starts_with("validate --all --strict"),
        "{arguments}"
    );
}

#[gsttt_scenario::scenario("distribution", "Releases follow green pushes to main")]
#[test]
fn releases_follow_green_pushes_to_main() {
    let workflow = workflow();
    let jobs = jobs(&workflow);
    let release_jobs: Vec<(&String, &Value)> =
        jobs.iter().filter(|(name, _)| *name != "checks").collect();
    assert!(!release_jobs.is_empty(), "the workflow has release jobs");

    for (name, job) in &release_jobs {
        assert!(needs_checks(jobs, job), "{name} needs the checks");
        let condition = job["if"].as_str().unwrap_or_default();
        for part in [
            "github.event_name == 'push'",
            "github.ref == 'refs/heads/main'",
            "github.actor != 'github-actions[bot]'",
        ] {
            assert!(condition.contains(part), "{name} runs only if {part}");
        }
    }

    let targets: BTreeSet<&str> = release_jobs
        .iter()
        .flat_map(|(_, job)| matrix(job))
        .filter_map(|entry| entry["target"].as_str())
        .collect();
    assert_eq!(targets, BTreeSet::from(RELEASE_TARGETS));
    for entry in release_jobs.iter().flat_map(|(_, job)| matrix(job)) {
        let target = entry["target"].as_str().unwrap();
        let os = entry["os"].as_str().unwrap();
        assert_eq!(
            target.ends_with("-apple-darwin"),
            os.starts_with("macos-"),
            "{target} is built on {os}"
        );
    }

    for (name, job) in jobs {
        if !only_on_pushes(job) {
            assert!(
                runners(job).iter().all(|os| !os.starts_with("macos")),
                "{name} runs on pull requests and must not use macOS"
            );
        }
    }
}

#[gsttt_scenario::scenario("distribution", "The release build is given its version")]
#[test]
fn the_release_build_is_given_its_version() {
    let workflow = workflow();
    let steps = workflow["jobs"]["build"]["steps"].as_array().unwrap();

    let version = steps
        .iter()
        .find(|step| step["id"] == "version")
        .expect("a step with the id `version`");
    assert!(
        version["run"]
            .as_str()
            .unwrap()
            .contains("$(cargo xtask version)")
    );
    let build = steps
        .iter()
        .find(|step| {
            step["run"]
                .as_str()
                .is_some_and(|run| run.contains("--profile dist"))
        })
        .expect("a step that builds with the dist profile");
    assert_eq!(
        build["env"]["GSTTT_VERSION"],
        "${{ steps.version.outputs.version }}"
    );
}

#[gsttt_scenario::scenario("distribution", "Cargo.lock is used as it is")]
#[test]
fn the_lockfile_is_used_as_it_is() {
    let workflow = workflow();
    for (job, command) in [
        ("checks", "cargo clippy"),
        ("checks", "cargo nextest"),
        ("build", "--profile dist"),
    ] {
        let line = commands(&workflow["jobs"][job])
            .into_iter()
            .find(|line| line.contains(command))
            .unwrap_or_else(|| panic!("{job} has no `{command}` command"));
        assert!(
            line.split_whitespace().any(|word| word == "--locked"),
            "`{line}` may resolve dependencies the checks never saw: add --locked"
        );
    }
}

#[test]
fn releases_are_made_one_at_a_time() {
    let workflow = workflow();

    assert_eq!(
        workflow["jobs"]["publish"]["concurrency"],
        json!({ "group": "release", "cancel-in-progress": false }),
        "two publish jobs must not run together, and a running one must not be cancelled"
    );
}

#[test]
fn release_jobs_do_not_run_after_a_failure() {
    let workflow = workflow();
    for (name, job) in jobs(&workflow) {
        if name == "checks" {
            continue;
        }
        let condition = job["if"].as_str().unwrap_or_default();
        for function in ["always()", "cancelled()", "failure()"] {
            assert!(
                !condition.contains(function),
                "{name} runs `if: {condition}`: with {function} it would release after a failed check"
            );
        }
    }
}

#[gsttt_scenario::scenario("distribution", "The release token reaches no compiler")]
#[test]
fn the_token_is_given_only_to_the_commands_that_release() {
    let workflow = workflow();
    for (name, job) in jobs(&workflow) {
        assert!(
            !holds_a_token(&json!({ "env": job["env"] })),
            "{name} gives every step a token"
        );
        for step in job["steps"].as_array().unwrap() {
            let run = step["run"].as_str().unwrap_or_default();
            // `cargo` compiles xtask and all of its dependencies, build scripts included, which is code the token must never reach.
            assert!(
                !(holds_a_token(step) && run.contains("cargo")),
                "{name}: a step that runs cargo is given a token: {run}"
            );
        }
    }

    let publish = &workflow["jobs"]["publish"];
    let release = step(publish, "Release").expect("the publish job has a step named `Release`");
    assert!(holds_a_token(release), "the Release step needs the token");
    assert_eq!(
        release["env"]["VERSION"], "${{ steps.version.outputs.version }}",
        "the Release step is given the version, not the means to compute it"
    );
    let version = publish["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["id"] == "version")
        .expect("a step with the id `version`");
    assert!(
        version["run"]
            .as_str()
            .unwrap()
            .contains("$(cargo xtask version)")
    );
    assert!(
        version["run"]
            .as_str()
            .unwrap()
            .contains("cargo xtask notes > notes.md")
    );
}

/// The `run` script of the step of the `publish` job named `name`.
pub(crate) fn publish_script(name: &str) -> String {
    workflow()
        .pointer("/jobs/publish")
        .and_then(|publish| step(publish, name))
        .and_then(|step| step.get("run"))
        .and_then(Value::as_str)
        .unwrap()
        .to_owned()
}

/// Runs the script of the step `name` of the `publish` job in `repo`, as the runner would: with `bash -e`, the programs of `tools` ahead of the others on `PATH`, and `env` set.
pub(crate) fn run_publish_step(
    repo: &Path,
    name: &str,
    tools: &Path,
    env: &[(&str, &str)],
) -> Output {
    let path = format!("{}:{}", tools.display(), std::env::var("PATH").unwrap());
    isolate_git(&mut Command::new("bash"))
        .args(["-e", "-c", &publish_script(name)])
        .current_dir(repo)
        .env("PATH", path)
        .envs(env.iter().copied())
        .output()
        .unwrap()
}

/// What the `Release` step did: how it ended and the arguments of each `gh` command it ran.
struct Release {
    output: Output,
    gh: Vec<String>,
}

/// Runs the `Release` step of the workflow in `repo`, as the runner would for the commit checked out there and the version `version`. `packages/` holds a package, and `gh` only writes its arguments down.
fn run_release(repo: &Path, version: &str) -> Release {
    let tools = tempfile::tempdir().unwrap();
    let log = tools.path().join("gh.log");
    write_executable(
        tools.path(),
        "gh",
        "#!/bin/sh\necho \"$*\" >> \"$GH_LOG\"\n",
    );
    write(repo, "packages/gsttt.tar.gz", "package");
    write(repo, "notes.md", "- a change\n");

    let output = run_publish_step(
        repo,
        "Release",
        tools.path(),
        &[
            ("GH_LOG", log.to_str().unwrap()),
            ("GH_TOKEN", "token"),
            ("VERSION", version),
            ("GITHUB_SHA", &git(repo, &["rev-parse", "HEAD"])),
        ],
    );
    let gh = std::fs::read_to_string(&log)
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default();
    Release { output, gh }
}

#[test]
fn a_new_version_is_released_from_the_commit_that_was_built() {
    let repo = repository("0.1.0");
    let commit = git(repo.path(), &["rev-parse", "HEAD"]);

    let release = run_release(repo.path(), "0.1.5");

    assert!(
        release.output.status.success(),
        "{}",
        printed(&release.output)
    );
    assert_eq!(
        release.gh,
        [format!(
            "release create v0.1.5 packages/gsttt.tar.gz --title v0.1.5 --notes-file notes.md --target {commit} --latest=true"
        )]
    );
}

#[test]
fn a_version_already_released_from_this_commit_is_left_alone() {
    let repo = repository("0.1.0");
    git(repo.path(), &["tag", "v0.1.5"]);

    let release = run_release(repo.path(), "0.1.5");

    assert!(
        release.output.status.success(),
        "{}",
        printed(&release.output)
    );
    assert_eq!(release.gh, Vec::<String>::new());
}

#[gsttt_scenario::scenario("distribution", "A version is released from one commit only")]
#[test]
fn a_version_released_from_another_commit_is_not_overwritten() {
    let repo = repository("0.1.0");
    git(repo.path(), &["tag", "v0.1.5"]);
    let released = git(repo.path(), &["rev-parse", "v0.1.5"]);
    commit(repo.path(), "feat: later");

    let release = run_release(repo.path(), "0.1.5");

    assert!(
        !release.output.status.success(),
        "{}",
        printed(&release.output)
    );
    let message = printed(&release.output);
    assert!(
        message.contains(&format!("::error::v0.1.5 is released from {released}")),
        "{message}"
    );
    assert_eq!(release.gh, Vec::<String>::new(), "nothing is uploaded");
}

#[gsttt_scenario::scenario("distribution", "Only the highest version is the latest release")]
#[test]
fn a_version_below_a_release_is_not_the_latest() {
    let repo = repository("0.1.0");
    git(repo.path(), &["tag", "v0.1.9"]);

    let release = run_release(repo.path(), "0.1.5");

    assert!(
        release.output.status.success(),
        "{}",
        printed(&release.output)
    );
    assert!(
        release
            .gh
            .iter()
            .all(|call| call.ends_with("--latest=false")),
        "{:?}",
        release.gh
    );
    assert_eq!(release.gh.len(), 1);
}

#[test]
fn versions_are_compared_as_numbers_to_find_the_latest() {
    let repo = repository("0.1.0");
    git(repo.path(), &["tag", "v0.1.9"]);

    let release = run_release(repo.path(), "0.1.10");

    assert!(
        release.output.status.success(),
        "{}",
        printed(&release.output)
    );
    assert!(
        release
            .gh
            .iter()
            .all(|call| call.ends_with("--latest=true")),
        "{:?}",
        release.gh
    );
    assert_eq!(release.gh.len(), 1);
}

/// The targets whose runner can execute them: an `x86_64` Linux runner and an arm64 macOS runner.
const NATIVE_TARGETS: [&str; 2] = ["x86_64-unknown-linux-musl", "aarch64-apple-darwin"];

#[gsttt_scenario::scenario("cli", "The release build runs the packaged binary")]
#[test]
fn the_release_build_runs_the_packaged_binary() {
    let workflow = workflow();
    let build = &jobs(&workflow)["build"];

    let entries = matrix(build);
    for entry in &entries {
        assert!(
            entry["native"].is_boolean(),
            "{} does not say whether it is native",
            entry["target"]
        );
    }
    let native: BTreeSet<&str> = entries
        .iter()
        .filter(|entry| entry["native"] == true)
        .filter_map(|entry| entry["target"].as_str())
        .collect();
    assert_eq!(native, BTreeSet::from(NATIVE_TARGETS));

    // The package is run once it is packed, and before the upload takes `target/packages` whole.
    let steps = build["steps"].as_array().unwrap();
    let position = |label: &str| {
        steps.iter().position(|step| {
            step["name"] == label
                || step["uses"]
                    .as_str()
                    .is_some_and(|uses| uses.starts_with(label))
        })
    };
    let packed = position("Package").unwrap();
    let run = position("Run the packaged binary").unwrap();
    let upload = position("actions/upload-artifact@").unwrap();
    assert!(
        packed < run && run < upload,
        "the steps Package, Run the packaged binary and the upload are at {packed}, {run} and {upload}"
    );

    let packaged = step(build, "Run the packaged binary").unwrap();
    assert_eq!(packaged["if"], "${{ matrix.native }}");
    assert_eq!(
        packaged["env"]["VERSION"],
        "${{ steps.version.outputs.version }}"
    );
    assert_eq!(packaged["env"]["TARGET"], "${{ matrix.target }}");
    let script = packaged["run"].as_str().unwrap();
    assert!(script.contains("unpacked=\"${RUNNER_TEMP}/"), "{script}");
    assert!(
        script.contains(
            "tar -xzf \"target/packages/gsttt-${VERSION}-${TARGET}.tar.gz\" -C \"$unpacked\""
        ),
        "{script}"
    );
    for arguments in ["--version", "index", "impact"] {
        assert!(
            script.contains(&format!("\"${{unpacked}}/gsttt\" {arguments}")),
            "nothing runs the unpacked binary with `{arguments}`: {script}"
        );
    }
    let file = script
        .lines()
        .find_map(|line| line.strip_prefix("\"${unpacked}/gsttt\" impact "))
        .unwrap();
    assert!(
        workspace_root().join(file).is_file(),
        "`impact` is given {file}, which is no file of the repository"
    );
}

/// What the `Run the packaged binary` step did: how it ended, and a line for each run of the packaged `gsttt` with the directory it ran in and the arguments it was given. The two directories are the temporary directory of the runner, where the package is unpacked, and the checkout.
struct Packaged {
    output: Output,
    runs: Vec<String>,
    runner_temp: tempfile::TempDir,
    checkout: tempfile::TempDir,
}

/// Runs the `Run the packaged binary` step as the runner would, in a checkout of one commit whose package holds a `gsttt` that says it is `says`, with `version` as the version of the `Version` step.
fn run_packaged(version: &str, says: &str) -> Packaged {
    let script = workflow()
        .pointer("/jobs/build")
        .and_then(|build| step(build, "Run the packaged binary"))
        .and_then(|packaged| packaged.get("run"))
        .and_then(Value::as_str)
        .unwrap()
        .to_owned();
    let checkout = repository("0.1.0");
    let target_dir = checkout.path().join("target");
    write_executable(
        &target_dir,
        &format!("{TARGET}/dist/gsttt"),
        &format!(
            "#!/bin/sh\necho \"$(pwd -P): $0 $*\" >> \"$GSTTT_LOG\"\n[ \"$1\" = --version ] && echo \"gsttt {says}\"\nexit 0\n"
        ),
    );
    let packed = package(checkout.path(), &target_dir, TARGET);
    assert!(packed.status.success(), "{}", printed(&packed));

    let runner_temp = tempfile::tempdir().unwrap();
    let log = runner_temp.path().join("runs.log");
    let output = Command::new("bash")
        .args(["-e", "-c", &script])
        .current_dir(checkout.path())
        .env("RUNNER_TEMP", runner_temp.path())
        .env("VERSION", version)
        .env("TARGET", TARGET)
        .env("GSTTT_LOG", &log)
        .output()
        .unwrap();
    let runs = std::fs::read_to_string(&log)
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default();
    Packaged {
        output,
        runs,
        runner_temp,
        checkout,
    }
}

#[test]
fn the_packaged_binary_is_run_unpacked_on_the_checkout() {
    let ran = run_packaged("0.1.1", "0.1.1");

    assert!(ran.output.status.success(), "{}", printed(&ran.output));
    let program = ran.runner_temp.path().join("gsttt-package/gsttt");
    let checkout = ran.checkout.path().canonicalize().unwrap();
    let prefix = format!("{}: {}", checkout.display(), program.display());
    let [version, index, impact] = ran.runs.as_slice() else {
        panic!("the binary was run as {:?}", ran.runs);
    };
    assert_eq!(version, &format!("{prefix} --version"));
    assert_eq!(index, &format!("{prefix} index"));
    assert!(impact.starts_with(&format!("{prefix} impact ")), "{impact}");
}

#[test]
fn a_binary_that_says_another_version_fails_the_step() {
    let ran = run_packaged("0.1.1", "0.1.0-dev");

    assert!(!ran.output.status.success(), "{}", printed(&ran.output));
    assert!(
        printed(&ran.output).contains("gsttt 0.1.0-dev"),
        "{}",
        printed(&ran.output)
    );
    assert_eq!(ran.runs.len(), 1, "only --version is run: {:?}", ran.runs);
}

#[test]
fn a_package_of_another_version_is_not_run() {
    let ran = run_packaged("0.1.2", "0.1.2");

    assert!(!ran.output.status.success(), "{}", printed(&ran.output));
    assert!(
        printed(&ran.output).contains("gsttt-0.1.2-x86_64-unknown-linux-musl.tar.gz"),
        "{}",
        printed(&ran.output)
    );
    assert_eq!(ran.runs, Vec::<String>::new());
}
