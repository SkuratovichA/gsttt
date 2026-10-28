//! What the binary does with a panic.
//!
//! The engine catches the panics of connector calls and reports each as a line of coverage, so the hook prints nothing for those. A panic that nothing catches ends the command: [`guard`] turns it into one line on stderr and the exit code 101, which is what Rust itself exits with when a panic unwinds out of `main`.

use std::panic::{self, AssertUnwindSafe, PanicHookInfo};
use std::sync::OnceLock;

use gsttt_core::engine::pipeline::quiet_contained_panics;

use crate::streams::{Io, Outcome};

/// Where a bug is reported.
const ISSUES: &str = "https://github.com/SkuratovichA/gsttt/issues";

/// The exit code of a command that stopped on a bug.
const BUG: u8 = 101;

/// What the hook saw of the first panic that no connector call contained.
///
/// It lives in a static and not in a thread-local because the panic and `main` are usually on different threads: a worker that panics hands its payload to the thread that waits for it with `resume_unwind`, which does not run the hook again, so only the worker's own thread-local would hold the message and the place.
static FIRST: OnceLock<Bug> = OnceLock::new();

/// A panic as the hook saw it.
#[derive(Debug, Default)]
struct Bug {
    /// What the panic said, `None` when its payload was not text.
    message: Option<String>,
    /// `file:line` of the panic, when the runtime knows it.
    location: Option<String>,
}

impl Bug {
    fn seen_by(info: &PanicHookInfo<'_>) -> Self {
        Self {
            message: info.payload_as_str().map(str::to_owned),
            location: info
                .location()
                .map(|place| format!("{}:{}", place.file(), place.line())),
        }
    }

    /// The one line `main` prints: what failed, where when that is known, and where to report it.
    fn line(&self) -> String {
        let said = self.message.as_deref().map_or_else(String::new, one_line);
        let said = if said.is_empty() {
            "a panic without a message"
        } else {
            &said
        };
        let place = self
            .location
            .as_deref()
            .map_or_else(String::new, |location| format!(" at {location}"));
        format!("gsttt stopped on a bug{place}: {said}. Please report it at {ISSUES}")
    }
}

/// `text` on one line, without a closing full stop, so that a sentence can go after it.
fn one_line(text: &str) -> String {
    let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match words.strip_suffix('.') {
        Some(without) => without.to_owned(),
        None => words,
    }
}

/// Whether the person asked for Rust's own panic output, which they do by setting `RUST_BACKTRACE` to anything but `0`, the way the standard hook reads it.
fn standard_output_wanted() -> bool {
    std::env::var_os("RUST_BACKTRACE").is_some_and(|value| value != "0")
}

/// Records the first panic, prints nothing for it unless the person asked for the standard output, and leaves out the panics the engine contains. The order matters: the engine's filter wraps whatever hook is installed when it is called, so it has to come after ours.
fn install() {
    let standard = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        // The first panic explains the rest: a later one is often the poisoned lock that the first left behind.
        drop(FIRST.set(Bug::seen_by(info)));
        if standard_output_wanted() {
            standard(info);
        }
    }));
    quiet_contained_panics();
}

/// Installs the panic hook and runs `command`, returning the exit code of the process. A panic that gets out of the command is told in one line on stderr and ends in 101. Call it once per process.
pub fn guard(io: &mut Io<'_>, command: impl FnOnce(&mut Io<'_>) -> Outcome) -> u8 {
    install();
    // The streams are only written to after a panic, never read, so a half-finished write cannot be mistaken for a whole one.
    let Ok(outcome) = panic::catch_unwind(AssertUnwindSafe(|| command(&mut *io))) else {
        let line = FIRST.get().map_or_else(|| Bug::default().line(), Bug::line);
        io.complain(format!("{line}\n").as_bytes());
        return BUG;
    };
    outcome.code()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn bug(message: Option<&str>, location: Option<&str>) -> Bug {
        Bug {
            message: message.map(str::to_owned),
            location: location.map(str::to_owned),
        }
    }

    #[test]
    fn the_line_names_what_failed_where_and_the_address() {
        assert_eq!(
            bug(
                Some("index out of bounds: the len is 3 but the index is 5"),
                Some("crates/core/src/engine/build.rs:142")
            )
            .line(),
            "gsttt stopped on a bug at crates/core/src/engine/build.rs:142: index out of bounds: the len is 3 but the \
             index is 5. Please report it at https://github.com/SkuratovichA/gsttt/issues"
        );
    }

    #[test]
    fn a_place_the_runtime_does_not_know_is_left_out() {
        assert_eq!(
            bug(Some("boom"), None).line(),
            "gsttt stopped on a bug: boom. Please report it at https://github.com/SkuratovichA/gsttt/issues"
        );
    }

    #[test]
    fn a_missing_or_blank_message_is_said_so() {
        let expected = "gsttt stopped on a bug at a.rs:1: a panic without a message. Please report it at \
                        https://github.com/SkuratovichA/gsttt/issues";
        assert_eq!(bug(None, Some("a.rs:1")).line(), expected);
        assert_eq!(bug(Some(" \n\t"), Some("a.rs:1")).line(), expected);
    }

    #[test]
    fn a_message_of_several_lines_becomes_one_and_its_closing_stop_is_not_doubled() {
        assert_eq!(
            bug(Some("left != right\n  left: 1\n right: 2.\n"), None).line(),
            "gsttt stopped on a bug: left != right left: 1 right: 2. Please report it at \
             https://github.com/SkuratovichA/gsttt/issues"
        );
        assert!(
            bug(Some("it ends in an ellipsis..."), None)
                .line()
                .contains("an ellipsis... Please"),
        );
    }

    /// Text with every kind of line break in it, and the full stops the line strips.
    fn text() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop_oneof![
                any::<char>(),
                Just('\n'),
                Just('\r'),
                Just('\u{2028}'),
                Just('\u{85}'),
                Just('.'),
                Just(' '),
            ],
            0..40,
        )
        .prop_map(|chars| chars.into_iter().collect())
    }

    proptest! {
        #[test]
        fn the_line_is_one_line_whatever_the_panic_said(
            message in proptest::option::of(text()),
            file in "[a-z/._-]{1,30}",
            number in 1_u32..100_000,
        ) {
            let line = Bug { message, location: Some(format!("{file}:{number}")) }.line();
            prop_assert!(!line.contains(['\n', '\r']), "{line:?}");
            prop_assert!(line.starts_with("gsttt stopped on a bug at "), "{line:?}");
            prop_assert!(line.ends_with(ISSUES), "{line:?}");
        }
    }
}
