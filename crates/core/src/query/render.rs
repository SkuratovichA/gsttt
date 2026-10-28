//! What every answer shares: text with lists cut at a count, and a footer of coverage and freshness.

use std::fmt;

use serde::Serialize;

use crate::coverage::ConnectorCoverage;
use crate::ids::FileId;
use crate::index::{Freshness, Index};
use crate::text::{join_and, noun, percent, short, thousands};

/// How many entries of a list the text of an answer shows unless every entry is asked for.
pub const LIST_CAP: usize = 30;

/// How many entries of each list the text of an answer shows. The one list that is never cut is the problems of a coverage answer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Entries {
    /// The first [`LIST_CAP`] entries of each list that can be cut, then a line with the count of the rest.
    #[default]
    Capped,
    /// Every entry.
    All,
}

impl Entries {
    /// [`Entries::All`] when the caller asked for every entry (`--all` on the command line, `all` in a tool call),
    /// and [`Entries::Capped`] otherwise. Both surfaces map their flag through this one function, so a list is cut
    /// in the same way wherever it is asked for.
    pub const fn from_flag(all: bool) -> Self {
        if all { Self::All } else { Self::Capped }
    }
}

/// An answer to a question over an index: text for people and agents, and JSON for scripts.
///
/// [`Display`](fmt::Display) gives the text with lists cut at [`LIST_CAP`] entries; [`Answer::text`] gives it with
/// the cut the caller asks for. [`Serialize`] always holds every entry.
pub trait Answer: fmt::Display + Serialize {
    /// Writes the text of the answer, showing `entries` of each list.
    fn write_text(&self, f: &mut fmt::Formatter<'_>, entries: Entries) -> fmt::Result;

    /// The text of the answer, showing `entries` of each list.
    fn text(&self, entries: Entries) -> String {
        struct Text<'a, A: ?Sized>(&'a A, Entries);

        impl<A: Answer + ?Sized> fmt::Display for Text<'_, A> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.write_text(f, self.1)
            }
        }

        Text(self, entries).to_string()
    }
}

/// Implements [`Display`](fmt::Display) for answers as their text with lists cut.
macro_rules! display_as_text {
    ($($answer:ty),+ $(,)?) => {
        $(
            impl ::std::fmt::Display for $answer {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    $crate::query::Answer::write_text(self, f, $crate::query::Entries::Capped)
                }
            }
        )+
    };
}
pub(super) use display_as_text;

/// What one connector read of the files it owns.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ConnectorShare {
    pub connector: String,
    /// Files the connector owns.
    pub files: u32,
    /// Files it read, including those it read past syntax errors and not those it failed on or skipped.
    pub read: u32,
    /// Imports it found.
    pub imports: u32,
    /// Those that resolved to a file, a package or a builtin.
    pub resolved_imports: u32,
    /// `resolved_imports` over `imports`, from 0 to 1, for scripts. The text computes its percentage from the two
    /// counts, because a float gives 28 for 29 of 100.
    pub resolved: f64,
}

impl ConnectorShare {
    fn of(index: &Index, coverage: &ConnectorCoverage) -> Self {
        Self {
            connector: coverage.connector.clone(),
            files: coverage.files,
            read: read_files(index, coverage),
            imports: coverage.imports,
            resolved_imports: coverage.internal.saturating_add(coverage.external),
            resolved: coverage.resolved_share(),
        }
    }

    fn write(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} read {} of {} files, ",
            self.connector,
            thousands(self.read),
            thousands(self.files),
        )?;
        if self.imports == 0 {
            f.write_str("no imports")
        } else {
            write!(
                f,
                "{} of imports resolved",
                percent(self.resolved_imports, self.imports)
            )
        }
    }
}

/// The last two lines of every answer: how much of the tree the connectors read, and how fresh the index is.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Footer {
    /// The connectors whose files the answer involves, in the order the engine ran them. An answer that names no file a connector read lists the connectors that own a file instead, so that it still says what the index covers, and leaves out a connector that owns none.
    pub coverage: Vec<ConnectorShare>,
    pub freshness: Freshness,
    /// The commit the index was built at.
    pub head: Option<String>,
    /// Files that differed from that commit when the index was built.
    pub dirty: u32,
}

impl Footer {
    /// The footer of an answer that involves `files`. An answer that involves no file a connector owns lists the connectors that own a file, and leaves out one that owns none, which is involved in no answer: `graphql` and `sql` detect a project in every tree, so a tree without their documents has them with no file of their own.
    pub(super) fn new(
        index: &Index,
        freshness: Freshness,
        files: impl IntoIterator<Item = FileId>,
    ) -> Self {
        let owners: Vec<&str> = files
            .into_iter()
            .filter_map(|file| index.project_of(file))
            .map(|project| project.connector.as_str())
            .collect();
        let involved =
            |coverage: &&ConnectorCoverage| owners.contains(&coverage.connector.as_str());
        let share = |coverage: &ConnectorCoverage| ConnectorShare::of(index, coverage);
        let mut coverage: Vec<ConnectorShare> = index
            .coverage
            .connectors
            .iter()
            .filter(involved)
            .map(share)
            .collect();
        if coverage.is_empty() {
            coverage = index
                .coverage
                .connectors
                .iter()
                .filter(|connector| connector.files > 0)
                .map(share)
                .collect();
        }
        Self {
            coverage,
            freshness,
            head: index.info.head.clone(),
            dirty: index.info.dirty,
        }
    }

