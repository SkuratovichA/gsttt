//! The commands: what each one checks, and what it ends with.

use camino::Utf8PathBuf;
use clap::CommandFactory;
use gsttt_core::{Loaded, Target};

use crate::cli::{Cli, Command, ConfigCommand, Flags, HookCommand, ImpactArgs, SchemaCommand};
use crate::error::CliError;
use crate::repository::Repository;
use crate::streams::{Io, Outcome};
use crate::{hook, mcp, root};

mod config;
mod impact;
mod index;
mod init;
mod print;
mod questions;
mod schema;
mod update;

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
        Command::Schema(SchemaCommand::External) => schema::external(io),
        Command::Update(args) => update::update(args, io),
        Command::Config(ConfigCommand::Trust) => config::trust(flags, io),
        Command::Config(ConfigCommand::Untrust) => config::untrust(flags, io),
        Command::Config(ConfigCommand::Check) => config::check(&Session::open(flags)?, io),
        Command::Config(ConfigCommand::Show) => config::show(&Session::open(flags)?, io),
        Command::Init => init::init(flags, io),
        Command::Impact(args) => impact::impact(&Session::open(flags)?, args, io),
        Command::Index => index::index(&Session::open(flags)?, io),
        Command::Status => index::status(&Session::open(flags)?, io),
        Command::Mcp => mcp::run(flags),
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
        Command::Mcp if flags.json => Err(CliError::McpTakesNoFlag { flag: "--json" }),
        Command::Mcp if flags.all => Err(CliError::McpTakesNoFlag { flag: "--all" }),
        Command::Mcp if flags.no_refresh => Err(CliError::McpTakesNoFlag {
            flag: "--no-refresh",
        }),
        Command::Update(_) if flags.directory.is_some() => {
            Err(CliError::UpdateTakesNoFlag { flag: "-C" })
        }
        Command::Update(_) if flags.json => Err(CliError::UpdateTakesNoFlag { flag: "--json" }),
        Command::Update(_) if flags.all => Err(CliError::UpdateTakesNoFlag { flag: "--all" }),
        Command::Update(_) if flags.no_refresh => Err(CliError::UpdateTakesNoFlag {
            flag: "--no-refresh",
        }),
        Command::Impact(ImpactArgs {
            changed: false,
            base: Some(_),
            ..
        }) => Err(CliError::BaseNeedsChanged),
        Command::Impact(ImpactArgs {
            changed: true,
            names,
            ..
        }) if !names.is_empty() => Err(CliError::NameNeedsPaths),
        Command::Impact(ImpactArgs { names, .. }) if names.iter().any(|name| is_blank(name)) => {
            Err(CliError::BlankName)
        }
        Command::Contract(query) if is_blank(&query.text()) => Err(CliError::EmptyQuery),
        _ => Ok(()),
    }
}

/// Whether `text` is empty or only whitespace, which names nothing to look up or to ask about.
fn is_blank(text: &str) -> bool {
    text.trim().is_empty()
}
