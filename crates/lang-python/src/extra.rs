//! The per-file data the link step needs, cached with the facts: the routes declared on routers, the prefixes routers declare for themselves, where routers are mounted and what the file's imports bound.

use gsttt_core::fact::{ContractFact, Line};
use serde::{Deserialize, Serialize};

/// What `parse` keeps for `link`. Empty vectors are left out of the cache entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PyExtra {
    /// The routes declared on a router object, `APIRouter` or `Blueprint`, and on a Django URL module. They are not facts of the file: `link` serves each of them behind the prefixes of the mounts above its router.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) routes: Vec<Route>,
    /// The prefix each router of this file declares for itself, when it declares one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) routers: Vec<RouterPrefix>,
    /// Routers and URL modules this file mounts under a prefix.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) mounts: Vec<Mount>,
    /// Local names the file's imports bind; kept in a file that mounts something and in a package's `__init__`, which may import a router from one of its modules and give it another name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) binds: Vec<Bind>,
}

/// A route fact and the router it sits under. Django's `urlpatterns` list is the module itself and has the empty name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Route {
    pub(crate) var: String,
    /// The fact as the route's own decorator or call reads it: the router's own prefix is not in it, because whether that prefix applies depends on how the router is mounted.
    pub(crate) fact: ContractFact,
}

/// The prefix a router variable declares in its own constructor, `APIRouter(prefix="/bookings")` or `Blueprint("b", __name__, url_prefix="/bookings")`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RouterPrefix {
    pub(crate) var: String,
    pub(crate) prefix: String,
}

/// `app.include_router(bookings.router, prefix="/api")`, `app.register_blueprint(bp, url_prefix="/api")` or Django's `path("api/", include("bookings.urls"))`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Mount {
    pub(crate) line: Line,
    /// The router the mount is called on, `app` in `app.include_router(…)`; empty for Django.
    pub(crate) receiver: String,
    pub(crate) target: Target,
    /// The prefix as written; `None` when there is none or when it is not a literal, which `unread_prefix` tells apart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) prefix: Option<String>,
    /// Whether the call gives a prefix that is not a literal, so that the path the mounted router is served at is not known.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) unread_prefix: bool,
    /// Whether the prefix replaces the mounted router's own, as Flask's `register_blueprint(url_prefix=…)` does, or comes before it, as FastAPI's `include_router(prefix=…)` and Django's `include` do.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) replaces: bool,
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
