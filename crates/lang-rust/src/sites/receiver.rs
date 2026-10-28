//! Receivers: whether the file shows what a method is called on to be what a rule wants.
//!
//! `cache.get("/bookings")` and `client.get("/bookings")` read alike, so a rule with a `receiver` applies only where
//! the same file binds the receiver to a call the rule names, or declares it with a type the rule names. Nothing is
//! inferred past that: a client that comes out of another function is not seen.

use gsttt_core::rules::{OneOrMany, Prefix};
use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, SyntaxNode};

use super::Reader;
use super::candidate::{Candidate, View, path_callee, peel};
use crate::syntax::{segments, text};

/// How the file shows a receiver to be what a rule wants.
enum Origin {
    /// It is made by this call.
    Call(SyntaxNode),
    /// It is declared with a type the rule names, so no call carries a prefix.
    Declared,
}

impl Reader<'_> {
    /// The prefix the receiver of `candidate` carries when the file shows it to be made by a call `prefix` names, or
    /// to be declared with a type it names. `None` when it does not.
    pub(super) fn gate(&self, candidate: &Candidate, prefix: &Prefix) -> Option<String> {
        let receiver = candidate.receiver.as_ref()?;
        match self.origin(receiver, &prefix.from)? {
            Origin::Call(maker) => {
                let maker = Candidate::of_call(&maker, self.lines)?.prepare();
                Some(prefix.read(&View::new(&maker)))
            }
            Origin::Declared => Some(String::new()),
        }
    }

    fn origin(&self, receiver: &ast::Expr, from: &OneOrMany) -> Option<Origin> {
        match peel(receiver)? {
            ast::Expr::PathExpr(path) => {
                let [name] = <[String; 1]>::try_from(segments(&path.path()?)?).ok()?;
                self.binding(path.syntax(), &name, from)
            }
            ast::Expr::FieldExpr(field) => {
                let name = text(&field.name_ref()?);
                self.root
                    .descendants()
                    .filter_map(ast::RecordField::cast)
                    .any(|declared| {
                        declared.name().is_some_and(|field| text(&field) == name)
                            && declared.ty().is_some_and(|ty| type_names(&ty, from))
                    })
                    .then_some(Origin::Declared)
            }
            other => maker(&other, from).map(Origin::Call),
        }
    }

    /// What a local name stands for at `at`: the call its last `let` binds it to, or the type it is declared with as
    /// a `let`, a parameter, a `static` or a `const`.
    fn binding(&self, at: &SyntaxNode, name: &str, from: &OneOrMany) -> Option<Origin> {
        if name == "self" {
            return None;
        }
        let function = at.ancestors().find_map(ast::Fn::cast);
        let scope = function
            .as_ref()
            .map_or_else(|| self.root.clone(), |function| function.syntax().clone());
        let start = at.text_range().start();
        let bound = scope
            .descendants()
            .filter_map(ast::LetStmt::cast)
            .filter(|statement| statement.syntax().text_range().end() <= start)
            .filter(|statement| is_named(statement.pat(), name))
            .last();
        if let Some(statement) = bound {
            if let Some(call) = statement.initializer().and_then(|init| maker(&init, from)) {
                return Some(Origin::Call(call));
            }
            return statement
                .ty()
                .is_some_and(|ty| type_names(&ty, from))
                .then_some(Origin::Declared);
        }
        let parameter = function
            .and_then(|function| function.param_list())
            .into_iter()
            .flat_map(|list| list.params())
            .any(|param| {
                is_named(param.pat(), name) && param.ty().is_some_and(|ty| type_names(&ty, from))
            });
        let item = self.root.descendants().any(|node| {
            let typed = |item_name: Option<ast::Name>, ty: Option<ast::Type>| {
                item_name.is_some_and(|item| text(&item) == name)
                    && ty.is_some_and(|ty| type_names(&ty, from))
            };
            ast::Static::cast(node.clone()).is_some_and(|item| typed(item.name(), item.ty()))
                || ast::Const::cast(node).is_some_and(|item| typed(item.name(), item.ty()))
        });
        (parameter || item).then_some(Origin::Declared)
    }
}

/// The call at the root of a method chain, when its callee is one of `from`: `Client::builder()` in
/// `Client::builder().build()?`.
fn maker(expr: &ast::Expr, from: &OneOrMany) -> Option<SyntaxNode> {
    match expr {
        ast::Expr::CallExpr(call) => from
            .matches(&path_callee(call)?)
            .then(|| call.syntax().clone()),
        ast::Expr::MethodCallExpr(call) => maker(&call.receiver()?, from),
        ast::Expr::ParenExpr(inner) => maker(&inner.expr()?, from),
        ast::Expr::RefExpr(inner) => maker(&inner.expr()?, from),
        ast::Expr::TryExpr(inner) => maker(&inner.expr()?, from),
        ast::Expr::AwaitExpr(inner) => maker(&inner.expr()?, from),
        _ => None,
    }
}

fn is_named(pattern: Option<ast::Pat>, name: &str) -> bool {
    matches!(pattern, Some(ast::Pat::IdentPat(ident)) if ident.name().is_some_and(|bound| text(&bound) == name))
}

/// Whether a declared type mentions the type a `from` pattern constructs: `Client` and `reqwest::Client` both stand
/// for `reqwest.Client.new`, and so does `LazyLock<Client>`.
fn type_names(ty: &ast::Type, from: &OneOrMany) -> bool {
    let wanted: Vec<&str> = from
        .iter()
        .filter_map(|pattern| pattern.rsplit_once('.').map(|(type_path, _)| type_path))
        .collect();
    ty.syntax()
        .descendants()
        .filter_map(ast::Path::cast)
        .filter_map(|path| segments(&path))
        .map(|names| names.join("."))
        .any(|found| wanted.iter().any(|want| same_tail(&found, want)))
}

/// Whether the shorter of two dotted paths is the tail of the longer: `Client` and `reqwest.Client` agree.
fn same_tail(a: &str, b: &str) -> bool {
    let (a, b): (Vec<&str>, Vec<&str>) = (a.split('.').collect(), b.split('.').collect());
    let (long, short) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    long.iter()
        .rev()
        .zip(short.iter().rev())
        .all(|(long, short)| *short == "*" || long == short)
}
