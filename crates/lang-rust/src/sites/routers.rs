//! The routers of a file, for the link step: what serves routes together, and which other routers it mounts.
//!
//! A chain of method calls is one group, whether it is `Router::new().route(..).nest(..)`, a `web::scope("/a")` with services, or a `web::resource("/r")` with routes, and so is a function that an attribute gives a path to. A mount is a `nest`, `merge` or `service` call; what it mounts is a group of this file, or a function whose routers the link step looks up by name, because the router may be built in another file. A router bound with `let` is followed to what it was bound to, so no group needs a name.

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::contract::Role;
use gsttt_core::fact::{ContractFact, UnresolvedReason, UnresolvedSite};
use ra_ap_syntax::ast::{self, HasArgList, HasName};
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode};

use super::candidate::{Candidate, callee_segments, chain_top, peel};
use super::{Found, Reader, Sites};
use crate::eval::text_of;
use crate::link::{Extra, Function, Group, Mount, Prefix, Target};
use crate::syntax::{segments, text};

/// The words in a function's return type that say it builds a router.
const ROUTER_TYPES: [&str; 3] = ["Router", "Scope", "Resource"];

/// How a method call puts another router into the one it is called on.
#[derive(Clone, Copy)]
enum Mounting {
    /// `nest("/prefix", router)`.
    Nest,
    /// `merge(router)`, `service(router)`.
    Plain,
}

/// A run of method calls, the way a router is built: `Router::new().route(..).nest(..)`.
struct Chain {
    anchor: u32,
    /// The calls in the order they are written.
    calls: Vec<ast::MethodCallExpr>,
    /// What the first call is made on.
    root: ast::Expr,
}

impl Chain {
    /// The chain that `node` is the outermost call of, if it is one.
    fn of(node: SyntaxNode) -> Option<Self> {
        if node.kind() != SyntaxKind::METHOD_CALL_EXPR || chain_top(&node) != node {
            return None;
        }
        let anchor = anchor_of(&node);
        let mut calls = vec![ast::MethodCallExpr::cast(node)?];
        let root = loop {
            match calls.last()?.receiver()? {
                ast::Expr::MethodCallExpr(call) => calls.push(call),
                other => break other,
            }
        };
        calls.reverse();
        Some(Self {
            anchor,
            calls,
            root,
        })
    }
}

impl Reader<'_> {
    /// Records the groups and the functions of the file that the link step reads.
    pub(super) fn routers(&self, root: &SyntaxNode, sites: &mut Sites) {
        if !self.axum && !self.actix {
            return;
        }
        let mut routes = routes(&sites.found);
        let chains: Vec<Chain> = root
            .descendants()
            .filter_map(Chain::of)
            .filter(|chain| {
                routes.contains_key(&chain.anchor)
                    || chain.calls.iter().any(|call| self.mounting(call).is_some())
            })
            .collect();
        let functions: Vec<ast::Fn> = root.descendants().filter_map(ast::Fn::cast).collect();
        let handlers: Vec<&ast::Fn> = functions
            .iter()
            .filter(|function| routes.contains_key(&anchor_of(function.syntax())))
            .collect();
        let anchors: BTreeSet<u32> = chains
            .iter()
            .map(|chain| chain.anchor)
            .chain(handlers.iter().map(|handler| anchor_of(handler.syntax())))
            .collect();

        let mut groups: BTreeMap<u32, Group> = BTreeMap::new();
        for chain in &chains {
            let routes = routes.remove(&chain.anchor).unwrap_or_default();
            let group = self.group(chain, routes, &anchors, sites);
            groups.insert(chain.anchor, group);
        }
        for handler in handlers {
            let anchor = anchor_of(handler.syntax());
            groups.insert(
                anchor,
                Group {
                    anchor,
                    base: Prefix::none(),
                    routes: routes.remove(&anchor).unwrap_or_default(),
                    mounts: Vec::new(),
                },
            );
        }

        sites.extra = Extra {
            functions: functions
                .iter()
                .filter_map(|function| returned_by(function, &groups, &anchors))
                .collect(),
            groups: groups.into_values().collect(),
        };
    }

    /// The group of one chain, with the routers it mounts.
    fn group(
        &self,
        chain: &Chain,
        routes: Vec<ContractFact>,
        anchors: &BTreeSet<u32>,
        sites: &mut Sites,
    ) -> Group {
        let scope = self.scope(&chain.root, sites);
        let mut mounts = Vec::new();
        // A chain that starts from an existing router adds to it, as `app.route(..)` does after `let app = ..`.
        if scope.is_none() && !is_constructor(&chain.root) {
            mounts.extend(target_of(&chain.root, anchors).map(|target| Mount {
                prefix: Prefix::none(),
                target,
            }));
        }
        for call in &chain.calls {
            let Some(mounting) = self.mounting(call) else {
                continue;
            };
            let arguments: Vec<ast::Expr> = call
                .arg_list()
                .map(|list| list.args().collect())
                .unwrap_or_default();
            let (prefix, router) = match (mounting, arguments.as_slice()) {
                (Mounting::Nest, [prefix, router]) => {
                    (self.prefix(call.syntax(), prefix, sites), router)
                }
                (Mounting::Plain, [router]) => (Prefix::none(), router),
                _ => continue,
            };
            mounts.extend(target_of(router, anchors).map(|target| Mount { prefix, target }));
        }
        Group {
            anchor: chain.anchor,
            base: scope.unwrap_or_else(Prefix::none),
            routes,
            mounts,
        }
    }

    /// How `call` mounts a router, if it does with the frameworks in force.
    fn mounting(&self, call: &ast::MethodCallExpr) -> Option<Mounting> {
        match text(&call.name_ref()?).as_str() {
            "nest" if self.axum => Some(Mounting::Nest),
            "merge" if self.axum => Some(Mounting::Plain),
            "service" if self.actix => Some(Mounting::Plain),
            _ => None,
        }
    }

    /// What `web::scope("/prefix")` puts in front of a chain that starts with it; `None` for any other start.
    fn scope(&self, root: &ast::Expr, sites: &mut Sites) -> Option<Prefix> {
        let ast::Expr::CallExpr(call) = root else {
            return None;
        };
        if !self.actix || callee_segments(call)?.last().map(String::as_str) != Some("scope") {
            return None;
        }
        let first = call.arg_list()?.args().next()?;
        Some(self.prefix(call.syntax(), &first, sites))
    }

    /// A prefix written at `call`. One that is not text is a site the connector cannot read.
    fn prefix(&self, call: &SyntaxNode, written: &ast::Expr, sites: &mut Sites) -> Prefix {
        if let Some(prefix) = text_of(written, false, self.edition) {
            return Prefix::Literal(prefix);
        }
        if let Some(site) = Candidate::of_call(call, self.lines) {
            sites.unresolved.push(UnresolvedSite {
                line: site.line,
                reason: UnresolvedReason::NonLiteral,
                detail: site.callee,
            });
        }
        Prefix::Computed
    }
}

