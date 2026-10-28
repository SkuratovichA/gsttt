//! Why a tool call ends in a result with `isError` set.
//!
//! The model reads the message and corrects the call, so each one names the argument or the failure and says what to
//! do. The tool's name leads a message about arguments, because the arguments are the part the model chose.

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
        "impact: name the files to ask about in `paths`, or set `changed` to true to ask about the files that differ \
         from HEAD"
    )]
    NothingAsked,
    #[error(
        "impact: `base` goes with `changed`; set `changed` to true to compare against `base`, or leave `base` out"
    )]
    BaseWithoutChanged,
    /// The source could not answer. Its message is already a sentence that names what failed and what to do, so it
    /// goes to the model as it is.
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error("contract: {0}")]
    Contract(#[from] ContractLookupError),
}
