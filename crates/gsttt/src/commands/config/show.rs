//! `gsttt config show`: the setup a run would use, as the config and the tree make it.

use std::fmt;

use gsttt_core::Setup;
use gsttt_core::engine::packs::merge_packs;
use gsttt_core::query::{Answer, Entries};
use serde::Serialize;
use serde_json::Value;

use crate::commands::{Session, print};
use crate::error::CliError;
use crate::repository::{Repository, RepositoryError};
use crate::streams::{Io, Outcome};

/// `gsttt config show`: prints the modules with whether each is on and why, the options the config gives them, the
/// packs they receive, the external connectors with whether the user trusts each, and the settings that apply to
/// the whole run. Under `--json` it is one JSON document with the same parts.
pub(in crate::commands) fn show(
    session: &Session<'_>,
    io: &mut Io<'_>,
) -> Result<Outcome, CliError> {
    let shown = Shown::of(&session.repository)?;
    print::answer(&shown, session.flags, io)
}

/// Why a module is on or off. The config decides in this order: `disable` first, then a table of its own for the
/// module, then `detect`, and only then what the tree holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Why {
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

impl Why {
    /// What [`Setup::runs`] decides, with the reason: the two are pinned against each other by a test.
    fn of(setup: &Setup, name: &str, detected: bool) -> Self {
        if setup.disable.iter().any(|off| off == name) {
            Self::Disabled
        } else if setup.module(name).is_some() {
            Self::Configured
        } else if !setup.detect {
            Self::DetectionOff
        } else if detected {
            Self::Detected
        } else {
            Self::NotDetected
        }
    }

    const fn is_on(self) -> bool {
        matches!(self, Self::Configured | Self::Detected)
    }

    const fn words(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Configured => "in the config",
            Self::DetectionOff => "detection is off",
            Self::Detected => "detected",
            Self::NotDetected => "not detected",
        }
    }
}

/// Where a pack that a module receives comes from.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Origin {
    /// It ships with gsttt.
    BuiltIn,
    /// The config, or a file in `.gsttt/packs/`, wrote it, and it replaces a built-in pack of the same name.
    Config,
}

impl Origin {
    const fn words(self) -> &'static str {
        match self {
            Self::BuiltIn => "built-in",
            Self::Config => "config",
        }
    }
}

/// A module of the registry.
#[derive(Debug, Serialize)]
struct ModuleLine {
    name: &'static str,
    on: bool,
    why: Why,
    /// The options as the config wrote them; `null` when it gives none and the module runs on its defaults.
    options: Value,
}

/// A pack that a module which is on receives.
#[derive(Debug, Serialize)]
struct PackLine {
    name: String,
    language: String,
    origin: Origin,
}

/// An `[[external]]` of the config.
#[derive(Debug, Serialize)]
struct ExternalLine {
    name: String,
    trusted: bool,
    cmd: Vec<String>,
    files: Vec<String>,
    /// `cmd` as a shell would take it, for the text.
    #[serde(skip)]
    command: String,
}

/// The setup of a run as a document.
#[derive(Debug, Serialize)]
struct Shown {
    root: String,
    modules: Vec<ModuleLine>,
    packs: Vec<PackLine>,
    externals: Vec<ExternalLine>,
    detect: bool,
    exclude: Vec<String>,
    max_file_bytes: u64,
    http_prefixes: Vec<String>,
}

impl Shown {
    fn of(repository: &Repository) -> Result<Self, RepositoryError> {
        let setup = repository.setup();
        let registry = repository.registry();
        let detected = repository.detected()?;
        let mut modules = Vec::new();
        let mut packs = Vec::new();
        for connector in registry.connectors() {
            let name = connector.meta().name;
            let why = Why::of(setup, name, detected.contains(&name));
            if why.is_on() {
                let received = merge_packs(registry.packs(), setup, name);
                packs.extend(received.into_iter().map(|pack| PackLine {
                    origin: origin_of(setup, &pack.language, &pack.name),
                    name: pack.name,
                    language: pack.language,
                }));
            }
            modules.push(ModuleLine {
                name,
                on: why.is_on(),
                why,
                options: setup.options(name).clone(),
            });
        }
        let externals = repository
            .externals()
            .iter()
            .map(|external| ExternalLine {
                name: external.name().to_owned(),
                trusted: external.trusted(),
                cmd: external.setup().cmd.clone(),
                files: external.setup().files.clone(),
                command: external.command(),
            })
            .collect();
        Ok(Self {
            root: repository.root().to_string(),
            modules,
            packs,
            externals,
            detect: setup.detect,
            exclude: setup.exclude.clone(),
            max_file_bytes: setup.max_file_bytes,
            http_prefixes: setup.http_prefixes.clone(),
        })
    }

