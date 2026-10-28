//! Python for gsttt: one project per manifest directory, imports read with the ruff parser and resolved against
//! source roots, `__init__.py` packages, namespace packages and relative imports, and the contracts of FastAPI,
//! Flask, Django, requests, httpx, SQLAlchemy, Alembic and the environment found by rule packs.

mod extra;
mod files;
mod link;
mod manifest;
mod names;
mod parse;
mod project;
mod provides;
mod resolve;
mod site;

use gsttt_core::connector::{Connector, ConnectorError, Erased, Meta};
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use extra::PyExtra;
pub use project::PythonProject;

/// The Lua sources of the built-in packs, `(file name, source)`.
pub const PACKS: &[(&str, &str)] = &[
    ("alembic.lua", include_str!("../packs/alembic.lua")),
    ("dbapi.lua", include_str!("../packs/dbapi.lua")),
    ("django.lua", include_str!("../packs/django.lua")),
    ("env.lua", include_str!("../packs/env.lua")),
    ("fastapi.lua", include_str!("../packs/fastapi.lua")),
    ("flask.lua", include_str!("../packs/flask.lua")),
    ("httpx.lua", include_str!("../packs/httpx.lua")),
    ("requests.lua", include_str!("../packs/requests.lua")),
    ("sqlalchemy.lua", include_str!("../packs/sqlalchemy.lua")),
];

/// The Python connector, ready for the registry.
pub static PYTHON: Erased<Python> = Erased(Python);

/// The options of `g.python{ … }`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PythonOptions {
    /// Directories modules are imported from, relative to each project's directory. They come on top of the
    /// project's own directory, `src` when it exists and the directories its package settings name.
    pub source_roots: Vec<String>,
}

/// Reads Python: `.py` and `.pyi` files, grouped into projects by `pyproject.toml`, `setup.py` and `setup.cfg`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Python;

impl Connector for Python {
    type Options = PythonOptions;
    type Project = PythonProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "python",
            version: 1,
            summary: "Python imports and packages, with FastAPI, Flask, Django, requests, httpx, SQLAlchemy and Alembic contracts",
            extensions: &["py", "pyi"],
        }
    }

    fn detect(&self, repo: &RepoView) -> bool {
        repo.files().any(|(_, path)| {
            files::is_python_source(path) || files::PROJECT_MANIFESTS.contains(&path.file_name())
        })
    }

    fn open(
        &self,
        repo: &RepoView,
        options: &PythonOptions,
        packs: &[Pack],
    ) -> Result<Vec<PythonProject>, ConnectorError> {
        project::open_projects(repo, options, packs)
    }
}
