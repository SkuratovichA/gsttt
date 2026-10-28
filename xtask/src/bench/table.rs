//! The table `cargo xtask bench` prints when it is done.

use super::results::Results;

/// One line per repository and one column per tool; a cell holds the cold and the warm median in milliseconds, and `-` where the tool does not read the repository.
pub(super) fn render(results: &Results) -> String {
    let mut tools: Vec<&str> = Vec::new();
    for run in results
        .repositories
        .iter()
        .flat_map(|repository| &repository.tools)
    {
        if !tools.contains(&run.tool.as_str()) {
            tools.push(&run.tool);
        }
    }

    let header: Vec<String> = ["repository", "files"]
        .into_iter()
        .chain(tools.iter().copied())
        .map(str::to_owned)
        .collect();
    let rows: Vec<Vec<String>> = results
        .repositories
        .iter()
        .map(|repository| {
            let cells = tools.iter().map(|name| {
                repository
                    .tools
                    .iter()
                    .find(|run| run.tool == *name)
                    .map_or_else(
                        || "-".to_owned(),
                        |run| format!("{} / {}", run.cold_ms, run.warm_ms),
                    )
            });
            [repository.name.clone(), repository.files.to_string()]
                .into_iter()
                .chain(cells)
                .collect()
        })
        .collect();

    let widths: Vec<usize> = (0..header.len())
        .map(|column| {
            std::iter::once(&header)
                .chain(&rows)
                .filter_map(|row| row.get(column))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let line = |row: &[String]| {
        let cells: Vec<String> = row
            .iter()
            .zip(&widths)
            .map(|(cell, &width)| format!("{cell:<width$}"))
            .collect();
        cells.join("  ").trim_end().to_owned()
    };

    let mut lines = vec!["median milliseconds, cold / warm".to_owned(), line(&header)];
    lines.extend(rows.iter().map(|row| line(row)));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::results::{RepositoryRuns, ToolRun};

    fn run(tool: &str, cold_ms: u64, warm_ms: u64) -> ToolRun {
        ToolRun {
            tool: tool.to_owned(),
            version: "1".to_owned(),
            cold_ms,
            warm_ms,
            command: tool.to_owned(),
        }
    }

    #[test]
    fn a_tool_that_does_not_read_a_repository_has_a_dash() {
        let repository = |name: &str, files, tools| RepositoryRuns {
            name: name.to_owned(),
            url: "https://example.test".to_owned(),
            files,
            tools,
        };
        let results = Results {
            measured_at: "2026-10-28T12:30:00Z".to_owned(),
            machine: "machine".to_owned(),
            repositories: vec![
                repository(
                    "vite",
                    42,
                    vec![run("gsttt", 120, 30), run("madge", 900, 800)],
                ),
                repository("ripgrep", 7, vec![run("gsttt", 10, 5)]),
            ],
        };
        let table = render(&results);
        let lines: Vec<Vec<&str>> = table
            .lines()
            .map(|line| line.split_whitespace().collect())
            .collect();
        assert_eq!(lines[0], ["median", "milliseconds,", "cold", "/", "warm"]);
        assert_eq!(lines[1], ["repository", "files", "gsttt", "madge"]);
        assert_eq!(
            lines[2],
            ["vite", "42", "120", "/", "30", "900", "/", "800"]
        );
        assert_eq!(lines[3], ["ripgrep", "7", "10", "/", "5", "-"]);
    }
}
