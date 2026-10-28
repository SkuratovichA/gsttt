//! Colour: styled text reaches a terminal and nothing else.

use anstream::AutoStream;
use gsttt::Io;
use gsttt_scenario::scenario;

use super::support::{Sandbox, ran};

/// What the environment of a user who asks for colour looks like: a terminal that supports it, and every variable that forces it. None of it may reach a pipe.
const WANTING_COLOUR: [(&str, &str); 4] = [
    ("CLICOLOR_FORCE", "1"),
    ("CLICOLOR", "1"),
    ("COLORTERM", "truecolor"),
    ("TERM", "xterm-256color"),
];

#[scenario("cli", "Output to a pipe has no colour")]
#[test]
fn output_to_a_pipe_has_no_colour() {
    let sandbox = Sandbox::new();
    for args in [
        &["coverage"][..],
        &["contract", ""][..],
        &["--help"][..],
        &["--version"][..],
        &["completions", "zsh"][..],
        &[][..],
    ] {
        let mut command = sandbox.gsttt();
        command
            .envs(WANTING_COLOUR)
            .env("GSTTT_LOG", "debug")
            .args(args);
        let ran = ran(&mut command);
        assert!(!ran.stdout.contains('\u{1b}'), "{args:?} on stdout");
        assert!(!ran.stderr.contains('\u{1b}'), "{args:?} on stderr");
        assert!(
            !ran.stdout.is_empty() || !ran.stderr.is_empty(),
            "{args:?} printed nothing, so nothing was checked"
        );
    }
}

/// Runs `args` in this process with both streams letting styling through, and returns what each one holds.
fn run_styled(args: &[&str]) -> (String, String) {
    let (mut stdout, mut stderr) = (
        AutoStream::always(Vec::new()),
        AutoStream::always(Vec::new()),
    );
    let mut io = Io::new(&mut stdout, &mut stderr);
    if let Ok(cli) = gsttt::parse(args, &mut io) {
        gsttt::run(cli, &mut io);
    }
    (
        String::from_utf8(stdout.into_inner()).unwrap(),
        String::from_utf8(stderr.into_inner()).unwrap(),
    )
}

/// The control of the test above: the text is styled at its source, so it is the stream that holds the styling back.
#[test]
fn the_text_is_styled_where_a_terminal_would_show_it() {
    let (help, _) = run_styled(&["gsttt", "--help"]);
    assert!(help.contains('\u{1b}'), "{help}");
    let (_, error) = run_styled(&["gsttt", "contract", ""]);
    assert!(error.contains('\u{1b}'), "{error}");
}
