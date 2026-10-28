//! `coverage` and `status`: what the connectors read and what they could not, and how the index was built.

use std::fmt;

use serde::Serialize;

use crate::connector::Provide;
use crate::coverage::{ConnectorCoverage, FailedFile, ReasonCount};
use crate::fact::{Line, ParseFailure, UnresolvedReason};
use crate::graph::Graph;
use crate::index::{Freshness, Index, Timings};

use super::render::{
    Answer, Entries, Footer, display_as_text, join_and, list, moved, noun, percent, short,
    thousands,
};

/// A line of a file, written `path:line`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Spot {
    pub path: String,
    pub line: Line,
}

impl fmt::Display for Spot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.path, self.line)
    }
}

/// A file that was not read, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Failure {
    pub path: String,
    #[serde(flatten)]
    pub failure: ParseFailure,
}

/// How often one reason came up, and where first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Reasons {
    #[serde(flatten)]
    pub reason: UnresolvedReason,
    pub count: u32,
    /// The first place in path order.
    pub first: Spot,
}

/// What one connector read of the files it owns.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ConnectorReport {
    pub connector: String,
    /// The roots of its projects, `""` for the repository root.
    pub projects: Vec<String>,
    /// Files it owns.
    pub files: u32,
    /// Files it read without a syntax error.
    pub parsed: u32,
    /// Files it read past syntax errors.
    pub recovered: u32,
    /// Files it could not read.
    pub failed: Vec<Failure>,
    pub imports: u32,
    /// Imports that point at files of the repository.
    pub internal: u32,
    /// Imports of packages and of the language's own modules.
    pub external: u32,
    /// The share of imports that resolved to a file, a package or a builtin, from 0 to 1.
    pub resolved: f64,
    /// Imports that point nowhere, by reason, most first.
    pub unresolved: Vec<Reasons>,
    /// Sites it saw but could not read, by reason, most first.
    pub unread_sites: Vec<Reasons>,
    /// The contracts it finds and what it cannot see of them.
    pub provides: Vec<Provide>,
}

/// Files of one programming language that no connector read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UnclaimedLanguage {
    pub language: String,
    pub files: u32,
    /// The extensions of those files without their dot, in order.
    pub extensions: Vec<String>,
}

/// Files with one extension that no connector read and that belong to no programming language the engine knows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UnclaimedExtension {
    /// The extension without its dot, `""` for files without one.
    pub extension: String,
    pub files: u32,
}

/// What the connectors read and what they could not.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CoverageReport {
    /// One entry per connector that ran, in the order the engine ran them.
    pub connectors: Vec<ConnectorReport>,
    /// Code no connector read, by language, most files first.
    pub languages: Vec<UnclaimedLanguage>,
    /// Other files no connector read, by extension, most files first.
    pub other_extensions: Vec<UnclaimedExtension>,
    /// Files left out before any connector saw them: over the size limit, not UTF-8, unreadable.
    pub skipped: Vec<Failure>,
    pub footer: Footer,
}

/// What every connector read and could not read, and the files no connector read.
pub fn coverage(index: &Index, freshness: Freshness) -> CoverageReport {
    let graph = &index.graph;
    let counted = &index.coverage;

    let mut languages: Vec<UnclaimedLanguage> = Vec::new();
    let mut other_extensions = Vec::new();
    for unclaimed in &counted.unclaimed {
        let Some(language) = unclaimed.language() else {
            other_extensions.push(UnclaimedExtension {
                extension: unclaimed.extension.clone(),
                files: unclaimed.files,
            });
            continue;
        };
        match languages
            .iter_mut()
            .find(|known| known.language == language)
        {
            Some(known) => {
                known.files = known.files.saturating_add(unclaimed.files);
                known.extensions.push(unclaimed.extension.clone());
            }
            None => languages.push(UnclaimedLanguage {
                language: language.to_owned(),
                files: unclaimed.files,
                extensions: vec![unclaimed.extension.clone()],
            }),
        }
    }
    for language in &mut languages {
        language.extensions.sort_unstable();
    }
    languages.sort_by(|a, b| (b.files, &a.language).cmp(&(a.files, &b.language)));
    other_extensions.sort_by(|a, b| (b.files, &a.extension).cmp(&(a.files, &b.extension)));

    CoverageReport {
        connectors: counted
            .connectors
            .iter()
            .map(|connector| connector_report(graph, connector))
            .collect(),
        languages,
        other_extensions,
        skipped: failures(graph, &counted.skipped),
        footer: Footer::new(index, freshness, []),
    }
}