    /// Writes the coverage line and the freshness line, with no newline after the last.
    pub(super) fn write(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("coverage: ")?;
        if self.coverage.is_empty() {
            f.write_str("no connector read any file")?;
        }
        for (position, share) in self.coverage.iter().enumerate() {
            if position > 0 {
                f.write_str("; ")?;
            }
            share.write(f)?;
        }
        f.write_str("\n")?;
        match self.freshness {
            Freshness::Fresh => {
                f.write_str("fresh")?;
                if let Some(head) = &self.head {
                    write!(f, " at {}", short(head))?;
                }
                write!(f, ", {} dirty", noun(self.dirty, "file"))
            }
            Freshness::Stale {
                added,
                removed,
                changed,
            } => write!(
                f,
                "stale: {} since the index",
                moved(added, removed, changed)
            ),
        }
    }
}

/// The files `coverage`'s connector owns and left out before it read them: over the size limit, not UTF-8,
/// unreadable. The engine counts them in `files` and lists them in `Coverage::skipped`, which holds the files of every
/// connector, so the owner of each is looked up.
pub(super) fn skipped_files(index: &Index, coverage: &ConnectorCoverage) -> u32 {
    let owned = index.coverage.skipped.iter().filter(|skipped| {
        index
            .project_of(skipped.file)
            .is_some_and(|project| project.connector == coverage.connector)
    });
    u32::try_from(owned.count()).unwrap_or(u32::MAX)
}

/// The files a connector owns and read: a file it could not read at all, or never saw, holds no imports to search. The
/// engine puts a file in `failed` or in `skipped` and never in both, so the two are taken off the count once each.
pub(super) fn read_files(index: &Index, coverage: &ConnectorCoverage) -> u32 {
    let failed = u32::try_from(coverage.failed.len()).unwrap_or(u32::MAX);
    coverage
        .files
        .saturating_sub(failed)
        .saturating_sub(skipped_files(index, coverage))
}

/// What an answer searched, for the answers that find nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Searched {
    /// The files that connectors read.
    pub files: u32,
    /// The connectors that read them, in the order the engine ran them.
    pub connectors: Vec<String>,
}

impl Searched {
    pub(super) fn of(index: &Index) -> Self {
        let readers = || {
            index
                .coverage
                .connectors
                .iter()
                .filter(|coverage| read_files(index, coverage) > 0)
        };
        Self {
            files: readers().fold(0, |sum, coverage| {
                sum.saturating_add(read_files(index, coverage))
            }),
            connectors: readers()
                .map(|coverage| coverage.connector.clone())
                .collect(),
        }
    }
}

/// `4,213 files read by ts and python`.
impl fmt::Display for Searched {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&noun(self.files, "file"))?;
        if !self.connectors.is_empty() {
            write!(f, " read by {}", join_and(&self.connectors))?;
        }
        Ok(())
    }
}

/// The files a stale index lags behind by, as a sentence part: `2 files changed and 1 was added`.
pub(super) fn moved(added: u32, removed: u32, changed: u32) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (count, one, many) in [
        (changed, "changed", "changed"),
        (added, "was added", "were added"),
        (removed, "was removed", "were removed"),
    ] {
        if count == 0 {
            continue;
        }
        let verb = if count == 1 { one } else { many };
        parts.push(if parts.is_empty() {
            format!("{} {verb}", noun(count, "file"))
        } else {
            format!("{count} {verb}")
        });
    }
    join_and(&parts)
}

/// Writes `heading` with the count of `items`, then one line per item as `each` writes it, then `… N more` for the
/// items `entries` leaves out. An empty list writes nothing.
pub(super) fn list<T>(
    f: &mut fmt::Formatter<'_>,
    heading: &str,
    items: &[T],
    entries: Entries,
    each: impl Fn(&mut fmt::Formatter<'_>, &T) -> fmt::Result,
) -> fmt::Result {
    if items.is_empty() {
        return Ok(());
    }
    writeln!(f, "{heading} ({}):", thousands(items.len()))?;
    rows(f, items, entries, each)
}

/// Writes one line per item as `each` writes it, then `… N more` for the items `entries` leaves out.
pub(super) fn rows<T>(
    f: &mut fmt::Formatter<'_>,
    items: &[T],
    entries: Entries,
    each: impl Fn(&mut fmt::Formatter<'_>, &T) -> fmt::Result,
) -> fmt::Result {
    let shown = match entries {
        Entries::All => items.len(),
        Entries::Capped => items.len().min(LIST_CAP),
    };
    for item in items.iter().take(shown) {
        each(f, item)?;
        f.write_str("\n")?;
    }
    match items.len().saturating_sub(shown) {
        0 => Ok(()),
        rest => writeln!(f, "… {} more", thousands(rest)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_all_flag_asks_for_every_entry() {
        assert_eq!(Entries::from_flag(true), Entries::All);
        assert_eq!(Entries::from_flag(false), Entries::Capped);
        assert_eq!(Entries::from_flag(false), Entries::default());
    }
}
