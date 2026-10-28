//! `cargo xtask bench`: how long gsttt and the tools people use today take to build the import graph of whole repositories, cold and warm. Specified in `openspec/changes/add-benchmarks/`.
//!
//! The repositories and the tools are pinned in `bench/bench.toml`. The task clones the repositories into `bench/repos`, installs the tools into `bench/tools`, times every tool with hyperfine and writes the medians to `bench/results/benchmarks.json`, in the shape the site decodes.

mod args;
mod hyperfine;
mod machine;
mod pins;
mod plan;
mod results;
mod setup;
mod table;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context as _, ensure};

use self::pins::{Pins, Repository, Tool};
use self::plan::{Programs, Run};
use self::results::{RepositoryRuns, Results, ToolRun};
use crate::root::repository_root;

/// What every measurement of one run of the task shares.
#[derive(Debug)]
struct Session<'a> {
    programs: &'a Programs,
    /// What each tool says its version is.
    versions: BTreeMap<Tool, String>,
    /// The directory the repositories are cloned into.
    repos: PathBuf,
    /// Where hyperfine writes its numbers before they are read.
    export: PathBuf,
    runs: u32,
}

/// Measures the pinned repositories, or the ones named by `--repository`, and writes the results.
pub(crate) fn run(args: &[String]) -> anyhow::Result<ExitCode> {
    let args = args::parse(args)?;
    ensure!(
        args.gsttt.is_file(),
        "--gsttt {} is not a file",
        args.gsttt.display()
    );
    let root = repository_root()?;
    let pins = Pins::load(&root)?;
    let repositories = select(&pins, &args.repositories)?;

    // What the run needs from the machine is asked before the first clone, so that a missing program fails at once and not after an hour.
    hyperfine::require()?;
    let machine = machine::describe()?;
    let measured_at = machine::now_utc()?;

    let bench = root.join("bench");
    let programs = Programs::new(&bench.join("tools"), &args.gsttt);
    let needed: BTreeSet<Tool> = repositories
        .iter()
        .flat_map(|repository| repository.language.tools())
        .copied()
        .collect();
    setup::install(&programs, &pins, &needed)?;
    let versions = check_versions(&pins, &programs, &needed)?;

    let export = bench.join("results").join("hyperfine.json.tmp");
    if let Some(directory) = export.parent() {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("cannot create {}", directory.display()))?;
    }
    let session = Session {
        programs: &programs,
        versions,
        repos: bench.join("repos"),
        export,
        runs: args.runs,
    };
    let mut measured = Vec::new();
    for repository in repositories {
        measured.push(measure(&session, repository)?);
    }

    let results = Results {
        measured_at,
        machine,
        repositories: measured,
    };
    let out = args
        .out
        .unwrap_or_else(|| bench.join("results").join("benchmarks.json"));
    let home = std::env::var("HOME").unwrap_or_default();
    results::write(&out, &results, &[&root.to_string_lossy(), &home])?;
    writeln!(std::io::stdout(), "{}", table::render(&results))?;
    Ok(ExitCode::SUCCESS)
}

/// The pinned repositories to measure, in the order of the pins: the ones named, or all of them when none is named.
fn select<'a>(pins: &'a Pins, names: &[String]) -> anyhow::Result<Vec<&'a Repository>> {
    for name in names {
        ensure!(
            pins.repository
                .iter()
                .any(|repository| &repository.name == name),
            "no repository is called {name}; bench/bench.toml pins {}",
            pins.repository
                .iter()
                .map(|repository| repository.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(pins
        .repository
        .iter()
        .filter(|repository| names.is_empty() || names.contains(&repository.name))
        .collect())
}

/// Asks every tool for its version and fails when a pinned tool is not at its pin.
fn check_versions(
    pins: &Pins,
    programs: &Programs,
    needed: &BTreeSet<Tool>,
) -> anyhow::Result<BTreeMap<Tool, String>> {
    let mut versions = BTreeMap::new();
    for &tool in needed {
        let installed = setup::installed_version(tool, programs)?;
        if let Some(pinned) = pins.tools.version(tool) {
            ensure!(
                installed == pinned,
                "{} is installed at {installed}, but bench/bench.toml pins {pinned}",
                tool.name()
            );
        }
        versions.insert(tool, installed);
    }
    Ok(versions)
}

/// Clones one repository and times every tool of its language on it.
fn measure(session: &Session, repository: &Repository) -> anyhow::Result<RepositoryRuns> {
    let dir = session.repos.join(&repository.name);
    setup::checkout(&dir, repository)?;
    let files = setup::count_files(&dir, repository)?;
    let runs = plan::plan(repository, session.programs);

    // A cache that an earlier run left behind would make a cold run warm.
    for run in &runs {
        hyperfine::reset(&dir, run)?;
    }
    let timed = runs
        .iter()
        .map(|run| time_run(session, &dir, repository, run))
        .collect::<anyhow::Result<Vec<_>>>();
    // Whatever happened, no cache and no codebase-memory-mcp daemon is left behind.
    let cleaned = runs.iter().try_for_each(|run| hyperfine::reset(&dir, run));
    let tools = timed?;
    cleaned?;
    Ok(RepositoryRuns {
        name: repository.name.clone(),
        url: repository.url.clone(),
        files,
        tools,
    })
}

/// Times one tool cold and then warm, and throws its cache away so that the next tool does not read it.
fn time_run(
    session: &Session,
    dir: &Path,
    repository: &Repository,
    run: &Run,
) -> anyhow::Result<ToolRun> {
    let command = run.command.shown();
    let describe = || {
        format!(
            "{} on {} with `{command}`",
            run.tool.name(),
            repository.name
        )
    };
    let version = session.versions.get(&run.tool).with_context(describe)?;
    let cold_ms = hyperfine::time(dir, run, &run.cold, session.runs, &session.export)
        .with_context(describe)?;
    let warm_ms = hyperfine::time(dir, run, &run.warm, session.runs, &session.export)
        .with_context(describe)?;
    hyperfine::reset(dir, run).with_context(describe)?;
    writeln!(
        std::io::stderr(),
        "{}: {}: cold {cold_ms} ms, warm {warm_ms} ms",
        repository.name,
        run.tool.name()
    )?;
    Ok(ToolRun {
        tool: run.tool.name().to_owned(),
        version: version.clone(),
        cold_ms,
        warm_ms,
        command,
    })
}
