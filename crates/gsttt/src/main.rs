//! The `gsttt` binary: it reads the command line, runs the command and turns its outcome into an exit code.

use std::io;
use std::process::ExitCode;

use gsttt::Io;

fn main() -> ExitCode {
    let (mut stdout, mut stderr) = (io::stdout().lock(), io::stderr().lock());
    let mut streams = Io::new(&mut stdout, &mut stderr);
    let outcome = match gsttt::parse(std::env::args_os(), &mut streams) {
        Ok(cli) => gsttt::run(cli, &mut streams),
        Err(outcome) => outcome,
    };
    ExitCode::from(outcome.code())
}
