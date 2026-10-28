//! The arguments of `cargo xtask bench`.

use std::path::PathBuf;

use anyhow::{Context as _, bail, ensure};

/// What `cargo xtask bench` accepts.
pub(super) const USAGE: &str = "usage: cargo xtask bench --gsttt <binary> [--runs <n>] [--repository <name>]... [--out <file>]";

/// How many timed runs each measurement makes unless `--runs` says otherwise.
const DEFAULT_RUNS: u32 = 5;

/// What `cargo xtask bench` was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Args {
    /// The gsttt binary to time, as an absolute path: the tools run in the checkouts, not in the current directory.
    pub(super) gsttt: PathBuf,
    /// How many timed runs each measurement makes.
    pub(super) runs: u32,
    /// The repositories to measure; all of the pinned ones when empty.
    pub(super) repositories: Vec<String>,
    /// Where to write the results instead of `bench/results/benchmarks.json`.
    pub(super) out: Option<PathBuf>,
}

/// Reads the arguments that follow `bench`.
pub(super) fn parse(args: &[String]) -> anyhow::Result<Args> {
    let mut gsttt = None;
    let mut runs = None;
    let mut out = None;
    let mut repositories = Vec::new();
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        let Some(value) = args.next() else {
            bail!("{flag} needs a value\n{USAGE}")
        };
        match flag.as_str() {
            "--gsttt" => set(
                &mut gsttt,
                flag,
                std::path::absolute(value).with_context(|| format!("cannot resolve {value}"))?,
            )?,
            "--runs" => set(
                &mut runs,
                flag,
                value
                    .parse::<u32>()
                    .with_context(|| format!("--runs wants a number, not {value}"))?,
            )?,
            "--out" => set(
                &mut out,
                flag,
                std::path::absolute(value).with_context(|| format!("cannot resolve {value}"))?,
            )?,
            "--repository" => repositories.push(value.clone()),
            _ => bail!("{flag} is not an argument of bench\n{USAGE}"),
        }
    }
    let Some(gsttt) = gsttt else {
        bail!("--gsttt is required\n{USAGE}")
    };
    let runs = runs.unwrap_or(DEFAULT_RUNS);
    ensure!(runs >= 1, "--runs must be at least 1");
    Ok(Args {
        gsttt,
        runs,
        repositories,
        out,
    })
}

/// Stores the value of a flag that may be given once.
fn set<T>(slot: &mut Option<T>, flag: &str, value: T) -> anyhow::Result<()> {
    ensure!(slot.is_none(), "{flag} is given twice");
    *slot = Some(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn reads_every_flag() {
        let args = parse(&words(&[
            "--gsttt",
            "gsttt",
            "--runs",
            "3",
            "--repository",
            "vite",
            "--repository",
            "django",
            "--out",
            "r.json",
        ]))
        .unwrap();
        assert_eq!(args.runs, 3);
        assert_eq!(args.repositories, ["vite", "django"]);
        assert!(args.gsttt.is_absolute());
        assert!(args.out.is_some_and(|out| out.is_absolute()));
    }

    #[test]
    fn runs_default_to_five() {
        let args = parse(&words(&["--gsttt", "gsttt"])).unwrap();
        assert_eq!((args.runs, args.out), (5, None));
        assert_eq!(args.repositories, Vec::<String>::new());
    }

    #[test]
    fn refuses_what_is_missing_unknown_or_repeated() {
        for args in [
            &["--runs", "3"][..],
            &["--gsttt", "gsttt", "--runs", "0"],
            &["--gsttt", "gsttt", "--runs", "many"],
            &["--gsttt", "gsttt", "--gsttt", "other"],
            &["--gsttt", "gsttt", "--verbose", "yes"],
            &["--gsttt"],
        ] {
            assert!(parse(&words(args)).is_err(), "{args:?}");
        }
    }
}
