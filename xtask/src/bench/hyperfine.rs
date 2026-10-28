//! hyperfine, the timer: it runs a command many times and reports the median.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context as _, bail, ensure};
use serde::Deserialize;
use xshell::{Cmd, Shell};

use super::plan::{Measure, Run};

/// How many lines of an output an error quotes, counted from the end.
const TAIL_LINES: usize = 15;

/// How many characters of a line an error quotes.
const TAIL_WIDTH: usize = 300;

/// Fails, saying how to install hyperfine, when it is not on the path.
pub(super) fn require() -> anyhow::Result<()> {
    let sh = Shell::new()?;
    sh.cmd("hyperfine")
        .arg("--version")
        .quiet()
        .read()
        .context(
            "hyperfine is not installed: run `brew install hyperfine` or `cargo install hyperfine`",
        )?;
    Ok(())
}

/// Times `run` in `dir` as `measure` says and returns the median in whole milliseconds. hyperfine writes its numbers to `export`, which is removed again.
pub(super) fn time(
    dir: &Path,
    run: &Run,
    measure: &Measure,
    runs: u32,
    export: &Path,
) -> anyhow::Result<u64> {
    let sh = Shell::new()?;
    sh.change_dir(dir);
    let mut cmd = sh
        .cmd("hyperfine")
        .args(["--style", "none", "-N", "--runs"])
        .arg(runs.to_string())
        .arg("--export-json")
        .arg(export);
    if measure.warmup > 0 {
        cmd = cmd.arg("--warmup").arg(measure.warmup.to_string());
    }
    if let Some(prepare) = &measure.prepare {
        cmd = cmd.arg("--prepare").arg(prepare);
    }
    let output = environment(cmd.arg(run.command.executed()), run)
        .quiet()
        .ignore_status()
        .output()?;
    if !output.status.success() {
        bail!(
            "hyperfine ended with {}\n{}\n{}\n{}",
            output.status,
            tail(&output.stdout),
            tail(&output.stderr),
            explain(dir, run)
        );
    }
    let text = std::fs::read_to_string(export)
        .with_context(|| format!("hyperfine wrote no {}", export.display()))?;
    std::fs::remove_file(export).with_context(|| format!("cannot remove {}", export.display()))?;
    median_ms(&text)
}

/// Throws the cache of the tool away in `dir`, so that its next run starts cold. The runs of other tools never find the output of this one.
pub(super) fn reset(dir: &Path, run: &Run) -> anyhow::Result<()> {
    let Some(prepare) = &run.cold.prepare else {
        return Ok(());
    };
    let sh = Shell::new()?;
    sh.change_dir(dir);
    environment(sh.cmd("sh").arg("-c").arg(prepare), run)
        .quiet()
        .run()
        .with_context(|| format!("cannot throw away the cache of {}", run.tool.name()))
}

/// The median wall-clock time of the one command in what hyperfine exported, in whole milliseconds. The export is read as hyperfine 2 writes it (`"schema_version": 2`), which keeps the medians under `results[].summary.<metric>`, where hyperfine 1 had `results[].median`; CPU time is a metric of its own and is not what is published.
fn median_ms(export: &str) -> anyhow::Result<u64> {
    #[derive(Deserialize)]
    struct Export {
        results: Vec<Timed>,
    }
    #[derive(Deserialize)]
    struct Timed {
        summary: Summary,
    }
    #[derive(Deserialize)]
    struct Summary {
        time_wall_clock: Metric,
    }
    #[derive(Deserialize)]
    struct Metric {
        unit: String,
        median: f64,
    }

    let export: Export = serde_json::from_str(export)
        .context("hyperfine's export is not the JSON of hyperfine 2 (results[].summary.time_wall_clock.median)")?;
    let [timed] = export.results.as_slice() else {
        bail!("hyperfine timed {} commands, not one", export.results.len())
    };
    let wall_clock = &timed.summary.time_wall_clock;
    ensure!(
        wall_clock.unit == "second",
        "hyperfine reports the wall-clock time in {}, not in seconds",
        wall_clock.unit
    );
    let median = Duration::try_from_secs_f64(wall_clock.median)
        .context("hyperfine's median is not a duration")?;
    // Rounded to the nearest millisecond, never cut.
    Ok(u64::try_from((median.as_micros() + 500) / 1000)?)
}

/// Gives `cmd` the variables the tool runs with.
fn environment<'a>(mut cmd: Cmd<'a>, run: &Run) -> Cmd<'a> {
    for (key, value) in &run.command.env {
        cmd = cmd.env(key, value);
    }
    for (key, value) in &run.isolation {
        cmd = cmd.env(key, value);
    }
    cmd
}

/// What the tool says when it is run once by itself in `dir`: hyperfine hides the output of the commands it times.
fn explain(dir: &Path, run: &Run) -> String {
    let sh = match Shell::new() {
        Ok(sh) => sh,
        Err(error) => return error.to_string(),
    };
    sh.change_dir(dir);
    let cmd = environment(sh.cmd(&run.command.program).args(&run.command.args), run)
        .quiet()
        .ignore_status();
    match cmd.output() {
        Ok(output) => format!(
            "run by itself, `{}` ended with {}\n{}\n{}",
            run.command.shown(),
            output.status,
            tail(&output.stdout),
            tail(&output.stderr)
        ),
        Err(error) => error.to_string(),
    }
}

/// The last lines of an output, each cut short: an error quotes the end of an output, not all of it.
fn tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut lines: Vec<String> = text
        .lines()
        .rev()
        .take(TAIL_LINES)
        .map(|line| line.chars().take(TAIL_WIDTH).collect())
        .collect();
    lines.reverse();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An export in the shape of hyperfine 2 for one command, with the CPU time far from the wall-clock time so that a mix-up shows.
    fn export(wall_clock: f64, unit: &str) -> String {
        serde_json::json!({
            "schema_version": 2,
            "primary_metric": "time_wall_clock",
            "results": [{
                "command": "x",
                "summary": {
                    "time_cpu": {"unit": "second", "count": 3, "mean": 99.0, "stddev": null, "median": 99.0, "min": 99.0, "max": 99.0},
                    "time_wall_clock": {"unit": unit, "count": 3, "mean": 1.0, "stddev": null, "median": wall_clock, "min": 0.0, "max": 2.0},
                },
            }],
        })
        .to_string()
    }

    #[test]
    fn the_median_is_the_wall_clock_time_rounded_to_whole_milliseconds() {
        assert_eq!(median_ms(&export(0.1234, "second")).unwrap(), 123);
        assert_eq!(median_ms(&export(0.0005, "second")).unwrap(), 1);
        assert_eq!(median_ms(&export(12.0, "second")).unwrap(), 12_000);
    }

    #[test]
    fn an_export_that_is_not_one_command_in_seconds_is_refused() {
        assert!(median_ms(&export(-1.0, "second")).is_err());
        assert!(median_ms(&export(1.0, "millisecond")).is_err());
        assert!(median_ms("{\"results\": []}").is_err());
        // The export of hyperfine 1, which has the median on the result itself.
        assert!(median_ms("{\"results\": [{\"command\": \"x\", \"median\": 0.5}]}").is_err());
    }

    #[test]
    fn an_error_quotes_the_end_of_an_output() {
        let output = (1..=40)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let quoted = tail(output.as_bytes());
        assert_eq!(quoted.lines().count(), TAIL_LINES);
        assert!(quoted.ends_with("line 40"));
        assert!(!quoted.contains("line 1\n"));
    }
}