/// The routes of a file by the chain or handler that serves them.
fn routes(found: &[Found]) -> BTreeMap<u32, Vec<ContractFact>> {
    let mut routes: BTreeMap<u32, Vec<ContractFact>> = BTreeMap::new();
    for site in found
        .iter()
        .filter(|site| site.fact.role == Role::Serves && site.fact.key.http_parts().is_some())
    {
        routes
            .entry(site.anchor)
            .or_default()
            .push(site.fact.clone());
    }
    for group in routes.values_mut() {
        group.sort_by(|a, b| (a.line, &a.key, &a.via).cmp(&(b.line, &b.key, &b.via)));
        group.dedup();
    }
    routes
}

/// The routers a function hands back: the group its attribute makes, and what its last expression stands for. A function that only forwards another's call is followed when its signature says it returns a router.
fn returned_by(
    function: &ast::Fn,
    groups: &BTreeMap<u32, Group>,
    anchors: &BTreeSet<u32>,
) -> Option<Function> {
    let name = text(&function.name()?);
    let mut returns = Vec::new();
    let anchor = anchor_of(function.syntax());
    if groups.contains_key(&anchor) {
        returns.push(Target::Local(anchor));
    }
    let last = function.body().and_then(|body| body.tail_expr());
    returns.extend(
        last.and_then(|last| target_of(&last, anchors))
            .filter(|target| matches!(target, Target::Local(_)) || builds_router(function)),
    );
    (!returns.is_empty()).then_some(Function { name, returns })
}

/// Whether the signature of a function says it returns a router.
fn builds_router(function: &ast::Fn) -> bool {
    function.ret_type().is_some_and(|returned| {
        let returned = text(&returned);
        ROUTER_TYPES.iter().any(|word| returned.contains(word))
    })
}

/// The router an expression stands for, following `let` bindings in the function around it.
fn target_of(expr: &ast::Expr, anchors: &BTreeSet<u32>) -> Option<Target> {
    match peel(expr)? {
        ast::Expr::MethodCallExpr(call) => {
            let anchor = anchor_of(call.syntax());
            if anchors.contains(&anchor) {
                return Some(Target::Local(anchor));
            }
            // `bookings::router().layer(auth)` is the router it layers.
            target_of(&call.receiver()?, anchors)
        }
        ast::Expr::CallExpr(call) => {
            let names = callee_segments(&call)?;
            match (names.as_slice(), call.arg_list()?.args().next()) {
                ([wrapper], Some(inner)) if wrapper == "Ok" || wrapper == "Some" => {
                    target_of(&inner, anchors)
                }
                _ => Some(Target::Path(names)),
            }
        }
        ast::Expr::PathExpr(path) => {
            let names = segments(&path.path()?)?;
            if let [name] = names.as_slice()
                && let Some(bound) = binding(path.syntax(), name)
            {
                return target_of(&bound, anchors);
            }
            Some(Target::Path(names))
        }
        _ => None,
    }
}

/// What the nearest `let name = ..` written before `at`, in the function around it, binds the name to.
fn binding(at: &SyntaxNode, name: &str) -> Option<ast::Expr> {
    let function = at.ancestors().find_map(ast::Fn::cast)?;
    let before = at.text_range().start();
    function
        .syntax()
        .descendants()
        .filter_map(ast::LetStmt::cast)
        .filter(|statement| statement.syntax().text_range().end() <= before)
        .filter(|statement| {
            matches!(
                statement.pat(),
                Some(ast::Pat::IdentPat(pattern))
                    if pattern.name().is_some_and(|bound| text(&bound) == name)
            )
        })
        .last()?
        .initializer()
}

/// Whether a chain starts by making a router, rather than by naming one that exists.
fn is_constructor(root: &ast::Expr) -> bool {
    let ast::Expr::CallExpr(call) = root else {
        return false;
    };
    callee_segments(call)
        .is_some_and(|names| matches!(names.last().map(String::as_str), Some("new" | "default")))
}

/// Where a node starts in the file.
fn anchor_of(node: &SyntaxNode) -> u32 {
    u32::from(node.text_range().start())
}
