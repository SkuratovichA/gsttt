//! What a config asks for. The config crate builds a [`Setup`] from `.gsttt/config.toml`; the engine runs it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::connector::Provide;
use crate::rules::Pack;

/// The modules to run with their options, the packs, and the files to leave out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Setup {
    /// Turns on every module whose `detect` finds something in the repository. A module in `modules` runs either way.
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
    /// Path prefixes a gateway or proxy puts in front of routes, like `/api`: with it, a call to `/api/bookings` meets the route `/bookings`.
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

/// A module turned on by the config: a `[ts]` table holding `tsconfig = "tsconfig.base.json"`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleSetup {
    pub module: String,
    /// Checked against the module's own option schema when the engine opens it.
    #[serde(default)]
    pub options: serde_json::Value,
}

/// A connector that runs as a command, one `[[external]]` entry of the config: `name = "kotlin"`, `cmd = ["gsttt-kotlin"]`, `files = ["**/*.kt"]`.
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
    /// Names of environment variables the process may read, on top of the few every program needs (`PATH`, `HOME`, `USER`, `LOGNAME`, `LANG`, `LC_ALL`, `LC_CTYPE`, `TMPDIR`). The rest of gsttt's environment, where secrets live, stays out of its reach.
    #[serde(default)]
    pub env: Vec<String>,
    /// What the connector finds and what it cannot see, as `[[external.provides]]` tables with `kind`, `role`, `via` and `blind_spots`. They are declared here and not asked of the process, because a run answered from the snapshot never starts it and must still report the same coverage as a run that did.
    #[serde(default)]
    pub provides: Vec<Provide>,
    /// The options of the connector: a table of named values, sent to the process in `open` and mixed into the cache key. `{}` when it has none.
    #[serde(default = "no_options")]
    // The reader refuses anything but a table, which `Value` alone would not tell an editor.
    #[schemars(with = "serde_json::Map<String, serde_json::Value>")]
    pub options: serde_json::Value,
}

/// The options of a connector that was given none.
fn no_options() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

/// Why a module runs or does not. The setup decides in this order: `disable` first, then a table of the module's own, then `detect`, and only then what the tree holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// The module is in `disable`.
    Disabled,
    /// The config has a table for the module.
    Configured,
    /// The config turns detection off, and has no table for the module.
    DetectionOff,
    /// The module finds files to read in the tree.
    Detected,
    /// The module finds nothing to read in the tree.
    NotDetected,
}

impl Reason {
    /// Whether a module runs for this reason.
    pub const fn runs(self) -> bool {
        matches!(self, Self::Configured | Self::Detected)
    }
}

impl Setup {
    pub fn module(&self, name: &str) -> Option<&ModuleSetup> {
        self.modules.iter().find(|module| module.module == name)
    }

    /// Why the module `name` runs or does not, in the order of [`Reason`]. `detected` runs the module's own detection, and only when the order reaches it.
    pub fn reason(&self, name: &str, detected: impl FnOnce() -> bool) -> Reason {
        if self.disable.iter().any(|off| off == name) {
            Reason::Disabled
        } else if self.module(name).is_some() {
            Reason::Configured
        } else if !self.detect {
            Reason::DetectionOff
        } else if detected() {
            Reason::Detected
        } else {
            Reason::NotDetected
        }
    }

    /// Whether the module `name` runs: what [`Setup::reason`] decides. `detected` runs the module's own detection, and only when the order reaches it.
    pub fn runs(&self, name: &str, detected: impl FnOnce() -> bool) -> bool {
        self.reason(name, detected).runs()
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

    /// The reason follows the order of the setup, whichever way the four facts it rests on fall; `runs` agrees with it; and the detection of the module is run only when the order reaches it.
    #[test]
    fn the_reason_follows_the_order_of_the_setup_and_runs_agrees() {
        for bits in 0..16_u8 {
            let [disabled, listed, detect, detected] =
                [0, 1, 2, 3].map(|bit| bits & (1 << bit) != 0);
            let setup = Setup {
                detect,
                disable: if disabled {
                    vec!["m".to_owned()]
                } else {
                    Vec::new()
                },
                modules: if listed {
                    vec![ModuleSetup {
                        module: "m".to_owned(),
                        options: serde_json::Value::Null,
                    }]
                } else {
                    Vec::new()
                },
                ..Setup::default()
            };
            let asked = std::cell::Cell::new(false);

            let reason = setup.reason("m", || {
                asked.set(true);
                detected
            });

            let expected = match (disabled, listed, detect, detected) {
                (true, ..) => Reason::Disabled,
                (false, true, ..) => Reason::Configured,
                (false, false, false, _) => Reason::DetectionOff,
                (false, false, true, true) => Reason::Detected,
                (false, false, true, false) => Reason::NotDetected,
            };
            let facts = format!(
                "disabled {disabled}, listed {listed}, detect {detect}, detected {detected}"
            );
            assert_eq!(reason, expected, "{facts}");
            assert_eq!(setup.runs("m", || detected), reason.runs(), "{facts}");
            assert_eq!(asked.get(), !disabled && !listed && detect, "{facts}");
        }
    }

    /// `config show --json` prints the reason under these names.
    #[test]
    fn the_reasons_are_named_in_snake_case() {
        let all = [
            Reason::Disabled,
            Reason::Configured,
            Reason::DetectionOff,
            Reason::Detected,
            Reason::NotDetected,
        ];
        assert_eq!(
            serde_json::to_string(&all).unwrap(),
            r#"["disabled","configured","detection_off","detected","not_detected"]"#
        );
    }
}
