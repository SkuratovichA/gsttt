//! A site in the syntax tree and how the rules read it.
//!
//! The rules read dotted callees, so every site gets a dotted name. A call is its path (`web::scope(..)` is `web.scope`), a macro is its path with a `!` (`std::env!` is `std.env!`), an attribute is its path, and a method call is the last name of its receiver and the method: `self.http.get(..)` is `http.get`. A receiver that has no name, such as another call, is `_`, which a `*` pattern still matches.
//!
//! Rust has no keyword arguments or object literals, so the value expressions of a rule read them like this: `$keyword` is a `name = value` among the arguments of an attribute or a macro, `$argN.method` is the HTTP verbs of the method router passed as argument `N` (`get(show).delete(destroy)` has GET and DELETE, `web::get().to(show)` has GET), and `$argN.last` is the last segment of the path passed as argument `N` (`Method::POST` is `POST`).

use std::borrow::Cow;
use std::num::NonZeroU32;

use gsttt_core::fact::Line;
use gsttt_core::rules::{CallRule, Outcome, Site, ValueExpr, ValueSource};
use gsttt_core::text::LineIndex;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasArgList};
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode};

use crate::eval::{reparse_args, text_of};
use crate::syntax::{line_at, segments, text};

/// The name of a receiver that has none.
const UNNAMED: &str = "_";

/// A site in the syntax tree, before its arguments are read.
pub(super) struct Candidate {
    pub(super) callee: String,
    pub(super) line: Line,
    /// Where the router chain or the item the site belongs to starts in the file.
    pub(super) anchor: u32,
    /// For a method call, what the method is called on.
    pub(super) receiver: Option<ast::Expr>,
    arguments: Arguments,
    /// For an attribute, the name of what it sits on.
    member: Option<String>,
}

/// Where the arguments of a site are. A macro's or an attribute's are tokens, read as a call's when a rule asks.
enum Arguments {
    List(Option<ast::ArgList>),
    Tree(Option<ast::TokenTree>),
}

impl Candidate {
    /// A call, a method call or a macro call. `None` when the callee is no plain path, as in `(self.f)(x)`.
    pub(super) fn of_call(node: &SyntaxNode, lines: &LineIndex) -> Option<Self> {
        match node.kind() {
            SyntaxKind::CALL_EXPR => {
                let call = ast::CallExpr::cast(node.clone())?;
                Some(Self {
                    callee: path_callee(&call)?,
                    line: line_at(lines, node.text_range().start()),
                    anchor: u32::from(node.text_range().start()),
                    receiver: None,
                    arguments: Arguments::List(call.arg_list()),
                    member: None,
                })
            }
            SyntaxKind::METHOD_CALL_EXPR => {
                let call = ast::MethodCallExpr::cast(node.clone())?;
                let name = call.name_ref()?;
                let receiver = call.receiver()?;
                Some(Self {
                    callee: format!("{}.{}", receiver_name(&receiver), text(&name)),
                    // A chain spans lines, and the call is where its method is written.
                    line: line_at(lines, name.syntax().text_range().start()),
                    anchor: u32::from(chain_top(node).text_range().start()),
                    receiver: Some(receiver),
                    arguments: Arguments::List(call.arg_list()),
                    member: None,
                })
            }
            SyntaxKind::MACRO_CALL => {
                let call = ast::MacroCall::cast(node.clone())?;
                Some(Self {
                    callee: format!("{}!", segments(&call.path()?)?.join(".")),
                    line: line_at(lines, node.text_range().start()),
                    anchor: u32::from(node.text_range().start()),
                    receiver: None,
                    arguments: Arguments::Tree(call.token_tree()),
                    member: None,
                })
            }
            _ => None,
        }
    }

    /// An attribute, with the name of the item it sits on.
    pub(super) fn of_attribute(node: &SyntaxNode, lines: &LineIndex) -> Option<Self> {
        let attr = ast::Attr::cast(node.clone())?;
        let item = node.parent()?;
        let tree = match attr.meta()? {
            ast::Meta::TokenTreeMeta(meta) => meta.token_tree(),
            _ => None,
        };
        Some(Self {
            callee: segments(&attr.path()?)?.join("."),
            line: line_at(lines, node.text_range().start()),
            anchor: u32::from(item.text_range().start()),
            receiver: None,
            arguments: Arguments::Tree(tree),
            member: item
                .children()
                .find_map(ast::Name::cast)
                .map(|name| text(&name)),
        })
    }

    /// Reads the arguments. Only done for a site a rule matched, since a macro's are parsed from tokens.
    pub(super) fn prepare(&self, edition: Edition) -> Prepared {
        let (arguments, split) = match &self.arguments {
            Arguments::List(list) => (
                list.iter().flat_map(ast::ArgList::args).collect::<Vec<_>>(),
                false,
            ),
            Arguments::Tree(tree) => (
                tree.iter()
                    .flat_map(|tree| reparse_args(&text(tree), edition))
                    .collect(),
                true,
            ),
        };
        let mut positional = Vec::new();
        let mut keywords = Vec::new();
        for argument in arguments {
            match split.then(|| keyword(&argument)).flatten() {
                Some(pair) => keywords.push(pair),
                None => positional.push(argument),
            }
        }
        Prepared {
            callee: self.callee.clone(),
            positional,
            keywords,
            member: self.member.clone(),
            edition,
        }
    }
}

