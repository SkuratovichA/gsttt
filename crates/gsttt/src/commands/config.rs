//! `gsttt config`: checking the config, showing it, and the trust in the external connectors of a repository.

mod show;

use std::fmt::Write as _;
use std::io::{self, BufRead, IsTerminal};

use gsttt_core::text::noun;

use super::Session;
use crate::cli::Flags;
use crate::error::CliError;
use crate::root;
use crate::streams::{Io, Outcome};
use crate::trust::{Declared, Store};

pub(super) use show::show;

/// `gsttt config check`: opens the repository, which reads the config and has every module read its options, and
/// says the config is valid, with a line for each external connector the user does not trust yet. An error ends the
/// run the way it ends any other command that opens the repository. It prints plain text under `--json` too, because
/// it has no answer to give but yes.
pub(super) fn check(session: &Session<'_>, io: &mut Io<'_>) -> Result<Outcome, CliError> {
    let mut text = String::from("The config is valid.\n");
    for external in session
        .repository
        .externals()
        .iter()
        .filter(|external| !external.trusted())
    {
        text.push_str(&external.problem());
        text.push('\n');
    }
    io.answer(text.as_bytes())?;
    Ok(Outcome::Answered)
}

/// `gsttt config trust`: shows what each external connector that waits for trust would run, asks once, and records
/// trust in all of them when the answer is `y` or `yes`. Any other answer trusts nothing.
///
/// It needs stdin and stderr to be terminals and nothing skips that, because the point is that a program driving
/// gsttt cannot trust on the user's behalf.
pub(super) fn trust(flags: &Flags, io: &mut Io<'_>) -> Result<Outcome, CliError> {
    if !(io::stdin().is_terminal() && io::stderr().is_terminal()) {
        return Err(CliError::TrustNeedsTerminal);
    }
    let session = Session::open(flags)?;
    let root = session.repository.root();
    let waiting: Vec<&Declared> = session
        .repository
        .externals()
        .iter()
        .filter(|external| !external.trusted())
        .collect();
    if waiting.is_empty() {
        let nothing =
            format!("There is nothing to trust: no external connector of {root} waits for it.\n");
        io.complain(nothing.as_bytes());
        return Ok(Outcome::Answered);
    }
    // The keys are made before the question, so that trust is recorded for what the person was shown.
    let keys = waiting
        .iter()
        .map(|external| {
            external
                .key()
                .map(str::to_owned)
                .map_err(|error| CliError::Key(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    io.complain(prompt(root.as_str(), &waiting).as_bytes());
    if !agreed(&mut io::stdin().lock()) {
        io.complain(b"Nothing was trusted.\n");
        return Ok(Outcome::Answered);
    }
    Store::locate()?.trust(root, &keys)?;
    let trusted = format!(
        "Trusted {} in {root}.\n",
        noun(count(keys.len()), "external connector")
    );
    io.complain(trusted.as_bytes());
    Ok(Outcome::Answered)
}

/// `gsttt config untrust`: forgets every trust recorded for this root.
pub(super) fn untrust(flags: &Flags, io: &mut Io<'_>) -> Result<Outcome, CliError> {
    let workspace = root::locate(&root::start(flags.directory.as_deref())?)?;
    let root = workspace.root();
    let text = match Store::locate()?.forget(root)? {
        0 => format!("No trust was recorded for {root}.\n"),
        forgotten => format!(
            "Forgot the trust in {} for {root}.\n",
            noun(count(forgotten), "external connector")
        ),
    };
    io.answer(text.as_bytes())?;
    Ok(Outcome::Answered)
}

/// What the person reads before answering: every external with the whole command it would run, and the question.
fn prompt(root: &str, waiting: &[&Declared]) -> String {
    let mut text =
        format!("These external connectors do not run in {root} until you trust them:\n");
    for external in waiting {
        // Writing to a `String` cannot fail.
        let _ = writeln!(text, "  {}: {}", external.name(), external.command());
    }
    let _ = write!(
        text,
        "Run these commands whenever gsttt indexes {root}? [y/N] "
    );
    text
}

/// Whether the first line of `input` is `y` or `yes`, once trimmed. A line that is empty, anything else, or that
/// cannot be read is a no: the default of the question.
fn agreed(input: &mut impl BufRead) -> bool {
    let mut line = String::new();
    input.read_line(&mut line).is_ok() && matches!(line.trim(), "y" | "yes")
}

fn count(length: usize) -> u32 {
    u32::try_from(length).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(text: &str) -> bool {
        agreed(&mut text.as_bytes())
    }

    #[test]
    fn only_y_and_yes_agree() {
        assert!(answer("y\n"));
        assert!(answer("yes\n"));
        assert!(answer("  yes  \r\n"));
        assert!(answer("y"));
        for no in [
            "", "\n", "n\n", "no\n", "Y\n", "YES\n", "yep\n", "y es\n", "sure\n",
        ] {
            assert!(!answer(no), "{no:?}");
        }
    }

    #[test]
    fn only_the_first_line_counts() {
        assert!(!answer("n\ny\n"));
        assert!(answer("y\nn\n"));
    }
}
