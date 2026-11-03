//! Go for gsttt: one project per `go.mod`, imports read with tree-sitter and resolved against the module paths of the repository, the declarations of each file with the names they use, and the contracts of the environment, `net/http` and `database/sql` found by rule packs, and the routes of gin, chi, echo, gorilla and fiber.

mod analyze;
mod code;
mod file;
mod files;
mod interfaces;
mod manifest;
mod names;
mod parse;
mod project;
mod provides;
mod resolve;
mod routers;
mod scope;
mod site;
mod syntax;
mod value;
mod walk;

use gsttt_core::connector::{Connector, ConnectorError, Erased, Meta};
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use project::GoProject;

/// The rule packs shipped with the connector, as `(name, TOML source)` pairs. The name is the file name without `.toml`, which is also the `name` the pack declares.
pub const PACKS: &[(&str, &str)] = &[
    ("chi", include_str!("../packs/chi.toml")),
    ("database-sql", include_str!("../packs/database-sql.toml")),
    ("echo", include_str!("../packs/echo.toml")),
    ("env", include_str!("../packs/env.toml")),
    ("fiber", include_str!("../packs/fiber.toml")),
    ("gin", include_str!("../packs/gin.toml")),
    ("gorilla", include_str!("../packs/gorilla.toml")),
    ("net-http", include_str!("../packs/net-http.toml")),
];

/// The Go connector, ready for the registry.
pub static GO: Erased<Go> = Erased(Go);

/// What the `[go]` table of the configuration accepts: nothing yet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct GoOptions {}

/// Reads Go: `.go` files and `go.mod`, grouped into projects by `go.mod`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Go;

impl Connector for Go {
    type Options = GoOptions;
    type Project = GoProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "go",
            version: 1,
            summary: "Go imports and packages, with the declarations each file uses, and environment, net/http and database/sql contracts",
            extensions: &["go"],
        }
    }

    fn detect(&self, repo: &RepoView) -> bool {
        repo.files().any(|(_, path)| files::is_manifest(path))
    }

    fn open(
        &self,
        repo: &RepoView,
        _options: &GoOptions,
        packs: &[Pack],
    ) -> Result<Vec<GoProject>, ConnectorError> {
        Ok(project::open_projects(repo, packs))
    }
}
