//! The `ci` workflow, read as data.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::support::workspace_root;

const RELEASE_TARGETS: [&str; 4] = [
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
];

fn workflow() -> Value {
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
        "cargo clippy --workspace --all-targets -- -D warnings",
        "cargo nextest run --workspace",
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
