//! What the link phase needs from a file besides its facts. It is cached with them, so every field is plain data.

use gsttt_core::fact::{ContractFact, Line};
use serde::{Deserialize, Serialize};

use crate::shape::Shape;

/// Per-file data for the link phase: the names a file takes from other files, what it exports, the routers it makes and mounts, the NestJS modules and controllers it declares, the tRPC routers it declares, the constants it exports, the clients and routers it makes and exports, and the sites that wait for other files. Empty parts are left out of the cached bytes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TsExtra {
    /// Names this file binds to the exports of other modules.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<Binding>,
    /// What other files can take from this one.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<Export>,
    /// The names mounts can attach, sorted and once each: variables made by an Express-like call, `@Module` classes, and classes that serve routes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<String>,
    /// Routes served by a host, waiting for the prefix its mounts give it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<Route>,
    /// `host.use(prefix, router)` calls, the lists of a `@Module`, and `app.setGlobalPrefix(prefix)` on a NestJS app.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<Mount>,
    /// Routes that NestJS classes declare, waiting for the prefixes of the modules that reach the class.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub class_routes: Vec<ClassRoute>,
    /// Controllers that extend a plain name, which may be a class that declares routes in another file.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub inherits: Vec<Inherit>,
    /// tRPC routers declared in this file.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub routers: Vec<TrpcRouter>,
    /// Variables a tRPC procedure call is stored in, which a router can list by name.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub procedures: Vec<String>,
    /// The top-level constants of this file whose first name it exports, by the name they are read under (`ROUTES.users.byId`), sorted by name, so that the link can read them for the files that import them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub constants: Vec<(String, String)>,
    /// The variables of this file made by a call that some call rule takes a receiver from, whose name the file exports as a local, sorted by name, so that the link can read a call on them in the files that import them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) makers: Vec<ExportedMaker>,
    /// The sites left to the link, in source order: those whose rule reads a name imported from another file, and those on a name imported from another file that a rule reads only on the receiver it was made as.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) deferred: Vec<Deferred>,
}

/// A variable made by a call that a call rule takes the receiver of its calls from: `export const api = axios.create({ baseURL: '/api' })`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ExportedMaker {
    /// The name it is declared under in this file, `default` for `export default axios.create()`.
    pub(crate) name: String,
    /// The callee of the call that made it, as written: `axios.create`.
    pub(crate) callee: String,
    /// The call as a rule reads it, for the prefix the rule takes from it.
    pub(crate) shape: Shape,
}

/// A call or a decorator that the parse left to the link: a rule could not read it because it names a constant of another file, or it is a call on a client or a router that another file makes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Deferred {
    /// The rule that read the site, by its place in the packs.
    pub(crate) rule: RuleRef,
    /// The site, with the names the link reads.
    pub(crate) shape: Shape,
    /// The prefix the rule was given, as the parse read it.
    pub(crate) prefix: Option<String>,
    pub(crate) line: Line,
    /// The callee or decorator name the rule is applied with.
    pub(crate) what: String,
    /// Where a contract the rule finds goes.
    pub(crate) target: DeferredTarget,
}

/// The place of a rule in the packs the parse and the link are both given: the index of its pack and of the rule among the pack's rules of that kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RuleRef {
    Call { pack: usize, rule: usize },
    Decorator { pack: usize, rule: usize },
}

/// What a contract found for a deferred site becomes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DeferredTarget {
    /// A contract of the file, as any other.
    Own,
    /// A route served by a variable of this file, which waits for the prefixes its mounts give it.
    Host(String),
    /// A route that a decorator on a member of a class declares.
    Class { class: String, inherited: bool },
    /// A call on a name imported from another file, read on the receiver that file makes under the name, whose rule needs one.
    Receiver { host: String },
    /// A call on a name imported from another file, read as a rule without a receiver reads it, unless the name is a receiver the rules in `rivals` read it on.
    Fallback { host: String, rivals: Vec<RuleRef> },
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

/// A route that a decorator on a member of a class declares.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassRoute {
    /// The class that declares it.
    pub class: String,
    /// The route as the rules found it: behind the prefix of the class decorator when the class carries one, and with no prefix when it does not.
    pub fact: ContractFact,
    /// The class carries no decorator that gives a prefix, so the route is served only by a controller that extends the class, and the parse records no fact for it.
    pub inherited: bool,
}

/// A controller that extends a plain name: `@Controller('users') class UserController extends UserControllerBase`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inherit {
    /// The controller.
    pub class: String,
    /// The name it extends, as written in the file.
    pub base: String,
    /// The prefix its class decorator gives.
    pub prefix: String,
    /// The line of the class decorator, where the routes it inherits are served in this file.
    pub line: Line,
}

/// A `host.use(prefix, targets…)` call, a `@Module` decorator, or an `app.setGlobalPrefix(prefix)` call.
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
    /// A name in the file: a router, module or controller made here or imported.
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
    /// A router, a procedure or anything else named by a variable; the line is where a procedure is reported.
    Ref {
        name: String,
        local: String,
        line: Line,
    },
}