fn connector_report(graph: &Graph, connector: &ConnectorCoverage) -> ConnectorReport {
    let failed = u32::try_from(connector.failed.len()).unwrap_or(u32::MAX);
    ConnectorReport {
        connector: connector.connector.clone(),
        projects: connector.projects.clone(),
        files: connector.files,
        parsed: connector
            .files
            .saturating_sub(connector.recovered)
            .saturating_sub(failed),
        recovered: connector.recovered,
        failed: failures(graph, &connector.failed),
        imports: connector.imports,
        internal: connector.internal,
        external: connector.external,
        resolved: connector.resolved_share(),
        unresolved: reasons(graph, &connector.unresolved),
        unread_sites: reasons(graph, &connector.blind),
        provides: connector.provides.clone(),
    }
}

fn failures(graph: &Graph, failed: &[FailedFile]) -> Vec<Failure> {
    let mut failures: Vec<Failure> = failed
        .iter()
        .filter_map(|failed| {
            Some(Failure {
                path: graph.path(failed.file)?.to_string(),
                failure: failed.failure.clone(),
            })
        })
        .collect();
    failures.sort_by(|a, b| a.path.cmp(&b.path));
    failures
}

fn reasons(graph: &Graph, counted: &[ReasonCount]) -> Vec<Reasons> {
    counted
        .iter()
        .filter_map(|counted| {
            Some(Reasons {
                reason: counted.reason.clone(),
                count: counted.count,
                first: Spot {
                    path: graph.path(counted.first.file)?.to_string(),
                    line: counted.first.line,
                },
            })
        })
        .collect()
}

impl ConnectorReport {
    fn write(&self, f: &mut fmt::Formatter<'_>, entries: Entries) -> fmt::Result {
        let name = &self.connector;
        write!(f, "{name}: {}", noun(self.files, "file"))?;
        if !self.projects.is_empty() {
            let roots: Vec<String> = self
                .projects
                .iter()
                .map(|root| {
                    if root.is_empty() {
                        "the repository root".to_owned()
                    } else {
                        root.clone()
                    }
                })
                .collect();
            write!(f, " in {}", join_and(&roots))?;
        }
        writeln!(f)?;
        writeln!(
            f,
            "{name} parse status: {} read cleanly, {} read past syntax errors, {} failed",
            thousands(self.parsed),
            thousands(self.recovered),
            thousands(self.failed.len())
        )?;
        if self.imports == 0 {
            writeln!(f, "{name} imports: none")?;
        } else {
            writeln!(
                f,
                "{name} imports: {}, {} resolved ({} to files, {} to packages and builtins)",
                thousands(self.imports),
                percent(self.resolved),
                thousands(self.internal),
                thousands(self.external)
            )?;
        }
        list(
            f,
            &format!("{name} failed files"),
            &self.failed,
            entries,
            |f, failed| write!(f, "{}  {}", failed.path, failure_text(&failed.failure)),
        )?;
        let unresolved = format!("{name} unresolved imports");
        list(f, &unresolved, &self.unresolved, entries, |f, reason| {
            write_reason(f, reason, "import")
        })?;
        let unread = format!("{name} unread sites");
        list(f, &unread, &self.unread_sites, entries, |f, reason| {
            write_reason(f, reason, "site")
        })?;
        list(
            f,
            &format!("{name} provides"),
            &self.provides,
            entries,
            |f, provide| {
                write!(f, "{} {}", provide.kind, provide.role)?;
                if !provide.via.is_empty() {
                    write!(f, " via {}", provide.via.join(", "))?;
                }
                for spot in &provide.blind_spots {
                    write!(f, "; {spot}")?;
                }
                Ok(())
            },
        )
    }
}

/// `no such file  3 imports, first at src/a.ts:4`.
fn write_reason(f: &mut fmt::Formatter<'_>, reasons: &Reasons, what: &str) -> fmt::Result {
    write!(
        f,
        "{}  {}, first at {}",
        reasons.reason,
        noun(reasons.count, what),
        reasons.first
    )
}

/// How a file failed, in a sentence part.
fn failure_text(failure: &ParseFailure) -> String {
    match failure {
        ParseFailure::Panicked { message } => format!("the parser panicked: {message}"),
        ParseFailure::TooLarge { bytes, limit } => format!(
            "{} bytes, over the limit of {}",
            thousands(*bytes),
            thousands(*limit)
        ),
        ParseFailure::NotUtf8 => "not valid UTF-8".to_owned(),
        ParseFailure::Unreadable { message } => format!("could not be read: {message}"),
        ParseFailure::Syntax { line, message } => format!("syntax error at line {line}: {message}"),
    }
}

