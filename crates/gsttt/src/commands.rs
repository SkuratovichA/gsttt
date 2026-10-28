//! The commands: what each one checks, and what it ends with.

use clap::CommandFactory;

use crate::cli::{Cli, Command, ContractArgs, Flags, ImpactArgs};
use crate::error::CliError;
use crate::streams::{Io, Outcome};

/// Runs the command `cli` names, writing the answer to `io.stdout` and the failure, if any, to `io.stderr`.
pub fn run(cli: Cli, io: &mut Io<'_>) -> Outcome {
    let Cli { flags, command } = cli;
    let Some(command) = command else {
        return no_command(io);
    };
    tracing::debug!(command = command.name(), ?flags, "running the command");
    match dispatch(&flags, &command, io) {
        Ok(()) => Outcome::Answered,
        Err(error) => error.report(io),
    }
}

/// With no command there is nothing to do, so the usage goes to stderr and the exit code says it was misused.
fn no_command(io: &mut Io<'_>) -> Outcome {
    let usage = Cli::command().render_help();
    io.complain(usage.ansi().to_string().as_bytes());
    Outcome::Usage
}

fn dispatch(flags: &Flags, command: &Command, io: &mut Io<'_>) -> Result<(), CliError> {
    match command {
        Command::Completions { shell } => {
            let mut script = Vec::new();
            clap_complete::generate(*shell, &mut Cli::command(), "gsttt", &mut script);
            io.answer(&script)
        }
        command => {
            check_arguments(flags, command)?;
            Err(not_built(command))
        }
    }
}

/// The arguments clap cannot judge alone: a combination that makes no sense, and a value that is empty.
fn check_arguments(flags: &Flags, command: &Command) -> Result<(), CliError> {
    match command {
        Command::Index if flags.no_refresh => Err(CliError::NoRefreshWithIndex),
        Command::Impact(ImpactArgs {
            changed: false,
            base: Some(_),
            ..
        }) => Err(CliError::BaseNeedsChanged),
        Command::Contract(query) if is_blank(query) => Err(CliError::EmptyQuery),
        _ => Ok(()),
    }
}

fn is_blank(query: &ContractArgs) -> bool {
    query.text().trim().is_empty()
}

/// Every command whose answer needs the config, a connector or the index fails here until it is built.
fn not_built(command: &Command) -> CliError {
    CliError::NotBuilt {
        command: command.name(),
    }
}
