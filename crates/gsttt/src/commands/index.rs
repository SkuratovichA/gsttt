//! The commands about the index itself: `index` builds it, and `status` says how it stands.

use std::fmt;
use std::time::Duration;

use gsttt_core::query::{self, Answer, Entries};
use gsttt_core::text::{duration, noun, short, thousands};
use gsttt_core::{Freshness, Index};
use serde::{Serialize, Serializer};

use super::{print, stored};
use crate::cli::Flags;
use crate::error::CliError;
use crate::repository::{Refreshed, Repository};
use crate::streams::{Io, Outcome};

/// `gsttt index`: brings the index up to date and prints a summary of the run.
pub(super) fn index(
    repository: &Repository,
    flags: &Flags,
    io: &mut Io<'_>,
) -> Result<Outcome, CliError> {
    let refreshed = repository.refreshed()?;
    print::answer(&Summary::of(&refreshed), flags, io)
}

/// `gsttt status`: how the stored index stands against the tree. It never refreshes, so that asking changes
/// nothing about the answer to the next question.
pub(super) fn status(
    repository: &Repository,
    flags: &Flags,
    io: &mut Io<'_>,
) -> Result<Outcome, CliError> {
    let loaded = stored(repository)?;
    print::answer(&query::status(&loaded.index, loaded.freshness), flags, io)
}

/// What `index` reports of a run.
#[derive(Debug, Serialize)]
struct Summary {
    /// The files the index holds.
    files: u32,
    /// Files a connector parsed in this run.
    parsed: u32,
    /// Files whose facts came from the stored index.
    reused: u32,
    /// The stored index already described the tree, so the run changed nothing.
    unchanged: bool,
    /// The commit the tree is at, `None` outside git and before the first commit.
    head: Option<String>,
    /// Files that differ from that commit.
    dirty: u32,
    #[serde(rename = "elapsed_ms", serialize_with = "in_milliseconds")]
    elapsed: Duration,
    /// The coverage line that ends every answer.
    coverage: String,
}

impl Summary {
    fn of(refreshed: &Refreshed) -> Self {
        let index = &refreshed.loaded.index;
        Self {
            files: u32::try_from(index.graph.file_count()).unwrap_or(u32::MAX),
            parsed: refreshed.parsed,
            reused: refreshed.reused,
            unchanged: refreshed.unchanged,
            head: index.info.head.clone(),
            dirty: index.info.dirty,
            elapsed: refreshed.elapsed,
            coverage: coverage_line(index),
        }
    }

    /// ` at a1b2c3d`, or nothing outside git.
    fn write_head(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.head {
            Some(head) => write!(f, " at {}", short(head)),
            None => Ok(()),
        }
    }
}

fn in_milliseconds<S: Serializer>(elapsed: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_u64(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
}

/// `Indexed 4,213 files at a1b2c3d (3 dirty) in 1.2 s: 112 parsed, 4,101 reused.` and the coverage line, or
/// `The index already describes the tree: 4,213 files at a1b2c3d.` and the coverage line.
impl Answer for Summary {
    fn write_text(&self, f: &mut fmt::Formatter<'_>, _entries: Entries) -> fmt::Result {
        if self.unchanged {
            write!(
                f,
                "The index already describes the tree: {}",
                noun(self.files, "file")
            )?;
            self.write_head(f)?;
            f.write_str(".\n")?;
        } else {
            write!(f, "Indexed {}", noun(self.files, "file"))?;
            self.write_head(f)?;
            if self.dirty > 0 {
                write!(f, " ({} dirty)", thousands(self.dirty))?;
            }
            let micros = u64::try_from(self.elapsed.as_micros()).unwrap_or(u64::MAX);
            writeln!(
                f,
                " in {}: {} parsed, {} reused.",
                duration(micros),
                thousands(self.parsed),
                thousands(self.reused)
            )?;
        }
        f.write_str(&self.coverage)
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_text(f, Entries::Capped)
    }
}

/// The label of the line every answer ends with, above its freshness line.
const COVERAGE_LABEL: &str = "coverage: ";

/// The coverage line of the whole index, as the footer of every answer writes it.
///
/// `gsttt-core` renders the line and offers no accessor for it, so it is taken from the text of `status`, the one
/// answer that involves no file and so lists every connector. A test pins it against the footer of `coverage`.
fn coverage_line(index: &Index) -> String {
    query::status(index, Freshness::Fresh)
        .text(Entries::Capped)
        .lines()
        .find(|line| line.starts_with(COVERAGE_LABEL))
        .unwrap_or_default()
        .to_owned()
}
