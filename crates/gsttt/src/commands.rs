//! The commands: what each one checks, and what it ends with.

use camino::Utf8Path;
use clap::CommandFactory;
use gsttt_core::RepoPath;
use gsttt_core::engine::snapshot;
use gsttt_core::engine::workspace::Workspace;

use crate::cli::{Cli, Command, ContractArgs, Flags, HookCommand, ImpactArgs};
use crate::error::CliError;
use crate::streams::{Io, Outcome};
use crate::{hook, root};

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
        Command::Hook(HookCommand::SessionStart) => {
            hook::session_start(flags, io);
            Ok(())
        }
        command => {
            check_arguments(flags, command)?;
            if command.needs_root() {
                let start = root::start(flags.directory.as_deref())?;
                let workspace = root::locate(&start)?;
                tracing::debug!(root = %workspace.root(), "found the root");
                read_paths(command, workspace.root(), &start)?;
                if answers_from_the_stored_index(flags, command) {
                    require_index(&workspace)?;
                }
            }
            Err(not_built(command))
        }
    }
}

/// Reads every path argument of `command` against `base`, the directory the run started in, as a path from `root`.
/// A usage error is told before anything reads the index, so the answer does not depend on it.
fn read_paths(
    command: &Command,
    root: &Utf8Path,
    base: &Utf8Path,
) -> Result<Vec<RepoPath>, CliError> {
    command
        .path_arguments()
        .into_iter()
        .map(|argument| Ok(RepoPath::from_argument(root, base, argument)?))
        .collect()
}

/// Whether the command reads the index as it was stored, without refreshing it first.
fn answers_from_the_stored_index(flags: &Flags, command: &Command) -> bool {
    matches!(command, Command::Status) || (flags.no_refresh && command.is_question())
}

/// Fails when the workspace has no index to read.
fn require_index(workspace: &Workspace) -> Result<(), CliError> {
    match snapshot::peek(workspace.snapshot_path().as_std_path()) {
        Some(_) => Ok(()),
        None => Err(CliError::NoIndex {
            root: workspace.root().to_owned(),
        }),
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
