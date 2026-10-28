//! The arguments of `cargo xtask agents questions`.

use std::path::PathBuf;

use anyhow::{Context as _, bail, ensure};

use crate::bench::args::set;

/// What `cargo xtask agents questions` was asked.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Args {
    /// The repositories to draw for; every one that asks for questions when empty.
    pub(super) repositories: Vec<String>,
    /// The file to write instead of `bench/agents/questions.json`.
    pub(super) out: Option<PathBuf>,
}

/// Reads `[--repository <name>]... [--out <file>]`.
pub(super) fn parse(args: &[String]) -> anyhow::Result<Args> {
    let mut repositories = Vec::new();
    let mut out = None;
    let mut words = args.iter();
    while let Some(flag) = words.next() {
        let mut value = || {
            words
                .next()
                .with_context(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--repository" => repositories.push(value()?.clone()),
            "--out" => set(&mut out, "--out", PathBuf::from(value()?))?,
            other => bail!("unknown argument `{other}`"),
        }
    }
    ensure!(
        repositories.is_empty() || out.is_some(),
        "--repository draws only some of the questions, so it needs --out: the file the runs read holds all of them"
    );
    Ok(Args { repositories, out })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn no_argument_draws_every_repository_into_the_checked_in_file() {
        let args = parse(&[]).unwrap();
        assert!(args.repositories.is_empty() && args.out.is_none());
    }

    #[test]
    fn repositories_and_the_file_are_read() {
        let args = parse(&words(&[
            "--repository",
            "vite",
            "--out",
            "x.json",
            "--repository",
            "excalidraw",
        ]))
        .unwrap();
        assert_eq!(args.repositories, ["vite", "excalidraw"]);
        assert_eq!(args.out, Some(PathBuf::from("x.json")));
    }

    #[test]
    fn a_part_of_the_questions_never_replaces_the_whole_file() {
        let error = parse(&words(&["--repository", "vite"]))
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
            assert!(parse(&words(args)).is_err(), "{args:?}");
        }
    }
}
