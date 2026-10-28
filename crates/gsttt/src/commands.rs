//! The commands: what each one checks, and what it ends with.

use camino::Utf8PathBuf;
use clap::CommandFactory;
use gsttt_core::{Loaded, Target};

use crate::cli::{Cli, Command, ContractArgs, Flags, HookCommand, ImpactArgs};
use crate::error::CliError;
use crate::repository::Repository;
use crate::streams::{Io, Outcome};
use crate::{hook, root};

mod impact;
mod index;
mod print;
mod questions;

/// Runs the command `cli` names, writing the answer to `io.stdout` and the failure, if any, to `io.stderr`.
pub fn run(cli: Cli, io: &mut Io<'_>) -> Outcome {
    let Cli { flags, command } = cli;
    let Some(command) = command else {
        return no_command(io);
    };
    tracing::debug!(command = command.name(), ?flags, "running the command");
    match dispatch(&flags, &command, io) {
        Ok(outcome) => outcome,
        Err(error) => error.report(io),
    }
}

/// With no command there is nothing to do, so the usage goes to stderr and the exit code says it was misused.
fn no_command(io: &mut Io<'_>) -> Outcome {
    let usage = Cli::command().render_help();
    io.complain(usage.ansi().to_string().as_bytes());
    Outcome::Usage
}

fn dispatch(flags: &Flags, command: &Command, io: &mut Io<'_>) -> Result<Outcome, CliError> {
    check_arguments(flags, command)?;
    match command {
        Command::Completions { shell } => {
            let mut script = Vec::new();
            clap_complete::generate(*shell, &mut Cli::command(), "gsttt", &mut script);
            io.answer(&script)?;
            Ok(Outcome::Answered)
        }
        Command::Hook(HookCommand::SessionStart) => {
            hook::session_start(flags, io);
            Ok(Outcome::Answered)
        }
        Command::Schema(_) => Err(not_built(command)),
        Command::Init | Command::Config(_) => {
            root::locate(&root::start(flags.directory.as_deref())?)?;
            Err(not_built(command))
        }
        Command::Impact(args) => impact::impact(&Session::open(flags)?, args, io),
        Command::Index => index::index(&Session::open(flags)?, io),
        Command::Status => index::status(&Session::open(flags)?, io),
        Command::Dependents(args) => questions::dependents(&Session::open(flags)?, args, io),
        Command::Dependencies(args) => questions::dependencies(&Session::open(flags)?, args, io),
        Command::Path(args) => questions::path(&Session::open(flags)?, args, io),
        Command::Contract(args) => questions::contract(&Session::open(flags)?, args, io),
        Command::Coverage => questions::coverage(&Session::open(flags)?, io),
    }
}

/// The repository a run works on, and the options of the run.
struct Session<'a> {
    flags: &'a Flags,
    /// The directory the run started in, which path arguments are read against.
    start: Utf8PathBuf,
    repository: Repository,
}

impl<'a> Session<'a> {
    /// Finds the root from the directory the run starts in and opens the repository there.
    fn open(flags: &'a Flags) -> Result<Self, CliError> {
        let start = root::start(flags.directory.as_deref())?;
        let repository = Repository::open(&start)?;
        tracing::debug!(root = %repository.root(), "found the root");
        Ok(Self {
            flags,
            start,
            repository,
        })
    }

    /// A path argument, as a path from the root.
    fn argument(&self, argument: &str) -> Result<Target, CliError> {
        Ok(self.repository.argument(&self.start, argument)?)
    }

    /// The stored index as it is, for the commands that never refresh it.
    fn stored(&self) -> Result<Loaded, CliError> {
        self.repository.stored()?.ok_or_else(|| CliError::NoIndex {
            root: self.repository.root().to_owned(),
        })
    }

    /// The index a question answers from: the one a refresh leaves, or the stored one under `--no-refresh`.
    fn loaded(&self) -> Result<Loaded, CliError> {
        if self.flags.no_refresh {
            self.stored()
        } else {
            Ok(self.repository.refreshed()?.loaded)
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

/// The commands of the third run, `init`, `config` and `schema`, fail here until they are built.
fn not_built(command: &Command) -> CliError {
    CliError::NotBuilt {
        command: command.name(),
    }
}
