//! `cargo xtask agents table`: the mean of the runs of each setup and arm, for each kind of question, as Markdown.

use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::bail;

use super::RESULTS;
use super::questions::Kind;
use super::results::{Outcome, Results, Run};
use super::score::float;
use super::setups::{self, Arm, Setup};
use crate::root::repository_root;

const COLUMNS: [&str; 13] = [
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
    "own tool calls",
    "runs that called them",
    "changed the checkout",
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

/// A table for each kind of question the results hold, with a row for each setup and arm that has runs of it; nothing at all for results without a run.
pub(super) fn render(results: &Results) -> String {
    let kinds: BTreeSet<Kind> = results.runs.iter().map(|run| run.kind).collect();
    if kinds.is_empty() {
        return String::new();
    }
    let tables: Vec<String> = kinds
        .into_iter()
        .map(|kind| {
            let mut lines = vec![
                format!("### {}", kind.name()),
                String::new(),
                row(&COLUMNS),
                row(&COLUMNS.map(|_| "---")),
            ];
            for (setup, arm) in setups::every_arm() {
                let runs: Vec<&Run> = results
                    .runs
                    .iter()
                    .filter(|run| run.kind == kind && run.setup == setup && run.arm == arm)
                    .collect();
                if !runs.is_empty() {
                    lines.push(row(&cells(setup, arm, &runs)));
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

/// The means over the `runs` of one setup in one arm, written the way the table shows them: the scores to two decimals, the tokens and the seconds whole, the turns and the calls of the setup's own tools to one decimal, the cost to three. The own tools are the ones the setup adds (`Setup::adds`), so `plain` has none and shows a dash for them.
fn cells(setup: Setup, arm: Arm, runs: &[&Run]) -> Vec<String> {
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
    let (own_calls, called_them) = if setup == Setup::Plain {
        ("-".to_owned(), "-".to_owned())
    } else {
        let calls: Vec<u32> = runs.iter().map(|run| own_calls(run, setup)).collect();
        let floats: Vec<f64> = calls.iter().map(|&calls| float(calls)).collect();
        (
            format!("{:.1}", mean(&floats)),
            calls.iter().filter(|&&calls| calls > 0).count().to_string(),
        )
    };
    let changed = runs.iter().filter(|run| run.changed_checkout).count();
    vec![
        label(setup, arm),
        format!("{:.2}", mean_of(|run| run.recall)),
        format!("{:.2}", mean_of(|run| run.precision)),
        format!("{:.2}", mean_of(|run| run.f1)),
        format!("{:.0}", mean_of(|run| float(run.tokens()))),
        format!("{:.1}", mean_of(|run| float(run.turns))),
        format!("{:.0}", mean_of(|run| float(run.duration_ms) / 1000.0)),
        cost,
        runs.len().to_string(),
        unanswered.to_string(),
        own_calls,
        called_them,
        changed.to_string(),
    ]
}

/// The name of a row: the setup, and the arm too when the model was told about its tools.
fn label(setup: Setup, arm: Arm) -> String {
    match arm {
        Arm::Installed => setup.name().to_owned(),
        Arm::Told => format!("{}, {arm}", setup.name()),
    }
}

/// How many times `run` called the tools `setup` adds.
fn own_calls(run: &Run, setup: Setup) -> u32 {
    run.tool_calls
        .iter()
        .filter(|(name, _)| setup.adds(name))
        .fold(0, |sum, (_, &calls)| sum.saturating_add(calls))
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

    /// The cells of every row of the table of `kind`, the header and its rule first.
    fn rows_of(table: &str, kind: &str) -> Vec<Vec<String>> {
        table
            .split("### ")
            .find(|section| section.starts_with(kind))
            .into_iter()
            .flat_map(str::lines)
            .filter(|line| line.starts_with('|'))
            .map(|line| {
                line.trim_matches('|')
                    .split('|')
                    .map(|cell| cell.trim().to_owned())
                    .collect()
            })
            .collect()
    }

    /// The cells of the row named `label` in the table of `kind`, or `None` when it has none.
    fn cells_of(table: &str, kind: &str, label: &str) -> Option<Vec<String>> {
        rows_of(table, kind)
            .into_iter()
            .find(|cells| cells.first().map(String::as_str) == Some(label))
    }

    /// A run of the first question in `setup` and `arm` that called the tools named, so many times each.
    fn calling(setup: Setup, arm: Arm, calls: &[(&str, u32)]) -> Run {
        let mut run = sample("vite-users-1", Kind::Users, setup, 1);
        run.arm = arm;
        run.tool_calls = calls
            .iter()
            .map(|&(name, count)| (name.to_owned(), count))
            .collect();
        run
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
        assert_eq!(row[8], "2", "{table}");
        assert_eq!(row[9], "0", "{table}");
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
        assert!(table.contains("| setup | recall | precision | F1 | tokens | turns | seconds | cost (USD) | runs | without an answer | own tool calls | runs that called them | changed the checkout |"));
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
        first.changed_checkout = true;
        let mut second = sample("vite-users-1", Kind::Users, Setup::Lsp, 2);
        second.cost_usd = Some(0.5);
        second.turns = 4;
        second.duration_ms = 4000;
        results.runs.extend([first, second]);

        let table = render(&results);
        let row = cells_of(&table, "users", "lsp").unwrap();
        // The mean cost is over the run that reported one, the tokens are the four kinds together, and the runs that called no tool of their own still count in the mean of the calls.
        assert_eq!(
            row,
            [
                "lsp", "0.50", "1.00", "0.67", "118", "3.5", "3", "0.500", "2", "1", "0.0", "0",
                "1"
            ]
        );
    }

    #[test]
    fn the_questions_about_clients_have_a_table_after_those_about_tests() {
        let mut results = started();
        for (question, kind) in [
            ("hoppscotch-clients-1", Kind::Clients),
            ("vite-tests-1", Kind::Tests),
            ("vite-users-1", Kind::Users),
        ] {
            results.runs.push(sample(question, kind, Setup::Plain, 1));
        }
        let table = render(&results);
        let places: Vec<Option<usize>> = ["### users", "### tests", "### clients"]
            .iter()
            .map(|title| table.find(title))
            .collect();
        assert!(places.iter().all(Option::is_some), "{table}");
        assert!(places.is_sorted(), "{table}");
        assert!(cells_of(&table, "clients", "plain").is_some(), "{table}");
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "Results without a run print no table")]
    #[test]
    fn results_without_a_run_render_to_nothing_and_not_to_an_empty_line() {
        assert_eq!(render(&started()), "");
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

    #[gsttt_scenario::scenario("agent-benchmarks", "The table has a row for each setup and arm")]
    #[test]
    fn a_row_is_made_for_each_setup_and_arm_that_has_runs() {
        let mut results = started();
        // The runs are pushed out of the order of the table, and the gsttt setup was only asked told.
        for (setup, arm) in [
            (Setup::Gsttt, Arm::Told),
            (Setup::Lsp, Arm::Told),
            (Setup::Lsp, Arm::Installed),
            (Setup::Plain, Arm::Installed),
        ] {
            results.runs.push(calling(setup, arm, &[]));
        }
        let table = render(&results);
        let labels: Vec<String> = rows_of(&table, "users")
            .into_iter()
            .skip(2)
            .map(|cells| cells[0].clone())
            .collect();
        assert_eq!(
            labels,
            ["plain", "lsp", "lsp, told", "gsttt, told"],
            "{table}"
        );
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "The table counts the calls of the tools a setup adds"
    )]
    #[test]
    fn only_the_tools_of_a_setup_count_as_its_own_calls() {
        let gsttt = "mcp__plugin_gsttt_gsttt__impact";
        let cbm = "mcp__codebase-memory-mcp__search_graph";
        let mut results = started();
        results.runs.extend([
            // Four calls of the built-in tool, whatever else the run read and searched for.
            calling(
                Setup::Lsp,
                Arm::Installed,
                &[("LSP", 4), ("Read", 6), ("ToolSearch", 1)],
            ),
            // A name that only begins with `LSP` is no call of the tool.
            calling(Setup::Lsp, Arm::Installed, &[("LSPx", 1), ("Read", 1)]),
            // The `LSP` call of a run in the gsttt setup is not a call of a tool of gsttt, nor is a call of a tool of another setup, or of a server whose name begins like that of gsttt.
            calling(
                Setup::Gsttt,
                Arm::Told,
                &[
                    (gsttt, 2),
                    ("LSP", 1),
                    (cbm, 1),
                    ("mcp__plugin_gsttt_gsttt2__impact", 1),
                    ("Read", 3),
                    ("ToolSearch", 1),
                ],
            ),
            calling(
                Setup::Cbm,
                Arm::Installed,
                &[(cbm, 3), (gsttt, 1), ("LSP", 1)],
            ),
            calling(Setup::Plain, Arm::Installed, &[("LSP", 9), ("Read", 2)]),
        ]);
        let mut changed = calling(Setup::Gsttt, Arm::Told, &[("Read", 1)]);
        changed.changed_checkout = true;
        results.runs.push(changed);

        let table = render(&results);
        let last_three = |label: &str| {
            let cells = cells_of(&table, "users", label).unwrap();
            cells[cells.len() - 3..].to_vec()
        };
        // The mean over the two runs of the setup, and the runs that called one of its tools.
        assert_eq!(last_three("lsp"), ["2.0", "1", "0"], "{table}");
        assert_eq!(last_three("gsttt, told"), ["1.0", "1", "1"], "{table}");
        assert_eq!(last_three("cbm"), ["3.0", "1", "0"], "{table}");
        // A setup with no tools of its own has nothing to count.
        assert_eq!(last_three("plain"), ["-", "-", "0"], "{table}");
    }
}
