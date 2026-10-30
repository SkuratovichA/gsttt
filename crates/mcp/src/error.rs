//! Why a tool call ends in a result with `isError` set.
//!
//! The model reads the message and corrects the call, so each one names the argument or the failure and says what to do. The tool's name leads a message about arguments, because the arguments are the part the model chose.

use gsttt_core::query::ContractLookupError;

use crate::source::SourceError;
use crate::tools::ToolName;

/// A call to a known tool that could not be answered.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ToolError {
    #[error("{tool}: `{argument}` is not an argument of this tool; {takes}")]
    UnknownArgument {
        tool: ToolName,
        argument: String,
        /// The arguments the tool does take, as the end of a sentence.
        takes: String,
    },
    #[error("{tool}: `{argument}` is required and must be {expects}")]
    MissingArgument {
        tool: ToolName,
        argument: &'static str,
        expects: &'static str,
    },
    #[error("{tool}: `{argument}` must be {expects}")]
    WrongArgument {
        tool: ToolName,
        argument: &'static str,
        expects: &'static str,
    },
    /// The arguments did not read, and no single argument is to blame, so serde's own words are all there is to say.
    #[error("{tool}: the arguments could not be read: {detail}")]
    Unreadable { tool: ToolName, detail: String },
    #[error("impact: pass either `paths` or `changed`, not both")]
    PathsAndChanged,
    #[error(
        "impact: `names` needs `paths`; name the file that declares them in `paths`, or leave `names` out to ask \
         about the files as a whole"
    )]
    NamesWithoutPaths,
    #[error(
        "impact: name the files to ask about in `paths`, or set `changed` to true to ask about the files that differ \
         from HEAD"
    )]
    NothingAsked,
    #[error(
        "impact: `base` goes with `changed`; set `changed` to true to compare against `base`, or leave `base` out"
    )]
    BaseWithoutChanged,
    /// A name that is empty or only whitespace declares nothing, so the walk by it reaches nothing, which a model reads as a name that has no callers.
    #[error(
        "impact: `names` holds a blank name; write each name as the file declares it, for example \"getHash\" or \
         \"Runner.start\""
    )]
    BlankName,
    /// The question names a file the index does not hold. The text is the answer's own, the one the command line prints on stderr when it fails for the same reason.
    #[error("{0}")]
    NotInIndex(String),
    /// The source could not answer. Its message is already a sentence that names what failed and what to do, so it goes to the model as it is.
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error("contract: {0}")]
    Contract(#[from] ContractLookupError),
    /// The work behind the call did not finish: it panicked, or the runtime was shutting down. The cause is in the log, and the model is told to ask again.
    #[error(
        "{tool}: the call stopped before it could answer; ask again, or run the same question on the gsttt command \
         line to see why"
    )]
    Stopped { tool: ToolName },
}