/// The names a call goes to, when the callee is a plain path.
pub(super) fn callee_segments(call: &ast::CallExpr) -> Option<Vec<String>> {
    let ast::Expr::PathExpr(path) = call.expr()? else {
        return None;
    };
    segments(&path.path()?)
}

/// The dotted path a call goes to, when the callee is a plain path.
pub(super) fn path_callee(call: &ast::CallExpr) -> Option<String> {
    Some(callee_segments(call)?.join("."))
}

/// The expression a receiver stands for, through `&`, `?`, `.await`, parentheses and `.clone()`.
pub(super) fn peel(expr: &ast::Expr) -> Option<ast::Expr> {
    match expr {
        ast::Expr::ParenExpr(inner) => peel(&inner.expr()?),
        ast::Expr::RefExpr(inner) => peel(&inner.expr()?),
        ast::Expr::TryExpr(inner) => peel(&inner.expr()?),
        ast::Expr::AwaitExpr(inner) => peel(&inner.expr()?),
        ast::Expr::MethodCallExpr(call)
            if call.name_ref().is_some_and(|name| text(&name) == "clone") =>
        {
            peel(&call.receiver()?)
        }
        other => Some(other.clone()),
    }
}

/// The last name an expression is written with: the last segment of a path, the name of a field.
pub(super) fn last_name(expr: &ast::Expr) -> Option<String> {
    match peel(expr)? {
        ast::Expr::PathExpr(path) => segments(&path.path()?)?.pop(),
        ast::Expr::FieldExpr(field) => Some(text(&field.name_ref()?)),
        _ => None,
    }
}

/// The name a method call's receiver goes by.
fn receiver_name(receiver: &ast::Expr) -> String {
    last_name(receiver).unwrap_or_else(|| UNNAMED.to_owned())
}

/// The outermost call of the method chain that `call` belongs to.
pub(super) fn chain_top(call: &SyntaxNode) -> SyntaxNode {
    let mut top = call.clone();
    while let Some(parent) = top.parent().filter(|parent| {
        ast::MethodCallExpr::cast(parent.clone())
            .and_then(|outer| outer.receiver())
            .is_some_and(|receiver| *receiver.syntax() == top)
    }) {
        top = parent;
    }
    top
}

/// `name = value` as an argument.
fn keyword(argument: &ast::Expr) -> Option<(String, ast::Expr)> {
    let ast::Expr::BinExpr(assignment) = argument else {
        return None;
    };
    if assignment.op_kind() != Some(ast::BinaryOp::Assignment { op: None }) {
        return None;
    }
    let ast::Expr::PathExpr(name) = assignment.lhs()? else {
        return None;
    };
    let [name] = <[String; 1]>::try_from(segments(&name.path()?)?).ok()?;
    Some((name, assignment.rhs()?))
}

/// The arguments of a site, split into positional ones and `name = value` ones.
#[derive(Clone)]
pub(super) struct Prepared {
    callee: String,
    positional: Vec<ast::Expr>,
    keywords: Vec<(String, ast::Expr)>,
    member: Option<String>,
    /// The edition of the file the arguments were parsed in.
    edition: Edition,
}

impl Prepared {
    /// A site that is only a name and a value, as an assignment is: the value is the first argument.
    pub(super) fn assignment(target: String, value: ast::Expr, edition: Edition) -> Self {
        Self {
            callee: target,
            positional: vec![value],
            keywords: Vec::new(),
            member: None,
            edition,
        }
    }

    /// The site once for each value of the keyword its HTTP method is read from, when the keyword is given more than once: actix's `#[route("/", method = "GET", method = "POST")]`.
    pub(super) fn per_method(&self, method: Option<&ValueExpr>) -> Vec<Self> {
        let keyword = method.and_then(|method| {
            method.sources().iter().find_map(|source| match source {
                ValueSource::Keyword(name) => Some(name.as_str()),
                _ => None,
            })
        });
        let Some(keyword) = keyword else {
            return vec![self.clone()];
        };
        let given: Vec<usize> = self
            .keywords
            .iter()
            .enumerate()
            .filter(|(_, (name, _))| name == keyword)
            .map(|(index, _)| index)
            .collect();
        if given.len() < 2 {
            return vec![self.clone()];
        }
        given
            .iter()
            .map(|kept| Self {
                callee: self.callee.clone(),
                positional: self.positional.clone(),
                keywords: self
                    .keywords
                    .iter()
                    .enumerate()
                    .filter(|(index, (name, _))| name != keyword || index == kept)
                    .map(|(_, pair)| pair.clone())
                    .collect(),
                member: self.member.clone(),
                edition: self.edition,
            })
            .collect()
    }
}

