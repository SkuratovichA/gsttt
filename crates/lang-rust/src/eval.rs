//! Arguments and strings as the rules read them.

use std::borrow::Cow;

use ra_ap_edition::Edition;
use ra_ap_syntax::ast::HasArgList;
use ra_ap_syntax::{AstNode, ast};

use crate::syntax::{segments, text};

/// Stands for a part of a string that is only known at run time. The rules treat it as such.
const COMPUTED: &str = "${}";

/// The arguments of a macro call or an attribute, read the way a function call's arguments are, in the `edition` of the file they are written in. `delimited` is the token tree with its brackets.
pub(crate) fn reparse_args(delimited: &str, edition: Edition) -> Vec<ast::Expr> {
    let mut inner: String = delimited.chars().skip(1).collect();
    inner.pop();
    let source = format!("fn _() {{ _({inner}); }}");
    ast::SourceFile::parse(&source, edition)
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::CallExpr::cast)
        .and_then(|call| call.arg_list())
        .map(|list| list.args().collect())
        .unwrap_or_default()
}

/// The arguments of a macro call.
pub(crate) fn macro_args(call: &ast::MacroCall, edition: Edition) -> Vec<ast::Expr> {
    call.token_tree()
        .map(|tree| reparse_args(&text(&tree), edition))
        .unwrap_or_default()
}

/// An expression as text: a string literal, or a template, a `concat!` or a `+` whose computed parts are written as `${}`. With `named_env`, `env!("NAME")` is written `${NAME}`. `None` for anything that is not a string. `edition` is the one the file is written in, for the macros read on the way.
pub(crate) fn text_of(expr: &ast::Expr, named_env: bool, edition: Edition) -> Option<String> {
    match expr {
        ast::Expr::Literal(literal) => match literal.kind() {
            ast::LiteralKind::String(string) => string.value().ok().map(Cow::into_owned),
            _ => None,
        },
        ast::Expr::ParenExpr(inner) => text_of(&inner.expr()?, named_env, edition),
        ast::Expr::RefExpr(inner) => text_of(&inner.expr()?, named_env, edition),
        ast::Expr::MacroExpr(inner) => macro_text(&inner.macro_call()?, named_env, edition),
        ast::Expr::MethodCallExpr(call) => {
            let name = text(&call.name_ref()?);
            let keeps_text = matches!(
                name.as_str(),
                "to_string" | "to_owned" | "into" | "as_str" | "clone"
            );
            keeps_text
                .then(|| text_of(&call.receiver()?, named_env, edition))
                .flatten()
        }
        ast::Expr::CallExpr(call) => {
            let ast::Expr::PathExpr(path) = call.expr()? else {
                return None;
            };
            let from = segments(&path.path()?)?
                .last()
                .is_some_and(|name| name == "from");
            from.then(|| text_of(&call.arg_list()?.args().next()?, named_env, edition))
                .flatten()
        }
        ast::Expr::BinExpr(bin) => {
            let add = bin.op_kind() == Some(ast::BinaryOp::ArithOp(ast::ArithOp::Add));
            if !add {
                return None;
            }
            let left = text_of(&bin.lhs()?, named_env, edition);
            let right = text_of(&bin.rhs()?, named_env, edition);
            if left.is_none() && right.is_none() {
                return None;
            }
            Some(format!(
                "{}{}",
                left.unwrap_or_else(|| COMPUTED.to_owned()),
                right.unwrap_or_else(|| COMPUTED.to_owned())
            ))
        }
        _ => None,
    }
}

fn macro_text(call: &ast::MacroCall, named_env: bool, edition: Edition) -> Option<String> {
    let name = segments(&call.path()?)?.pop()?;
    let args = macro_args(call, edition);
    match name.as_str() {
        "format" | "format_args" => Some(template(&text_of(args.first()?, named_env, edition)?)),
        "concat" => {
            let parts: Vec<Option<String>> = args
                .iter()
                .map(|arg| text_of(arg, named_env, edition))
                .collect();
            if parts.iter().all(Option::is_none) {
                return None;
            }
            Some(
                parts
                    .into_iter()
                    .map(|part| part.unwrap_or_else(|| COMPUTED.to_owned()))
                    .collect(),
            )
        }
        "env" | "option_env" => Some(
            match args.first().and_then(|arg| text_of(arg, false, edition)) {
                Some(variable) if named_env => format!("${{{variable}}}"),
                _ => COMPUTED.to_owned(),
            },
        ),
        _ => None,
    }
}

/// A format string with each `{...}` replaced by the computed marker.
fn template(format: &str) -> String {
    let mut out = String::new();
    let mut chars = format.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '{' => {
                for inner in chars.by_ref() {
                    if inner == '}' {
                        break;
                    }
                }
                out.push_str(COMPUTED);
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            other => out.push(other),
        }
    }
    out
}