    /// The text, a line at a time: a heading for each list with its entries two spaces in, and `(none)` where a list
    /// is empty.
    fn lines(&self) -> Vec<String> {
        let mut lines = vec![format!("root: {}", self.root), "modules:".to_owned()];
        lines.extend(self.modules.iter().map(ModuleLine::line));
        lines.push("packs:".to_owned());
        if self.packs.is_empty() {
            lines.push(NONE_ENTRY.to_owned());
        }
        lines.extend(self.packs.iter().map(|pack| {
            format!(
                "  {} ({}, {})",
                pack.name,
                pack.language,
                pack.origin.words()
            )
        }));
        lines.push("externals:".to_owned());
        if self.externals.is_empty() {
            lines.push(NONE_ENTRY.to_owned());
        }
        lines.extend(self.externals.iter().map(|external| {
            let trust = if external.trusted {
                "trusted"
            } else {
                "not trusted"
            };
            format!("  {}: {trust} ({})", external.name, external.command)
        }));
        lines.push(format!("detect: {}", self.detect));
        lines.push(format!("exclude: {}", list(&self.exclude)));
        lines.push(format!("max_file_bytes: {}", self.max_file_bytes));
        lines.push(format!("http_prefixes: {}", list(&self.http_prefixes)));
        lines
    }
}

/// The entry of a list that has none.
const NONE_ENTRY: &str = "  (none)";

impl ModuleLine {
    /// `  ts: on (detected)`, and after it `, options {"tsconfig":"tsconfig.base.json"}` when the config gives some.
    fn line(&self) -> String {
        let state = if self.on { "on" } else { "off" };
        let line = format!("  {}: {state} ({})", self.name, self.why.words());
        let none = self.options.is_null()
            || self
                .options
                .as_object()
                .is_some_and(serde_json::Map::is_empty);
        if none {
            line
        } else {
            format!("{line}, options {}", self.options)
        }
    }
}

/// Whether the config or the built-in packs wrote the pack `name` of `language` that a module receives. A pack of
/// the setup with that name and language is the one received: it replaced the built-in pack, or was added.
fn origin_of(setup: &Setup, language: &str, name: &str) -> Origin {
    if setup
        .packs
        .iter()
        .any(|own| own.language == language && own.name == name)
    {
        Origin::Config
    } else {
        Origin::BuiltIn
    }
}

/// The values of a list setting on one line, or `(none)`.
fn list(values: &[String]) -> String {
    if values.is_empty() {
        "(none)".to_owned()
    } else {
        values.join(", ")
    }
}

impl Answer for Shown {
    fn write_text(&self, f: &mut fmt::Formatter<'_>, _entries: Entries) -> fmt::Result {
        f.write_str(&self.lines().join("\n"))
    }
}

impl fmt::Display for Shown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_text(f, Entries::Capped)
    }
}

#[cfg(test)]
mod tests {
    use gsttt_core::setup::ModuleSetup;

    use super::*;

    /// `Why` is the reason for what [`Setup::runs`] decides, whichever way the four facts it depends on fall.
    #[test]
    fn the_reason_agrees_with_what_the_setup_decides() {
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
                        options: Value::Null,
                    }]
                } else {
                    Vec::new()
                },
                ..Setup::default()
            };
            assert_eq!(
                Why::of(&setup, "m", detected).is_on(),
                setup.runs("m", || detected),
                "disabled {disabled}, listed {listed}, detect {detect}, detected {detected}"
            );
        }
    }

    #[test]
    fn a_module_line_shows_options_only_when_the_config_gives_some() {
        let line = |options: Value| {
            ModuleLine {
                name: "ts",
                on: true,
                why: Why::Configured,
                options,
            }
            .line()
        };
        assert_eq!(line(Value::Null), "  ts: on (in the config)");
        assert_eq!(line(serde_json::json!({})), "  ts: on (in the config)");
        assert_eq!(
            line(serde_json::json!({ "tsconfig": "a.json" })),
            r#"  ts: on (in the config), options {"tsconfig":"a.json"}"#
        );
    }
}
