//! `cargo xtask agents table`: the mean of the runs of each setup, for each kind of question, as Markdown.

use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::bail;

use super::RESULTS;
use super::questions::Kind;
use super::results::{Outcome, Results, Run};
use super::score::float;
use super::setups::Setup;
use crate::root::repository_root;

const COLUMNS: [&str; 10] = [
    "setup",
    "recall",
    "precision",
    "F1",
    "tokens",
    "turns",
    "seconds",
    "cost (USD)",
    "runs",
    "without an answer",
];

/// Prints the table of the results in the file named, or in `bench/results/agents.json`.
pub(super) fn run(args: &[String]) -> anyhow::Result<ExitCode> {
    let file = match args {
        [] => repository_root()?.join(RESULTS),
        [file] => PathBuf::from(file),
        _ => bail!("usage: cargo xtask agents table [<results file>]"),
    };
    let results = Results::load(&file)?;
    write!(std::io::stdout().lock(), "{}", render(&results))?;
    Ok(ExitCode::SUCCESS)
}

/// A table for each kind of question the results hold, with a row for each setup that has runs of it.
pub(super) fn render(results: &Results) -> String {
    let kinds: BTreeSet<Kind> = results.runs.iter().map(|run| run.kind).collect();
    let tables: Vec<String> = kinds
        .into_iter()
        .map(|kind| {
            let mut lines = vec![
                format!("### {}", kind.name()),
                String::new(),
                row(&COLUMNS),
                row(&COLUMNS.map(|_| "---")),
            ];
            for setup in Setup::ALL {
                let runs: Vec<&Run> = results
                    .runs
                    .iter()
                    .filter(|run| run.kind == kind && run.setup == setup)
                    .collect();
                if !runs.is_empty() {
                    lines.push(row(&cells(setup, &runs)));
                }
            }
            lines.join("\n")
        })
        .collect();
    let mut text = tables.join("\n\n");
    text.push('\n');
    text
}

fn row<S: AsRef<str>>(cells: &[S]) -> String {
    let cells: Vec<&str> = cells.iter().map(AsRef::as_ref).collect();
    format!("| {} |", cells.join(" | "))
}

/// The means over the `runs` of one setup, written the way the table shows them: the scores to two decimals, the tokens and the seconds whole, the turns to one decimal, the cost to three.
fn cells(setup: Setup, runs: &[&Run]) -> Vec<String> {
    let mean_of =
        |value: fn(&Run) -> f64| mean(&runs.iter().map(|run| value(run)).collect::<Vec<_>>());
    let costs: Vec<f64> = runs.iter().filter_map(|run| run.cost_usd).collect();
    let cost = if costs.is_empty() {
        "-".to_owned()
    } else {
        format!("{:.3}", mean(&costs))
    };
    let unanswered = runs
        .iter()
        .filter(|run| run.outcome != Outcome::Answered)
        .count();
    vec![
        setup.name().to_owned(),
        format!("{:.2}", mean_of(|run| run.recall)),
        format!("{:.2}", mean_of(|run| run.precision)),
        format!("{:.2}", mean_of(|run| run.f1)),
        format!("{:.0}", mean_of(|run| float(run.tokens()))),
        format!("{:.1}", mean_of(|run| float(run.turns))),
        format!("{:.0}", mean_of(|run| float(run.duration_ms) / 1000.0)),
        cost,
        runs.len().to_string(),
        unanswered.to_string(),
    ]
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / float(values.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::results::{sample, started};

    /// The cells of the row of `setup` in the table of `kind`, or `None` when it has none.
    fn cells_of(table: &str, kind: &str, setup: &str) -> Option<Vec<String>> {
        let section = table
            .split("### ")
            .find(|section| section.starts_with(kind))?;
        section
            .lines()
            .map(|line| {
                line.trim_matches('|')
                    .split('|')
                    .map(|cell| cell.trim().to_owned())
                    .collect::<Vec<_>>()
            })
            .find(|cells| cells.first().map(String::as_str) == Some(setup))
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "The table averages the runs of each setup")]
    #[test]
    fn the_table_averages_the_runs_of_each_setup() {
        let mut results = started();
        for (repeat, recall) in [(1, 1.0), (2, 0.5)] {
            let mut run = sample("vite-users-1", Kind::Users, Setup::Plain, repeat);
            run.recall = recall;
            results.runs.push(run);
        }
        let table = render(&results);
        let row = cells_of(&table, "users", "plain").unwrap();
        assert_eq!(row[1], "0.75", "{table}");
        assert_eq!(row[row.len() - 2], "2", "{table}");
        assert_eq!(row[row.len() - 1], "0", "{table}");
    }

    #[test]
    fn each_kind_has_a_table_and_a_setup_without_runs_has_no_row() {
        let mut results = started();
        results
            .runs
            .push(sample("vite-tests-1", Kind::Tests, Setup::Gsttt, 1));
        results
            .runs
            .push(sample("vite-users-1", Kind::Users, Setup::Cbm, 1));
        results
            .runs
            .push(sample("vite-users-1", Kind::Users, Setup::Plain, 1));
        let table = render(&results);

        // Users come before tests, the setups in their order, and the columns are the ones the spec names.
        assert!(table.find("### users") < table.find("### tests"), "{table}");
        assert!(table.contains("| setup | recall | precision | F1 | tokens | turns | seconds | cost (USD) | runs | without an answer |"));
        let users = table.split("### tests").next().unwrap();
        assert!(users.find("| plain |") < users.find("| cbm |"), "{table}");
        assert!(cells_of(&table, "users", "gsttt").is_none(), "{table}");
        assert!(cells_of(&table, "tests", "gsttt").is_some(), "{table}");
        assert!(cells_of(&table, "tests", "plain").is_none(), "{table}");
    }

    #[test]
    fn the_means_are_written_as_the_table_shows_them() {
        let mut results = started();
        let mut first = sample("vite-users-1", Kind::Users, Setup::Lsp, 1);
        first.cost_usd = None;
        first.outcome = Outcome::Timeout;
        first.turns = 3;
        first.duration_ms = 2000;
        let mut second = sample("vite-users-1", Kind::Users, Setup::Lsp, 2);
        second.cost_usd = Some(0.5);
        second.turns = 4;
        second.duration_ms = 4000;
        results.runs.extend([first, second]);

        let table = render(&results);
        let row = cells_of(&table, "users", "lsp").unwrap();
        // The mean cost is over the run that reported one, the tokens are the four kinds together.
        assert_eq!(
            row,
            [
                "lsp", "0.50", "1.00", "0.67", "118", "3.5", "3", "0.500", "2", "1"
            ]
        );
    }

    #[test]
    fn a_setup_whose_runs_report_no_cost_shows_a_dash() {
        let mut results = started();
        let mut run = sample("vite-users-1", Kind::Users, Setup::Plain, 1);
        run.cost_usd = None;
        results.runs.push(run);
        let row = cells_of(&render(&results), "users", "plain").unwrap();
        assert_eq!(row[7], "-");
    }
}
