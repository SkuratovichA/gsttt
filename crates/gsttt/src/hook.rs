//! `gsttt hook session-start`: one line about the stored index, for an editor or an agent to show at the start of a session.

use gsttt_core::engine::snapshot::{self, Header};
use gsttt_core::text::{noun, short, thousands};
use jiff::Timestamp;

use crate::cli::Flags;
use crate::root;
use crate::streams::Io;

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;
/// Up to this many seconds the age is told in hours, and after it in days.
const TWO_DAYS: u64 = 2 * DAY;

/// Prints the session-start line when the tree has an index, and nothing otherwise.
///
/// It reads the header of the snapshot and nothing else: no `git` process, no config, no registry. Whatever goes wrong, it ends without a word, because a hook that complains would disturb the session that called it.
pub(crate) fn session_start(flags: &Flags, io: &mut Io<'_>) {
    if let Some(line) = line_for_the_tree(flags) {
        // A closed stdout is no reason to disturb the session either.
        drop(io.answer(format!("{line}\n").as_bytes()));
    }
}

fn line_for_the_tree(flags: &Flags) -> Option<String> {
    let start = root::start(flags.directory.as_deref())
        .inspect_err(|error| tracing::debug!(%error, "the hook has no directory to start from"))
        .ok()?;
    let workspace = root::locate(&start)
        .inspect_err(|error| tracing::debug!(%error, "the hook has no root"))
        .ok()?;
    let header = snapshot::peek(workspace.snapshot_path().as_std_path())?;
    session_line(&header, Timestamp::now())
}

/// The line for the snapshot `header` as it stands at `now`: how many files are indexed, at which commit and how many files differ from it, how long ago, and what the reader should do with that.
///
/// It is `None` when `built_at` is not a time or lies after `now`: no line is better than one that says the index was built a few minutes from now.
pub fn session_line(header: &Header, now: Timestamp) -> Option<String> {
    let built: Timestamp = header.built_at.parse().ok()?;
    let seconds = u64::try_from(now.as_second().checked_sub(built.as_second())?).ok()?;
    let commit = header
        .head
        .as_deref()
        .map_or_else(String::new, |head| format!(" at {}", short(head)));
    let dirty = match header.dirty {
        0 => String::new(),
        count => format!(" ({} dirty)", thousands(count)),
    };
    Some(format!(
        "gsttt: {} indexed{commit}{dirty}, {}; use the gsttt tools for impact before changing shared code",
        noun(header.files, "file"),
        age(seconds),
    ))
}

/// How long ago, in the unit people use for it: `just now`, `12 min ago`, `5 h ago`, `3 days ago`.
fn age(seconds: u64) -> String {
    match seconds {
        0..MINUTE => "just now".to_owned(),
        MINUTE..HOUR => format!("{} min ago", seconds / MINUTE),
        HOUR..TWO_DAYS => format!("{} h ago", seconds / HOUR),
        _ => format!("{} days ago", seconds / DAY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed clock: the tests never read the real one.
    fn now() -> Timestamp {
        "2026-10-28T12:00:00Z".parse().unwrap()
    }

    /// The header of an index of 4,213 files at a commit, with 3 files that differ from it, written `seconds_ago` before [`now`].
    fn header(seconds_ago: i64) -> Header {
        Header {
            built_by: "0.1.0".to_owned(),
            built_at: Timestamp::from_second(now().as_second() - seconds_ago)
                .unwrap()
                .to_string(),
            head: Some("a1b2c3d4e5f60718293a4b5c6d7e8f9012345678".to_owned()),
            dirty: 3,
            files: 4213,
            connectors: vec!["ts".to_owned()],
            coverage: String::new(),
        }
    }

    #[test]
    fn the_age_changes_its_unit_where_the_spec_says() {
        for (seconds, said) in [
            (0, "just now"),
            (59, "just now"),
            (MINUTE, "1 min ago"),
            (12 * MINUTE + 59, "12 min ago"),
            (HOUR - 1, "59 min ago"),
            (HOUR, "1 h ago"),
            (5 * HOUR + 1800, "5 h ago"),
            (2 * DAY - 1, "47 h ago"),
            (2 * DAY, "2 days ago"),
            (3 * DAY + HOUR, "3 days ago"),
            (400 * DAY, "400 days ago"),
        ] {
            assert_eq!(age(seconds), said, "{seconds} seconds");
        }
    }

    #[test]
    fn the_age_is_counted_from_built_at_to_now() {
        let line = session_line(&header(12 * 60 + 59), now()).unwrap();
        assert!(line.contains("(3 dirty), 12 min ago; "), "{line}");
    }

    #[test]
    fn the_commit_is_left_out_outside_git_and_the_dirty_count_at_zero() {
        let mut outside = header(30);
        outside.head = None;
        outside.dirty = 0;
        outside.files = 1;
        assert_eq!(
            session_line(&outside, now()).as_deref(),
            Some(
                "gsttt: 1 file indexed, just now; use the gsttt tools for impact before changing shared code"
            )
        );

        let mut clean = header(30);
        clean.dirty = 0;
        let line = session_line(&clean, now()).unwrap();
        assert!(
            line.starts_with("gsttt: 4,213 files indexed at a1b2c3d, just now; "),
            "{line}"
        );
    }

    #[test]
    fn a_header_with_no_usable_time_gives_no_line() {
        let mut broken = header(30);
        "yesterday".clone_into(&mut broken.built_at);
        assert_eq!(session_line(&broken, now()), None);
        assert_eq!(session_line(&header(-1), now()), None, "built after now");
    }
}
