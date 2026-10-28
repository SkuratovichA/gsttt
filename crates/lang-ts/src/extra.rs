//! What the link phase needs from a file besides its facts. It is cached with them, so every field is plain data.

use gsttt_core::fact::{ContractFact, Line};
use serde::{Deserialize, Serialize};

/// Per-file data for the link phase: the names a file takes from other files, what it exports, the routers it makes and
/// mounts, and the tRPC routers it declares. Empty parts are left out of the cached bytes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TsExtra {
    /// Names this file binds to the exports of other modules.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<Binding>,
    /// What other files can take from this one.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<Export>,
    /// Variables made by an Express-like call, the nodes mounts connect.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<String>,
    /// Routes served by a host, waiting for the prefix its mounts give it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<Route>,
    /// `host.use(prefix, router)` calls.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<Mount>,
    /// tRPC routers declared in this file.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub routers: Vec<TrpcRouter>,
}

/// A local name bound to something another module exports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    /// The name in this file.
    pub local: String,
    /// The specifier of the module, as written.
    pub spec: String,
    /// The exported name taken: `default`, a named export, or `*` for a namespace.
    pub name: String,
}

/// One export of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Export {
    /// The exported name: `default`, a named export, or `*` for `export * from`.
    pub name: String,
    pub target: ExportTarget,
}

/// What an export stands for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportTarget {
    /// A name declared or bound in the same file.
    Local(String),
    /// A re-export: the name taken from another module.
    From { spec: String, name: String },
}

/// A route a host serves, as the rules found it with no prefix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    /// The variable the route was registered on.
    pub host: String,
    pub fact: ContractFact,
}

/// A `host.use(prefix, targets…)` call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mount {
    pub host: String,
    /// The path the targets are mounted under, `""` for none; a computed part is written `${}`.
    pub prefix: String,
    pub targets: Vec<MountTarget>,
    pub line: Line,
}

/// What a mount attaches to its host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MountTarget {
    /// A name in the file: a router made here or imported.
    Local(String),
    /// `require('./router')` written in place.
    Module(String),
}

/// A tRPC router built from an object literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrpcRouter {
    /// The name it is declared under, `default` for the default export, or `#<line>` when it has none.
    pub id: String,
    pub entries: Vec<TrpcEntry>,
}

/// One property of a tRPC router.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrpcEntry {
    /// A procedure: its property name and the line of the property.
    Procedure { name: String, line: Line },
    /// A router written in place: the id it is stored under.
    Nested { name: String, id: String },
    /// A router (or anything else) named by a variable.
    Ref { name: String, local: String },
}
