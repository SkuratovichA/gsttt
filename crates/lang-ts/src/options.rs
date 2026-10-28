//! What the `ts` connector can be told in the configuration.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Options of the `ts` connector. Every field has a default, so `{}` and no options at all mean the same.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct TypeScriptOptions {
    /// The tsconfig to read for files that no `tsconfig.json` above them governs, as a path from the repository root.
    pub tsconfig: Option<String>,
    /// Export conditions to accept besides `types`, `import`, `module`, `node`, `default` and `require`: a package that names its sources under a custom condition (`source`, `development`) is then resolved to them.
    pub conditions: Vec<String>,
}