/// A [`Prepared`] site as the rules read it.
pub(super) struct View<'p> {
    site: &'p Prepared,
    /// For one application of a rule to a method router: the argument it was read from, and the verb to report there.
    verb: Option<(NonZeroU32, &'p str)>,
}

impl<'p> View<'p> {
    pub(super) fn new(site: &'p Prepared) -> Self {
        Self { site, verb: None }
    }

    fn pinned(site: &'p Prepared, verb: &'p Verb) -> Self {
        Self {
            site,
            verb: Some((verb.router, verb.name.as_str())),
        }
    }
}

fn index(position: NonZeroU32) -> Option<usize> {
    usize::try_from(position.get() - 1).ok()
}

impl Site for View<'_> {
    fn callee_last(&self) -> Option<Cow<'_, str>> {
        self.site
            .callee
            .rsplit('.')
            .next()
            .map(|last| Cow::Borrowed(last.trim_end_matches('!')))
    }

    fn arg(&self, position: NonZeroU32) -> Option<Cow<'_, str>> {
        let argument = self.site.positional.get(index(position)?)?;
        text_of(argument, false, self.site.edition).map(Cow::Owned)
    }

    fn arg_prop(&self, position: NonZeroU32, prop: &str) -> Option<Cow<'_, str>> {
        match prop {
            "method" => self
                .verb
                .filter(|(at, _)| *at == position)
                .map(|(_, verb)| Cow::Borrowed(verb)),
            "last" => match self.site.positional.get(index(position)?)? {
                ast::Expr::PathExpr(path) => segments(&path.path()?)?.pop().map(Cow::Owned),
                _ => None,
            },
            _ => None,
        }
    }

    fn keyword(&self, name: &str) -> Option<Cow<'_, str>> {
        let (_, value) = self
            .site
            .keywords
            .iter()
            .find(|(keyword, _)| keyword == name)?;
        text_of(value, false, self.site.edition).map(Cow::Owned)
    }

    fn member(&self) -> Option<Cow<'_, str>> {
        self.site.member.as_deref().map(Cow::Borrowed)
    }
}

/// Whether `key` reads keywords only and `site` gives none of them. `#[sea_orm(primary_key)]` has no `table_name`, which makes it another attribute, where `#[sea_orm(table_name = TABLE)]` is a table whose name is not read.
pub(super) fn lacks_keywords(key: &ValueExpr, site: &Prepared) -> bool {
    let sources = key.sources();
    !sources.is_empty()
        && sources.iter().all(|source| {
            matches!(source, ValueSource::Keyword(name)
                if !site.keywords.iter().any(|(given, _)| given == name))
        })
}

/// One verb of a method router, and the argument of the call it was read from.
struct Verb {
    router: NonZeroU32,
    name: String,
}

/// Applies `rule` to a call once for each verb of the method router it reads its HTTP method from, or once when it reads none.
pub(super) fn apply_per_verb(
    rule: &CallRule,
    site: &Prepared,
    mut apply: impl FnMut(&View<'_>) -> Outcome,
) -> Vec<Outcome> {
    let router = rule
        .method
        .as_ref()
        .and_then(method_position)
        .and_then(|position| Some((position, site.positional.get(index(position)?)?)));
    let verbs: Vec<Verb> = router
        .iter()
        .flat_map(|(position, router)| {
            verbs(router).into_iter().map(|name| Verb {
                router: *position,
                name,
            })
        })
        .collect();
    if verbs.is_empty() {
        return vec![apply(&View::new(site))];
    }
    verbs
        .iter()
        .map(|verb| apply(&View::pinned(site, verb)))
        .collect()
}

/// The argument whose `.method` a rule reads, if any.
fn method_position(method: &ValueExpr) -> Option<NonZeroU32> {
    method.sources().iter().find_map(|source| match source {
        ValueSource::ArgProp(position, prop) if prop == "method" => Some(*position),
        _ => None,
    })
}

/// The HTTP verbs a method router names, upper case, `*` for `any`: `get(show).delete(destroy)` has GET and DELETE, `web::get().to(show)` has GET.
fn verbs(router: &ast::Expr) -> Vec<String> {
    let mut found = Vec::new();
    let mut current = Some(router.clone());
    while let Some(expr) = current.take() {
        current = match &expr {
            ast::Expr::MethodCallExpr(call) => {
                found.extend(call.name_ref().and_then(|name| verb(&text(&name))));
                call.receiver()
            }
            ast::Expr::CallExpr(call) => {
                found.extend(
                    path_callee(call).and_then(|callee| callee.rsplit('.').next().and_then(verb)),
                );
                None
            }
            ast::Expr::ParenExpr(inner) => inner.expr(),
            _ => None,
        };
    }
    found.sort();
    found.dedup();
    found
}

/// The verb a function or method of a router is named after: `get`, or `get_service`.
fn verb(name: &str) -> Option<String> {
    let name = name.strip_suffix("_service").unwrap_or(name);
    match name {
        "get" | "post" | "put" | "patch" | "delete" | "head" | "options" | "trace" => {
            Some(name.to_ascii_uppercase())
        }
        "any" => Some("*".to_owned()),
        _ => None,
    }
}
