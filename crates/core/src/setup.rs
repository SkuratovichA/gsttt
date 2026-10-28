//! What a config asks for. The config crate builds a [`Setup`] from `.gsttt/init.lua`; the engine runs it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::rules::Pack;

/// The modules to run with their options, the packs, and the files to leave out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Setup {
    /// Turns on every module whose `detect` finds something in the repository. A module in `modules` runs either
    /// way.
    pub detect: bool,
    /// Modules turned on with options, in the order the config lists them.
    pub modules: Vec<ModuleSetup>,
    /// Modules that stay off even when detected.
    pub disable: Vec<String>,
    /// Packs of the config, on top of the built-in ones.
    pub packs: Vec<Pack>,
    /// Built-in packs to leave out, by name.
    pub disable_packs: Vec<String>,
    /// Gitignore-style patterns of files to leave out, on top of what git ignores.
    pub exclude: Vec<String>,
    /// Files above this size are counted in coverage and not parsed.
    pub max_file_bytes: u64,
    /// Connectors that run as a command speaking JSON lines, for languages gsttt does not ship.
    pub externals: Vec<ExternalSetup>,
    /// Path prefixes a gateway or proxy puts in front of routes, like `/api`: with it, a call to `/api/bookings`
    /// meets the route `/bookings`.
    pub http_prefixes: Vec<String>,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            detect: true,
            modules: Vec::new(),
            disable: Vec::new(),
            packs: Vec::new(),
            disable_packs: Vec::new(),
            exclude: Vec::new(),
            max_file_bytes: 2 * 1024 * 1024,
            externals: Vec::new(),
            http_prefixes: Vec::new(),
        }
    }
}

/// A module turned on by the config: `g.ts { tsconfig = "tsconfig.base.json" }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleSetup {
    pub module: String,
    /// Checked against the module's own option schema when the engine opens it.
    #[serde(default)]
    pub options: serde_json::Value,
}

/// A connector that runs as a command: `g.external { name = "kotlin", cmd = { "gsttt-kotlin" }, files = { "**/*.kt" } }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExternalSetup {
    /// The name answers and configs use.
    pub name: String,
    /// The program and its arguments.
    pub cmd: Vec<String>,
    /// Globs of the files it reads.
    pub files: Vec<String>,
    /// Mixed into the cache key with the command, so a new release of the connector drops its cached facts.
    #[serde(default)]
    pub version: String,
}

impl Setup {
    pub fn module(&self, name: &str) -> Option<&ModuleSetup> {
        self.modules.iter().find(|module| module.module == name)
    }

    /// Whether the module `name` runs. `detected` runs the module's own detection, and only when it decides.
    pub fn runs(&self, name: &str, detected: impl FnOnce() -> bool) -> bool {
        !self.disable.iter().any(|off| off == name)
            && (self.module(name).is_some() || (self.detect && detected()))
    }

    /// The options of the module `name`; `null`, the defaults, when the config gives none.
    pub fn options(&self, name: &str) -> &serde_json::Value {
        self.module(name)
            .map_or(&serde_json::Value::Null, |module| &module.options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_modules_run_disabled_ones_never_and_detection_decides_the_rest() {
        let setup: Setup = serde_json::from_value(serde_json::json!({
            "modules": [{ "module": "ts", "options": { "tsconfig": "tsconfig.base.json" } }],
            "disable": ["openapi"],
        }))
        .unwrap();
        assert_eq!(setup.max_file_bytes, 2 * 1024 * 1024);
        assert!(setup.runs("ts", || false));
        assert!(!setup.runs("openapi", || true));
        assert!(setup.runs("python", || true) && !setup.runs("python", || false));
        assert_eq!(setup.options("ts")["tsconfig"], "tsconfig.base.json");
        assert!(setup.options("python").is_null());
        assert!(serde_json::from_value::<Setup>(serde_json::json!({ "modulez": [] })).is_err());
    }
}
