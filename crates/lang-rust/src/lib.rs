//! Rust for gsttt: a project per `Cargo.toml`, the module tree of every crate target, `use` paths followed through
//! modules, dependencies and workspace crates, and the contracts that Rust frameworks name.
//!
//! Syntax comes from rust-analyzer's parser, which never fails: a file with errors still yields its items. Nothing
//! here expands a macro or runs the compiler, so what a macro generates is a blind spot that [`Project::provides`]
//! names.
//!
//! [`Project::provides`]: gsttt_core::connector::Project::provides

mod eval;
mod imports;
mod manifest;
mod project;
mod resolve;
mod sites;
mod syntax;
mod tree;

use std::str::FromStr;
use std::sync::Arc;

use gsttt_core::connector::{Connector, ConnectorError, Erased, Meta};
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;
use ra_ap_edition::Edition;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use project::RustProject;

use crate::manifest::Manifests;
use crate::tree::Registry;

/// What `g.rust { … }` accepts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RustOptions {
    /// The edition to parse with when a package's manifest names none: `2015`, `2018`, `2021` or `2024`. Unset: the
    /// latest.
    pub default_edition: Option<String>,
}

impl RustOptions {
    fn edition(&self) -> Result<Edition, ConnectorError> {
        let Some(text) = self.default_edition.as_deref() else {
            return Ok(Edition::CURRENT);
        };
        Edition::from_str(text).map_err(|error| ConnectorError::Options {
            connector: "rust".to_owned(),
            message: format!(
                "`default_edition` is `{text}`: {error}; expected 2015, 2018, 2021 or 2024"
            ),
        })
    }
}

/// The Rust connector.
#[derive(Clone, Copy, Debug, Default)]
pub struct Rust;

/// The built-in connector, as the binary registers it.
pub static RUST: Erased<Rust> = Erased(Rust);

/// The built-in rule packs as `(file name, Lua source)` pairs, for the binary to evaluate with the config crate.
pub const PACKS: &[(&str, &str)] = &[];

impl Connector for Rust {
    type Options = RustOptions;
    type Project = RustProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "rust",
            version: 1,
            summary: "Rust: crates, module trees, use paths, and the contracts of axum, actix, reqwest, sqlx and sea-orm",
            extensions: &["rs"],
        }
    }

    fn detect(&self, repo: &RepoView) -> bool {
        repo.files()
            .any(|(_, path)| path.file_name() == "Cargo.toml")
    }

    fn open(
        &self,
        repo: &RepoView,
        options: &RustOptions,
        packs: &[Pack],
    ) -> Result<Vec<RustProject>, ConnectorError> {
        let default_edition = options.edition()?;
        let manifests = Manifests::read(repo);
        let registry = Arc::new(Registry::default());
        Ok(manifests
            .dirs()
            .map(|dir| {
                let package = manifests.package(repo, dir).unwrap_or_else(|message| {
                    // The project stays: its files are still read, only the crate structure is unknown.
                    tracing::warn!(dir, %message, "manifest left unread");
                    None
                });
                RustProject::new(
                    dir,
                    package,
                    default_edition,
                    packs,
                    repo,
                    Arc::clone(&registry),
                )
            })
            .collect())
    }
}
