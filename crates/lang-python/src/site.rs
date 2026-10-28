//! Python call, decorator and assignment sites as the rule engine reads them, and the text of a Python value.
//!
//! The rules of `gsttt_core::rules` are written against [`Site`]; this file is the whole of what Python adds to
//! it: which expressions count as text, and how a dotted name is written.

use std::borrow::Cow;
use std::num::NonZeroU32;

use gsttt_core::contract::ContractKind;
use gsttt_core::rules::Site;
use ruff_python_ast::{Arguments, Expr, FStringPartRef, InterpolatedStringElement, Operator};

/// The text a computed part of a template is written as, the way every connector hands it to the rules.
const COMPUTED: &str = "${}";

/// The text of a Python value, when it has any: a string literal, an f-string or a `+` concatenation with each
/// computed part written as `${}`, a `"…".format(…)` call as its template, and a list, tuple or set as its first
/// element that has text.
pub(crate) fn text_of(expr: &Expr) -> Option<Cow<'_, str>> {
    match expr {
        Expr::StringLiteral(string) => Some(Cow::Borrowed(string.value.to_str())),
        Expr::FString(fstring) => Some(Cow::Owned(fstring_text(fstring))),
        Expr::BinOp(binop) if binop.op == Operator::Add => {
            let left = text_of(&binop.left);
            let right = text_of(&binop.right);
            if left.is_none() && right.is_none() {
                return None;
            }
            Some(Cow::Owned(format!(
                "{}{}",
                left.as_deref().unwrap_or(COMPUTED),
                right.as_deref().unwrap_or(COMPUTED)
            )))
        }
        Expr::Call(call) => match &*call.func {
            Expr::Attribute(attribute) if attribute.attr.as_str() == "format" => {
                text_of(&attribute.value)
            }
            _ => None,
        },
        Expr::List(list) => first_text(&list.elts),
        Expr::Tuple(tuple) => first_text(&tuple.elts),
        Expr::Set(set) => first_text(&set.elts),
        _ => None,
    }
}

/// Like [`text_of`], but only when no part of the text is computed.
pub(crate) fn literal_of(expr: &Expr) -> Option<Cow<'_, str>> {
    text_of(expr).filter(|text| !text.contains(COMPUTED))
}

fn first_text(elements: &[Expr]) -> Option<Cow<'_, str>> {
    elements.iter().find_map(text_of)
}

fn fstring_text(fstring: &ruff_python_ast::ExprFString) -> String {
    let mut text = String::new();
    for part in &fstring.value {
        match part {
            FStringPartRef::Literal(literal) => text.push_str(&literal.value),
            FStringPartRef::FString(fstring) => {
                for element in &fstring.elements {
                    match element {
                        InterpolatedStringElement::Literal(literal) => {
                            text.push_str(&literal.value);
                        }
                        InterpolatedStringElement::Interpolation(_) => text.push_str(COMPUTED),
                    }
                }
            }
        }
    }
    text
}

/// The property `prop` of a dict literal, as text.
pub(crate) fn dict_prop<'a>(expr: &'a Expr, prop: &str) -> Option<Cow<'a, str>> {
    let Expr::Dict(dict) = expr else {
        return None;
    };
    dict.items.iter().find_map(|item| {
        let Some(Expr::StringLiteral(key)) = &item.key else {
            return None;
        };
        if key.value.to_str() == prop {
            text_of(&item.value)
        } else {
            None
        }
    })
}

/// The dotted name of a `Name` or an attribute chain: `os.environ`, `self.router`. A chain that starts at
/// something else, such as a call or a subscript, has `?` for that segment so that `*` patterns still match:
/// `Session().execute` is `?.execute`. `None` when the expression is not a name at all.
pub(crate) fn dotted(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(name) => Some(name.id.as_str().to_owned()),
        Expr::Attribute(attribute) => {
            let base = dotted(&attribute.value).unwrap_or_else(|| "?".to_owned());
            Some(format!("{base}.{}", attribute.attr.as_str()))
        }
        _ => None,
    }
}

/// `outer` and `inner` as one prefix, with `/` between them for HTTP and `.` otherwise; an empty side is left out.
///
/// `gsttt_core::rules` joins the class and receiver prefixes of a decorator rule the same way before it reads the
/// rule's own `prefix`; its `join` is private, so this is the copy, and `joins_match_the_rule_engine` pins it.
pub(crate) fn join_prefixes(kind: &ContractKind, outer: &str, inner: &str) -> String {
    match (outer.is_empty(), inner.is_empty()) {
        (true, _) => inner.to_owned(),
        (false, true) => outer.to_owned(),
        (false, false) => match kind {
            ContractKind::Http => format!("{outer}/{inner}"),
            _ => format!("{outer}.{inner}"),
        },
    }
}

/// A call, or a decorator that may be a call: `requests.get("/x")`, `@router.get("/x")`, `@app.route`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CallSite<'a> {
    pub(crate) callee: &'a str,
    /// `None` for a decorator that is not called.
    pub(crate) arguments: Option<&'a Arguments>,
    /// The function or class a decorator sits on.
    pub(crate) member: Option<&'a str>,
}

