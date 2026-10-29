//! The arguments of `cargo xtask agents questions` and `cargo xtask agents run`.

use std::path::PathBuf;

use anyhow::{Context as _, bail, ensure};

use super::setups::Setup;
use crate::bench::args::set;

/// The task the arguments are for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Task {
    Questions,
    Run,
}

/// What `cargo xtask agents questions` or `cargo xtask agents run` was asked.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Args {
    /// The repositories to draw or run for; every one that asks for questions when empty.
    pub(super) repositories: Vec<String>,
    /// The file to write instead of the checked-in one: the questions for `questions`, the results for `run`.
    pub(super) out: Option<PathBuf>,
    /// The binary under test, as an absolute path. Only `run` takes it, and it must.
    pub(super) gsttt: Option<PathBuf>,
    /// The questions to ask, by id; every question of the repositories when empty. Only `run` takes it.
    pub(super) questions: Vec<String>,
    /// How many times each question is asked, at most the number the pins give. Only `run` takes it.
    pub(super) repeats: Option<u32>,
    /// The setups to ask, as named; every setup when empty. Only `run` takes it.
    pub(super) setups: Vec<Setup>,
}

/// Reads `[--repository <name>]... [--out <file>]` for `questions`, and for `run` also `--gsttt <binary>` and `[--question <id>]... [--repeats <n>] [--setup <name>]...`.
pub(super) fn parse(task: Task, args: &[String]) -> anyhow::Result<Args> {
    let mut parsed = Args {
        repositories: Vec::new(),
        out: None,
        gsttt: None,
        questions: Vec::new(),
        repeats: None,
        setups: Vec::new(),
    };
    let mut words = args.iter();
    while let Some(flag) = words.next() {
        let mut value = || {
            words
                .next()
                .with_context(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--repository" => parsed.repositories.push(value()?.clone()),
            "--out" => set(&mut parsed.out, "--out", PathBuf::from(value()?))?,
            "--gsttt" if task == Task::Run => {
                let binary = std::path::absolute(value()?).context("cannot resolve --gsttt")?;
                set(&mut parsed.gsttt, "--gsttt", binary)?;
            }
            "--question" if task == Task::Run => parsed.questions.push(value()?.clone()),
            "--repeats" if task == Task::Run => {
                let repeats: u32 = value()?.parse().context("--repeats needs a number")?;
                ensure!(repeats >= 1, "--repeats needs at least 1");
                set(&mut parsed.repeats, "--repeats", repeats)?;
            }
            "--setup" if task == Task::Run => parsed.setups.push(setup_named(value()?)?),
            "--gsttt" | "--question" | "--repeats" | "--setup" => {
                bail!("{flag} is an argument of `agents run`, not of `agents questions`")
            }
            other => bail!("unknown argument `{other}`"),
        }
    }
    match task {
        Task::Questions => ensure!(
            parsed.repositories.is_empty() || parsed.out.is_some(),
            "--repository draws only some of the questions, so it needs --out: the file the runs read holds all of them"
        ),
        Task::Run => ensure!(
            parsed.gsttt.is_some(),
            "--gsttt <binary> is needed: the binary under test"
        ),
    }
    Ok(parsed)
}

/// The setup that `name` spells, or an error that lists the names there are.
fn setup_named(name: &str) -> anyhow::Result<Setup> {
    Setup::ALL
        .into_iter()
        .find(|setup| setup.name() == name)
        .with_context(|| {
            let names: Vec<&str> = Setup::ALL.into_iter().map(Setup::name).collect();
            format!("no setup is called `{name}`; they are {}", names.join(", "))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn no_argument_draws_every_repository_into_the_checked_in_file() {
        let args = parse(Task::Questions, &[]).unwrap();
        assert!(args.repositories.is_empty() && args.out.is_none());
    }

    #[test]
    fn repositories_and_the_file_are_read() {
        let args = parse(
            Task::Questions,
            &words(&[
                "--repository",
                "vite",
                "--out",
                "x.json",
                "--repository",
                "excalidraw",
            ]),
        )
        .unwrap();
        assert_eq!(args.repositories, ["vite", "excalidraw"]);
        assert_eq!(args.out, Some(PathBuf::from("x.json")));
    }

    #[test]
    fn a_part_of_the_questions_never_replaces_the_whole_file() {
        let error = parse(Task::Questions, &words(&["--repository", "vite"]))
            .unwrap_err()
            .to_string();
        assert!(error.contains("needs --out"), "{error}");
    }

    #[test]
    fn a_flag_without_a_value_or_given_twice_or_unknown_is_refused() {
        for args in [
            &["--out"][..],
            &["--out", "a", "--out", "b"],
            &["--repository"],
            &["--seed", "1"],
        ] {
            assert!(parse(Task::Questions, &words(args)).is_err(), "{args:?}");
        }
    }

    #[test]
    fn a_run_needs_the_binary_under_test_and_reads_its_flags() {
        let error = parse(Task::Run, &[]).unwrap_err().to_string();
        assert!(error.contains("--gsttt"), "{error}");

        let args = parse(
            Task::Run,
            &words(&[
                "--gsttt",
                "target/release/gsttt",
                "--repository",
                "vite",
                "--question",
                "vite-users-1",
                "--question",
                "vite-tests-2",
                "--repeats",
                "1",
                "--out",
                "smoke.json",
            ]),
        )
        .unwrap();
        let gsttt = args.gsttt.unwrap();
        assert!(gsttt.is_absolute() && gsttt.ends_with("target/release/gsttt"));
        assert_eq!(args.repositories, ["vite"]);
        assert_eq!(args.questions, ["vite-users-1", "vite-tests-2"]);
        assert_eq!(args.repeats, Some(1));
        assert_eq!(args.out, Some(PathBuf::from("smoke.json")));
    }

    #[test]
    fn the_flags_of_a_run_are_refused_by_the_draw_and_the_repeats_must_count() {
        for args in [
            &["--gsttt", "gsttt"][..],
            &["--question", "vite-users-1"],
            &["--repeats", "1"],
            &["--setup", "plain"],
        ] {
            let error = parse(Task::Questions, &words(args))
                .unwrap_err()
                .to_string();
            assert!(error.contains("agents run"), "{error}");
        }
        for repeats in ["0", "two", "-1"] {
            let args = words(&["--gsttt", "gsttt", "--repeats", repeats]);
            assert!(parse(Task::Run, &args).is_err(), "{repeats}");
        }
        assert!(parse(Task::Run, &words(&["--gsttt", "a", "--gsttt", "b"])).is_err());
    }

    #[test]
    fn setups_are_read_by_name_and_an_unknown_name_lists_the_known_ones() {
        let args = parse(
            Task::Run,
            &words(&["--gsttt", "g", "--setup", "gsttt", "--setup", "plain"]),
        )
        .unwrap();
        assert_eq!(args.setups, [Setup::Gsttt, Setup::Plain]);
        let none = parse(Task::Run, &words(&["--gsttt", "g"])).unwrap();
        assert_eq!(none.setups, []);

        let error = parse(Task::Run, &words(&["--gsttt", "g", "--setup", "vim"]))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("`vim`") && error.contains("plain, lsp, cbm, gsttt"),
            "{error}"
        );
    }
}