impl Answer for CoverageReport {
    fn write_text(&self, f: &mut fmt::Formatter<'_>, entries: Entries) -> fmt::Result {
        if self.connectors.is_empty() {
            writeln!(f, "no connector read any file")?;
        }
        for connector in &self.connectors {
            connector.write(f, entries)?;
        }
        list(
            f,
            "files no connector read, by language",
            &self.languages,
            entries,
            |f, language| {
                let extensions: Vec<String> = language
                    .extensions
                    .iter()
                    .map(|extension| format!(".{extension}"))
                    .collect();
                write!(
                    f,
                    "{}  {} ({})",
                    language.language,
                    noun(language.files, "file"),
                    extensions.join(", ")
                )
            },
        )?;
        list(
            f,
            "other files no connector read, by extension",
            &self.other_extensions,
            entries,
            |f, other| {
                if other.extension.is_empty() {
                    write!(f, "(no extension)  {}", noun(other.files, "file"))
                } else {
                    write!(f, ".{}  {}", other.extension, noun(other.files, "file"))
                }
            },
        )?;
        list(
            f,
            "files skipped before any connector read them",
            &self.skipped,
            entries,
            |f, skipped| write!(f, "{}  {}", skipped.path, failure_text(&skipped.failure)),
        )?;
        self.footer.write(f)
    }
}

/// How an index was built and how it stands against the tree.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Status {
    /// The gsttt version that built the index.
    pub built_by: String,
    /// The commit the tree was at when the index was built.
    pub head: Option<String>,
    /// The files the index holds.
    pub files: u32,
    /// Files that differed from the commit when the index was built.
    pub dirty: u32,
    /// Files parsed by the run that made the index.
    pub parsed: u32,
    /// Files whose facts that run took from the cache.
    pub reused: u32,
    /// How long each phase of that run took.
    pub timings: Timings,
    pub freshness: Freshness,
    pub footer: Footer,
}

/// How the index was built: by which version, at which commit, from how many files, in how long.
pub fn status(index: &Index, freshness: Freshness) -> Status {
    let info = &index.info;
    Status {
        built_by: info.built_by.clone(),
        head: info.head.clone(),
        files: u32::try_from(index.graph.file_count()).unwrap_or(u32::MAX),
        dirty: info.dirty,
        parsed: info.parsed,
        reused: info.reused,
        timings: info.timings,
        freshness,
        footer: Footer::new(index, freshness, []),
    }
}

impl Answer for Status {
    fn write_text(&self, f: &mut fmt::Formatter<'_>, _entries: Entries) -> fmt::Result {
        write!(f, "built by gsttt {}", self.built_by)?;
        if let Some(head) = &self.head {
            write!(f, " at {}", short(head))?;
        }
        writeln!(f, ", {} dirty", noun(self.dirty, "file"))?;
        writeln!(
            f,
            "{}: {} parsed, {} reused",
            noun(self.files, "file"),
            thousands(self.parsed),
            thousands(self.reused)
        )?;
        let timings = &self.timings;
        writeln!(
            f,
            "took {} in all: files {}, parse {}, resolve {}, link {}, graph {}",
            duration(timings.total),
            duration(timings.files),
            duration(timings.parse),
            duration(timings.resolve),
            duration(timings.link),
            duration(timings.graph)
        )?;
        match self.freshness {
            Freshness::Fresh => writeln!(f, "nothing has changed since the index was built")?,
            Freshness::Stale {
                added,
                removed,
                changed,
            } => writeln!(
                f,
                "{} since the index was built",
                moved(added, removed, changed)
            )?,
        }
        self.footer.write(f)
    }
}

/// A time in microseconds, written as microseconds, or as milliseconds or seconds with one decimal.
fn duration(micros: u64) -> String {
    if micros < 1_000 {
        return format!("{micros} µs");
    }
    // Below this many microseconds a time in milliseconds still rounds to less than a thousand of them.
    if micros < 999_950 {
        format!("{} ms", tenths(micros, 1_000))
    } else {
        format!("{} s", tenths(micros, 1_000_000))
    }
}

/// `micros / unit` rounded to one decimal place, with integers only so that no float's rounding decides it.
fn tenths(micros: u64, unit: u64) -> String {
    let tenths = micros.saturating_mul(10).saturating_add(unit / 2) / unit;
    format!("{}.{}", tenths / 10, tenths % 10)
}

display_as_text!(CoverageReport, Status);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_pick_a_unit_and_round_once() {
        let spelled: Vec<String> = [
            0, 999, 1_000, 1_049, 1_050, 12_345, 999_949, 999_950, 2_500_000,
        ]
        .into_iter()
        .map(duration)
        .collect();
        assert_eq!(
            spelled,
            [
                "0 µs", "999 µs", "1.0 ms", "1.0 ms", "1.1 ms", "12.3 ms", "999.9 ms", "1.0 s",
                "2.5 s"
            ]
        );
    }

    #[test]
    fn the_largest_duration_does_not_overflow() {
        assert!(duration(u64::MAX).ends_with(" s"));
    }
}
