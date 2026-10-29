//! The seven tools: what each is called, what it answers and when to call it, and the schema of its arguments.

use std::fmt;
use std::sync::Arc;

use rmcp::handler::server::common::schema_for_input;
use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::Value;

use crate::arguments::{
    ContractArguments, CoverageArguments, ImpactArguments, NeighbourArguments, NoArguments,
    RouteArguments,
};

/// A tool of the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolName {
    Impact,
    Dependents,
    Dependencies,
    Path,
    Contract,
    Coverage,
    IndexStatus,
}

impl ToolName {
    /// Every tool, in the order `tools/list` gives them.
    pub(crate) const ALL: [Self; 7] = [
        Self::Impact,
        Self::Dependents,
        Self::Dependencies,
        Self::Path,
        Self::Contract,
        Self::Coverage,
        Self::IndexStatus,
    ];

    /// The name a client calls the tool by.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Impact => "impact",
            Self::Dependents => "dependents",
            Self::Dependencies => "dependencies",
            Self::Path => "path",
            Self::Contract => "contract",
            Self::Coverage => "coverage",
            Self::IndexStatus => "index_status",
        }
    }

    /// The tool a client called by `name`, or `None` for a name the server does not offer.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.name() == name)
    }

    /// The name as a person reads it in a list of tools.
    const fn title(self) -> &'static str {
        match self {
            Self::Impact => "Impact of a change",
            Self::Dependents => "Files that import a file",
            Self::Dependencies => "Imports of a file",
            Self::Path => "Chain between two files",
            Self::Contract => "Sites of a contract",
            Self::Coverage => "What the index covers",
            Self::IndexStatus => "State of the index",
        }
    }

    /// What the tool answers and when to call it, for the model that chooses between tools.
    const fn description(self) -> &'static str {
        match self {
            Self::Impact => {
                "Lists the files that a change to some files can break: the files that import them, directly or \
                 through other files, and the files that share a contract with them (an HTTP route, a GraphQL \
                 operation, a gRPC method, a table, a queue or an environment variable). Tests the change reaches \
                 are listed apart. Call it before changing a file that other code may use, and with `changed` set \
                 to true before finishing a task, to see what your edits reach. A question about one function, \
                 class, method or variable is asked with its name in `names`, beside the file that declares it in \
                 `paths`: the answer then lists its direct callers, the declarations that refer to it and the lines \
                 they start on, so ask again about a caller to go further. The answer ends with what the index \
                 covered and how fresh it is."
            }
            Self::Dependents => {
                "Lists the files that import one file, one level up, with how each import is written. Call it \
                 before renaming, moving or deleting a file, to find what has to change with it."
            }
            Self::Dependencies => {
                "Lists what one file imports: the files, the third-party packages and the builtins, and the imports \
                 that resolve to nothing, with the reason. Call it to learn what a file relies on, or why an import \
                 does not resolve."
            }
            Self::Path => {
                "Finds a shortest chain of imports and contract hops from one file to another, and names every file \
                 and contract on the way. Call it to learn how two files are connected. When no chain joins them it \
                 says how many files the first one reaches."
            }
            Self::Contract => {
                "Shows who declares, serves and calls a contract, by role: an HTTP route, a GraphQL operation, a \
                 gRPC method, a table, a queue or an environment variable. Part of a key lists the keys that \
                 contain it. Call it to find who serves an endpoint and who calls it, before changing its shape."
            }
            Self::Coverage => {
                "Shows what the index could and could not read: for each connector the files it read, the files it \
                 failed on and the imports it could not resolve, and the code that no connector reads. Call it \
                 when an answer names a gap, or before relying on an answer for a language or a directory the \
                 index may not cover."
            }
            Self::IndexStatus => {
                "Shows how the stored index was built, from how many files and at which commit, and whether the \
                 tree has changed since. It does not refresh the index, so it answers at once. Call it to check \
                 how fresh the earlier answers are before asking again."
            }
        }
    }

    /// The schema of the arguments, which is the schema of the struct that reads them.
    fn schema(self) -> Result<Arc<JsonObject>, String> {
        let generated = match self {
            Self::Impact => schema_for_input::<ImpactArguments>(),
            Self::Dependents | Self::Dependencies => schema_for_input::<NeighbourArguments>(),
            Self::Path => schema_for_input::<RouteArguments>(),
            Self::Contract => schema_for_input::<ContractArguments>(),
            Self::Coverage => schema_for_input::<CoverageArguments>(),
            Self::IndexStatus => schema_for_input::<NoArguments>(),
        }?;
        let mut schema = JsonObject::clone(&generated);
        join_description_lines(&mut schema);
        Ok(Arc::new(schema))
    }

    /// The entry of `tools/list` for the tool.
    ///
    /// # Errors
    ///
    /// The reason the schema of the arguments could not be built, which a struct of named fields never gives.
    fn listed(self) -> Result<Tool, String> {
        // Every tool reads the repository and keeps its own cache; none changes the tree or reaches beyond it.
        let annotations = ToolAnnotations::new().read_only(true).open_world(false);
        Ok(Tool::new(self.name(), self.description(), self.schema()?)
            .with_title(self.title())
            .with_annotations(annotations))
    }
}

impl fmt::Display for ToolName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Joins the lines of each argument's description into one.
///
/// The doc comments that become the descriptions are wrapped for the people who read the source, and schemars keeps their line breaks, which would reach the model as a stray `\n` in the middle of a sentence.
fn join_description_lines(schema: &mut JsonObject) {
    let Some(Value::Object(properties)) = schema.get_mut("properties") else {
        return;
    };
    for property in properties.values_mut() {
        if let Some(Value::String(description)) = property.get_mut("description") {
            *description = description.replace('\n', " ");
        }
    }
}

/// The entries of `tools/list`, in the order of [`ToolName::ALL`].
///
/// # Errors
///
/// The reason the schema of some tool's arguments could not be built.
pub(crate) fn listing() -> Result<Vec<Tool>, String> {
    ToolName::ALL.into_iter().map(ToolName::listed).collect()
}
