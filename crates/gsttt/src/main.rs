//! The `gsttt` binary: it reads the command line, runs the command and turns its outcome into an exit code.

use std::process::ExitCode;

use gsttt::{Io, logging, panics, streams};

fn main() -> ExitCode {
    // The streams stay unlocked: a worker thread that logs must not wait for this thread to let go of stderr.
    let (mut stdout, mut stderr) = (streams::stdout(), streams::stderr());
    logging::init(streams::shows_styling(&stderr));
    let mut io = Io::new(&mut stdout, &mut stderr);
    ExitCode::from(panics::guard(&mut io, |io| {
        match gsttt::parse(std::env::args_os(), io) {
            Ok(cli) => gsttt::run(cli, io),
            Err(outcome) => outcome,
        }
    }))
}
