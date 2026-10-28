//! The command line: every command and global flag, as clap reads them.

use std::ffi::OsString;

use camino::Utf8PathBuf;
use clap::{ArgGroup, Args, Parser, Subcommand};
use clap_complete::Shell;

use crate::streams::{Io, Outcome};

pub use gsttt_core::VERSION;

/// What does a change affect, across files, packages, languages and the contracts between services?
#[derive(Debug, Parser)]
#[command(name = "gsttt", version = VERSION, disable_help_subcommand = true, flatten_help = true)]
pub struct Cli {
    /// The flags every command takes
    #[command(flatten)]
    pub flags: Flags,

    /// What to do. Without one, gsttt prints this usage and exits with 2
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// The flags every command takes, before or after the command.
#[derive(Debug, Args)]
pub struct Flags {
    /// Run as if gsttt had been started in DIR
    #[arg(short = 'C', value_name = "DIR", global = true)]
    pub directory: Option<Utf8PathBuf>,

    /// Print the answer as one JSON document with every entry
    #[arg(long, global = true)]
    pub json: bool,

    /// Print every entry of a list, where the text cuts it short
    #[arg(long, global = true)]
    pub all: bool,

    /// Answer from the stored index, without refreshing it first
    #[arg(long, global = true)]
    pub no_refresh: bool,
}

/// A command of the binary.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Build or update the index of the tree
    Index,
    /// List the files a change affects, from paths or from `--changed`
    Impact(ImpactArgs),
    /// List the files that import a file
    Dependents(FileArgs),
    /// List the files a file imports
    Dependencies(FileArgs),
    /// Show how one file is connected to another
    Path(PathArgs),
    /// Look up the sites of a contract, or the contracts whose keys contain some text
    Contract(ContractArgs),
    /// Report what the index covered and what it could not read
    Coverage,
    /// Report how the stored index stands against the tree, without refreshing it
    Status,
    /// Answer an agent's questions over the Model Context Protocol, on stdin and stdout
    Mcp,
    /// Commands for the hooks of an editor or an agent
    #[command(subcommand, flatten_help = true)]
    Hook(HookCommand),
    /// Write the starter config into `.gsttt/`
    Init,
    /// Check, show and trust the config
    #[command(subcommand, flatten_help = true)]
    Config(ConfigCommand),
    /// Print a JSON schema
    #[command(subcommand, flatten_help = true)]
    Schema(SchemaCommand),
    /// Print the completion script of a shell
    Completions {
        /// The shell to complete for
        shell: Shell,
    },
}

/// The arguments of `impact`: paths or `--changed`, never both.
#[derive(Debug, Args)]
#[command(group = ArgGroup::new("target").required(true).args(["paths", "changed"]))]
pub struct ImpactArgs {
    /// The files that changed, read from the current directory. One that is not on disk counts as deleted
    #[arg(value_name = "PATH")]
    pub paths: Vec<String>,

    /// Take the files that differ from HEAD instead of naming them
    #[arg(long)]
    pub changed: bool,

    /// Compare with the best common ancestor of HEAD and REV; needs `--changed`
    #[arg(long, value_name = "REV")]
    pub base: Option<String>,

    /// Follow dependents at most N hops from a changed file
    #[arg(long, value_name = "N")]
    pub depth: Option<u32>,
}

/// The arguments of `dependents` and `dependencies`.
#[derive(Debug, Args)]
pub struct FileArgs {
    /// The file, read from the current directory
    #[arg(value_name = "PATH")]
    pub path: String,
}

/// The arguments of `path`.
#[derive(Debug, Args)]
pub struct PathArgs {
    /// The file the chain starts at, read from the current directory
    #[arg(value_name = "FROM")]
    pub from: String,

    /// The file the chain ends at, read from the current directory
    #[arg(value_name = "TO")]
    pub to: String,
}

/// The arguments of `contract`.
#[derive(Debug, Args)]
pub struct ContractArgs {
    /// A contract key such as `http GET /bookings/{id}`, or part of one. Words are joined with a space
    #[arg(value_name = "QUERY", required = true, num_args = 1..)]
    pub query: Vec<String>,
}

impl ContractArgs {
    /// The text to look up.
    pub fn text(&self) -> String {
        self.query.join(" ")
    }
}

/// The commands under `hook`.
#[derive(Debug, Subcommand)]
pub enum HookCommand {
    /// Print one line about the stored index, for the start of a session
    SessionStart,
}

/// The commands under `config`.
#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Evaluate the config and check the options of every module
    Check,
    /// Print the effective setup
    Show,
    /// Allow the external connectors this repository declares to run; asks on a terminal
    Trust,
    /// Forget the trust given to this repository
    Untrust,
}

/// The commands under `schema`.
#[derive(Debug, Subcommand)]
pub enum SchemaCommand {
    /// The messages of the external connector protocol
    External,
}

impl Command {
    /// The command as it is typed, for messages: `impact`, `config check`.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Impact(_) => "impact",
            Self::Dependents(_) => "dependents",
            Self::Dependencies(_) => "dependencies",
            Self::Path(_) => "path",
            Self::Contract(_) => "contract",
            Self::Coverage => "coverage",
            Self::Status => "status",
            Self::Mcp => "mcp",
            Self::Hook(HookCommand::SessionStart) => "hook session-start",
            Self::Init => "init",
            Self::Config(ConfigCommand::Check) => "config check",
            Self::Config(ConfigCommand::Show) => "config show",
            Self::Config(ConfigCommand::Trust) => "config trust",
            Self::Config(ConfigCommand::Untrust) => "config untrust",
            Self::Schema(SchemaCommand::External) => "schema external",
            Self::Completions { .. } => "completions",
        }
    }
}

/// Reads the command line `args`, the program name first.
///
/// A request for help or the version is printed to stdout, and a command line that cannot be read is reported on stderr. Either way the caller gets the [`Outcome`] to exit with in place of a command.
pub fn parse<I, T>(args: I, io: &mut Io<'_>) -> Result<Cli, Outcome>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    Cli::try_parse_from(args).map_err(|error| {
        let text = error.render().ansi().to_string();
        if error.use_stderr() {
            io.complain(text.as_bytes());
        } else if io.answer(text.as_bytes()).is_err() {
            return Outcome::Failed;
        }
        if error.exit_code() == 0 {
            Outcome::Answered
        } else {
            Outcome::Usage
        }
    })
}