impl Site for CallSite<'_> {
    fn callee_last(&self) -> Option<Cow<'_, str>> {
        self.callee.rsplit('.').next().map(Cow::Borrowed)
    }

    fn arg(&self, position: NonZeroU32) -> Option<Cow<'_, str>> {
        let index = usize::try_from(position.get().checked_sub(1)?).ok()?;
        text_of(self.arguments?.find_positional(index)?)
    }

    fn arg_prop(&self, position: NonZeroU32, prop: &str) -> Option<Cow<'_, str>> {
        let index = usize::try_from(position.get().checked_sub(1)?).ok()?;
        dict_prop(self.arguments?.find_positional(index)?, prop)
    }

    fn keyword(&self, name: &str) -> Option<Cow<'_, str>> {
        text_of(&self.arguments?.find_keyword(name)?.value)
    }

    fn member(&self) -> Option<Cow<'_, str>> {
        self.member.map(Cow::Borrowed)
    }
}

/// An assignment `target = value`: the target is the callee and the value the first argument.
#[derive(Clone, Debug)]
pub(crate) struct AssignSite<'a> {
    /// The name read as the callee: the assigned name, or the environment variable a settings field maps to.
    pub(crate) name: Cow<'a, str>,
    /// `None` for an annotation without a value.
    pub(crate) value: Option<&'a Expr>,
}

impl Site for AssignSite<'_> {
    fn callee_last(&self) -> Option<Cow<'_, str>> {
        self.name.rsplit('.').next().map(Cow::Borrowed)
    }

    fn arg(&self, position: NonZeroU32) -> Option<Cow<'_, str>> {
        if position != NonZeroU32::MIN {
            return None;
        }
        text_of(self.value?)
    }

    fn arg_prop(&self, position: NonZeroU32, prop: &str) -> Option<Cow<'_, str>> {
        if position != NonZeroU32::MIN {
            return None;
        }
        dict_prop(self.value?, prop)
    }

    fn keyword(&self, _name: &str) -> Option<Cow<'_, str>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use gsttt_core::rules::{CallRule, Outcome};
    use ruff_python_parser::parse_module;

    use super::*;

    fn call_text(source: &str, position: u32) -> Option<String> {
        let parsed = parse_module(source).unwrap();
        let Some(ruff_python_ast::Stmt::Expr(statement)) = parsed.suite().first() else {
            panic!("not an expression statement");
        };
        let Expr::Call(call) = &*statement.value else {
            panic!("not a call");
        };
        let site = CallSite {
            callee: "f",
            arguments: Some(&call.arguments),
            member: None,
        };
        site.arg(NonZeroU32::new(position).unwrap())
            .map(Cow::into_owned)
    }

    #[test]
    fn values_read_as_text() {
        let cases = [
            (r#"f("/bookings")"#, Some("/bookings")),
            (r#"f(f"{BASE}/bookings/{id}")"#, Some("${}/bookings/${}")),
            (r#"f(BASE + "/bookings")"#, Some("${}/bookings")),
            (
                r#"f("/bookings/" + id + "/cancel")"#,
                Some("/bookings/${}/cancel"),
            ),
            (r#"f("/bookings/{}".format(id))"#, Some("/bookings/{}")),
            (r#"f("a" "b")"#, Some("ab")),
            (r#"f(["GET", "POST"])"#, Some("GET")),
            (r"f(url)", None),
            (r"f(1 + 2)", None),
            (r"f(g())", None),
        ];
        for (source, want) in cases {
            assert_eq!(call_text(source, 1).as_deref(), want, "{source}");
        }
    }

    #[test]
    fn dotted_names_use_a_placeholder_for_calls() {
        let parsed = parse_module("a.b.c\nSession().execute\nx[0].y").unwrap();
        let names: Vec<Option<String>> = parsed
            .suite()
            .iter()
            .map(|stmt| match stmt {
                ruff_python_ast::Stmt::Expr(statement) => dotted(&statement.value),
                _ => None,
            })
            .collect();
        assert_eq!(
            names,
            [
                Some("a.b.c".to_owned()),
                Some("?.execute".to_owned()),
                Some("?.y".to_owned())
            ]
        );
    }

    #[test]
    fn joins_match_the_rule_engine() {
        // The engine joins a decorator rule's prefix after the one the connector passes, with the same separators.
        let rule: CallRule = serde_json::from_value(serde_json::json!({
            "callee": "f", "kind": "http", "role": "serves", "key": "$arg1", "method": "GET"
        }))
        .unwrap();
        let parsed = parse_module(r#"f("/y")"#).unwrap();
        let Some(ruff_python_ast::Stmt::Expr(statement)) = parsed.suite().first() else {
            panic!("not an expression statement");
        };
        let Expr::Call(call) = &*statement.value else {
            panic!("not a call");
        };
        let site = CallSite {
            callee: "f",
            arguments: Some(&call.arguments),
            member: None,
        };
        let joined = join_prefixes(&ContractKind::Http, "/a", "/b");
        let Outcome::Fact(fact) = rule.apply("test", &site, Some(&joined), 1, "f") else {
            panic!("no fact");
        };
        assert_eq!(fact.key.key(), "GET /a/b/y");
        assert_eq!(join_prefixes(&ContractKind::Table, "a", "b"), "a.b");
        assert_eq!(join_prefixes(&ContractKind::Http, "", "/b"), "/b");
    }
}
