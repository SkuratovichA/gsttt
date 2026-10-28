//! Errors of the reader: every one names the file and, for a format error, the line.

use std::path::PathBuf;

/// Why an OpenSpec file could not be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The file or directory could not be read.
    #[error("{}: cannot read: {source}", .path.display())]
    Read {
        /// The path that failed, relative to the repository root.
        path: PathBuf,
        /// The underlying failure.
        source: std::io::Error,
    },
    /// The file does not follow the OpenSpec format.
    #[error("{}:{line}: {problem}", .file.display())]
    Format {
        /// The file, relative to the repository root.
        file: PathBuf,
        /// The line of the offending heading or entry, counted from 1.
        line: usize,
        /// What was wrong and what was expected instead.
        problem: Problem,
    },
}

/// What is wrong with a line of an OpenSpec file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Problem {
    /// A `####` heading that is not a scenario heading.
    #[error("expected `#### Scenario: <title>`, found `{found}`")]
    ScenarioHeading {
        /// The heading as written.
        found: String,
    },
    /// A scenario without a `WHEN` or a `THEN` line.
    #[error(
        "scenario `{title}` has no {keyword} line; expected `- **{keyword}** …` under its heading"
    )]
    ScenarioStep {
        /// The scenario title.
        title: String,
        /// The missing keyword, `WHEN` or `THEN`.
        keyword: &'static str,
    },
    /// A scenario heading above the first requirement of its section.
    #[error(
        "scenario `{title}` is not under a requirement; expected `### Requirement: <name>` above it"
    )]
    ScenarioOutsideRequirement {
        /// The scenario title.
        title: String,
    },
    /// A `###` heading, in a section of requirements, that is not a requirement heading.
    #[error("expected `### Requirement: <name>`, found `{found}`")]
    RequirementHeading {
        /// The heading as written.
        found: String,
    },
    /// A requirement outside the sections that hold requirements.
    #[error(
        "requirement `{name}` is outside the sections that hold requirements; expected it under {expected}"
    )]
    RequirementOutsideSection {
        /// The requirement name.
        name: String,
        /// The sections it may sit in.
        expected: &'static str,
    },
    /// A `FROM:` without its `TO:`, or a `TO:` without its `FROM:`.
    #[error(
        "`{side}: ### Requirement: {name}` has no {missing}; expected `FROM:` and `TO:` in pairs"
    )]
    UnpairedRename {
        /// The side that is present, `FROM` or `TO`.
        side: &'static str,
        /// The requirement name on that side.
        name: String,
        /// The side that is missing.
        missing: &'static str,
    },
    /// A delta file with none of its four sections.
    #[error(
        "expected a `## ADDED Requirements`, `## MODIFIED Requirements`, `## REMOVED Requirements` or `## RENAMED Requirements` section"
    )]
    NoDeltaSection,
}
