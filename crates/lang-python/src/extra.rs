//! The per-file data the link step needs, cached with the facts: which names hold a router, which route facts
//! sit under one, where routers are mounted and what the file's imports bound.

use gsttt_core::fact::Line;
use serde::{Deserialize, Serialize};

/// What `parse` keeps for `link`. Empty vectors are left out of the cache entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PyExtra {
    /// Names bound to a router, blueprint or application in this file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) scopes: Vec<Scope>,
    /// Route facts of this file and the router each sits under.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) routes: Vec<Route>,
    /// Routers and URL modules this file mounts under a prefix.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) mounts: Vec<Mount>,
    /// Local names the file's imports bind; kept only in a file that mounts something.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) binds: Vec<Bind>,
}

/// A name bound to a router: `router = APIRouter(prefix="/bookings")`. Django's `urlpatterns` module is the
/// scope with the empty name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Scope {
    pub(crate) var: String,
    /// The prefix the router was made with, as written.
    pub(crate) prefix: String,
}

/// A route fact, found by its line, and the scope it sits under.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Route {
    pub(crate) var: String,
    pub(crate) line: Line,
}

/// `app.include_router(bookings.router, prefix="/api")`, `app.register_blueprint(bp, url_prefix="/api")` or
/// Django's `path("api/", include("bookings.urls"))`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Mount {
    pub(crate) line: Line,
    /// The router the mount is called on, `app` in `app.include_router(…)`; empty for Django.
    pub(crate) receiver: String,
    pub(crate) target: Target,
    /// The prefix as written; empty when there is none or when it is not a literal.
    pub(crate) prefix: String,
}

/// What a mount points at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Target {
    /// A module named by a string, `include("bookings.urls")`; the file's import of it holds the resolution.
    Module(String),
    /// A router named by an expression: `router`, `bookings.router`.
    Name(String),
}

/// A name an import binds: `from app.api import bookings as b` binds `b` to `bookings` of `app.api`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Bind {
    pub(crate) local: String,
    /// The specifier of the import, as written, which together with the line finds the import's resolution.
    pub(crate) spec: String,
    pub(crate) line: Line,
    /// The name taken from the module in a `from` import.
    pub(crate) name: Option<String>,
}
