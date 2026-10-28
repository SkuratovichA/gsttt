//! The two rule kinds that read names rather than calls.
//!
//! A chain is a path followed by field accesses, method names and string indexes: `Config::DATABASE.url` is the root
//! `Config` with the segments `DATABASE` and `url`, and `settings["db"].host()` is the root `settings` with the
//! segments `db` and `host`. A segment that is computed, as in `settings[key]`, is unknown.
//!
//! An assignment is a `const`, a `static`, a `let`, a field given a value in a struct literal, or `target = value`.
//! The target is the last name written, and the value is the first argument of the site. Where Python has a base
//! class, Rust has the trait of the enclosing `impl` block, which is what a rule's `class` is matched against.

use gsttt_core::rules::{AssignRule, Pack, callee_matches};
use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode};

use super::candidate::{Prepared, View, last_name};
use super::{Outcomes, Reader, Sites};
use crate::eval::text_of;
use crate::syntax::{line_at, segments, text};

/// A chain read off an expression: the names of its root path, then its segments.
type Names = (Vec<String>, Vec<Option<String>>);

impl Reader<'_> {
    /// The rules that read a member chain, at the outermost expression of one.
    pub(super) fn chain(&self, node: &SyntaxNode, sites: &mut Sites) {
        if self.chains.is_empty() || continues(node) {
            return;
        }
        let Some((root, rest)) = ast::Expr::cast(node.clone()).and_then(|expr| names(&expr)) else {
            return;
        };
        let names: Vec<Option<&str>> = root
            .iter()
            .map(|name| Some(name.as_str()))
            .chain(rest.iter().map(Option::as_deref))
            .collect();
        let line = line_at(self.lines, node.text_range().start());
        let mut outcomes = Outcomes::default();
        for (pack, rule) in &self.chains {
            for pattern in rule.root.iter() {
                if let Some((written, after)) = split_root(pattern, &names) {
                    outcomes.add(rule.apply(&pack.name, after, line, &written));
                }
            }
        }
        outcomes.flush(u32::from(node.text_range().start()), sites);
    }

    /// The rules that read an assignment.
    pub(super) fn assign(&self, node: &SyntaxNode, sites: &mut Sites) {
        if self.assigns.is_empty() {
            return;
        }
        let Some((target, value)) = assignment(node) else {
            return;
        };
        let rules: Vec<&(&Pack, &AssignRule)> = self
            .assigns
            .iter()
            .filter(|(_, rule)| rule.target.matches(&target))
            .filter(|(_, rule)| {
                rule.class.as_ref().is_none_or(|class| {
                    enclosing_trait(node).is_some_and(|implemented| class.matches(&implemented))
                })
            })
            .collect();
        if rules.is_empty() {
            return;
        }
        let line = line_at(self.lines, node.text_range().start());
        let site = Prepared::assignment(target.clone(), value);
        let view = View::new(&site);
        let mut outcomes = Outcomes::default();
        for (pack, rule) in rules {
            outcomes.add(rule.apply(&pack.name, &view, line, &target));
        }
        outcomes.flush(u32::from(node.text_range().start()), sites);
    }
}

/// `pattern` as a root of `names`, with the root as written and the segments after it. `None` when the chain does not
/// start with the root, or when a name inside the root is computed.
fn split_root<'n>(
    pattern: &str,
    names: &'n [Option<&'n str>],
) -> Option<(String, &'n [Option<&'n str>])> {
    let width = pattern.split('.').count();
    let written = names
        .get(..width)?
        .iter()
        .copied()
        .collect::<Option<Vec<&str>>>()?
        .join(".");
    callee_matches(pattern, &written).then(|| (written, names.get(width..).unwrap_or_default()))
}

/// Whether an access above `node` continues the chain it ends, so that `node` is not the chain to read.
fn continues(node: &SyntaxNode) -> bool {
    let mut current = node.clone();
    while let Some(parent) = current.parent() {
        let extends = match parent.kind() {
            SyntaxKind::PAREN_EXPR | SyntaxKind::TRY_EXPR | SyntaxKind::AWAIT_EXPR => {
                current = parent;
                continue;
            }
            SyntaxKind::FIELD_EXPR => ast::FieldExpr::cast(parent).and_then(|field| field.expr()),
            SyntaxKind::METHOD_CALL_EXPR => {
                ast::MethodCallExpr::cast(parent).and_then(|call| call.receiver())
            }
            SyntaxKind::INDEX_EXPR => ast::IndexExpr::cast(parent).and_then(|index| index.base()),
            _ => None,
        };
        return extends.is_some_and(|inner| *inner.syntax() == current);
    }
    false
}

/// The names of a chain: its root path and what follows.
fn names(expr: &ast::Expr) -> Option<Names> {
    let follow = |inner: Option<ast::Expr>, next: Option<String>| {
        let (root, mut rest) = names(&inner?)?;
        rest.push(next);
        Some((root, rest))
    };
    match expr {
        ast::Expr::PathExpr(path) => Some((segments(&path.path()?)?, Vec::new())),
        ast::Expr::FieldExpr(field) => follow(field.expr(), Some(text(&field.name_ref()?))),
        ast::Expr::MethodCallExpr(call) => follow(call.receiver(), Some(text(&call.name_ref()?))),
        ast::Expr::IndexExpr(index) => follow(
            index.base(),
            index.index().and_then(|key| text_of(&key, false)),
        ),
        ast::Expr::ParenExpr(inner) => names(&inner.expr()?),
        ast::Expr::TryExpr(inner) => names(&inner.expr()?),
        ast::Expr::AwaitExpr(inner) => names(&inner.expr()?),
        _ => None,
    }
}

/// The name and the value of an assignment.
fn assignment(node: &SyntaxNode) -> Option<(String, ast::Expr)> {
    match node.kind() {
        SyntaxKind::CONST => {
            let item = ast::Const::cast(node.clone())?;
            Some((text(&item.name()?), item.body()?))
        }
        SyntaxKind::STATIC => {
            let item = ast::Static::cast(node.clone())?;
            Some((text(&item.name()?), item.body()?))
        }
        SyntaxKind::LET_STMT => {
            let statement = ast::LetStmt::cast(node.clone())?;
            let ast::Pat::IdentPat(pattern) = statement.pat()? else {
                return None;
            };
            Some((text(&pattern.name()?), statement.initializer()?))
        }
        SyntaxKind::RECORD_EXPR_FIELD => {
            let field = ast::RecordExprField::cast(node.clone())?;
            Some((text(&field.name_ref()?), field.expr()?))
        }
        SyntaxKind::BIN_EXPR => {
            let assignment = ast::BinExpr::cast(node.clone())?;
            if assignment.op_kind() != Some(ast::BinaryOp::Assignment { op: None }) {
                return None;
            }
            Some((last_name(&assignment.lhs()?)?, assignment.rhs()?))
        }
        _ => None,
    }
}

/// The trait the innermost enclosing `impl` block implements, as a dotted path.
fn enclosing_trait(node: &SyntaxNode) -> Option<String> {
    let block = node.ancestors().find_map(ast::Impl::cast)?;
    match block.trait_()? {
        ast::Type::PathType(path) => Some(segments(&path.path()?)?.join(".")),
        _ => None,
    }
}
