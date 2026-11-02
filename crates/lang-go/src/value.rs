//! The text an expression stands for, as far as the file tells: literals, concatenations, `fmt.Sprintf` and the constants a name is bound to.

use tree_sitter::Node;

use crate::file::{FileCx, Shape};
use crate::scope::Scopes;
use crate::syntax::{named_children, text_of};

/// The text a computed part of a value is written as, the way every connector hands it to the rules.
pub(crate) const COMPUTED: &str = "${}";

/// How many names a value may be followed through. A constant that names itself, or a chain longer than anyone writes, is not text.
const MAX_HOPS: usize = 16;

/// How many expressions one read of a value may visit. A constant built from another twice, `b = a + a`, `c = b + b` and so on, doubles the work at every level, so that sixteen levels ask for more reads than any path needs; past this the value is no text.
const MAX_WORK: usize = 4096;

/// Reads the text of expressions in the scope the walk has reached.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Reader<'r, 'a> {
    pub(crate) cx: &'r FileCx<'a>,
    pub(crate) scopes: &'r Scopes<'a>,
}

impl<'a> Reader<'_, 'a> {
    /// The text of the expression, with each computed part written as `${}`. `None` when the expression is not text at all: a bare variable, a call, a number.
    pub(crate) fn text(&self, node: Node<'a>) -> Option<String> {
        self.read(node, 0, 0, &mut { MAX_WORK })
    }

    fn read(&self, node: Node<'a>, depth: usize, hops: usize, work: &mut usize) -> Option<String> {
        if depth > crate::syntax::MAX_DEPTH {
            return None;
        }
        *work = work.checked_sub(1)?;
        match node.kind() {
            "interpreted_string_literal" => decode(text_of(node, self.cx.source)),
            "raw_string_literal" => {
                let raw = text_of(node, self.cx.source);
                let inner = raw.get(1..raw.len().checked_sub(1)?)?;
                Some(inner.replace('\r', ""))
            }
            "parenthesized_expression" => self.read(node.named_child(0)?, depth + 1, hops, work),
            "binary_expression" => {
                if node.child_by_field_name("operator")?.kind() != "+" {
                    return None;
                }
                let left = self.read(node.child_by_field_name("left")?, depth + 1, hops, work);
                let right = self.read(node.child_by_field_name("right")?, depth + 1, hops, work);
                match (left, right) {
                    (None, None) => None,
                    (left, right) => Some(format!(
                        "{}{}",
                        left.as_deref().unwrap_or(COMPUTED),
                        right.as_deref().unwrap_or(COMPUTED)
                    )),
                }
            }
            "identifier" => self.name(text_of(node, self.cx.source), depth, hops, work),
            "call_expression" => self.sprintf(node, depth, hops, work),
            _ => None,
        }
    }

    /// The text of a name: what the nearest scope that binds it says, otherwise what the file declares it as.
    fn name(&self, name: &str, depth: usize, hops: usize, work: &mut usize) -> Option<String> {
        if hops >= MAX_HOPS {
            return None;
        }
        if let Some(binding) = self.scopes.find(name) {
            return binding.text.clone();
        }
        let value = self.cx.values.get(name)?;
        self.read(*value, depth + 1, hops + 1, work)
    }

    /// `fmt.Sprintf(template, arguments…)` with each verb replaced by its argument's text or `${}`.
    fn sprintf(
        &self,
        call: Node<'a>,
        depth: usize,
        hops: usize,
        work: &mut usize,
    ) -> Option<String> {
        let function = call.child_by_field_name("function")?;
        if function.kind() != "selector_expression" || !self.is_fmt_sprintf(function) {
            return None;
        }
        let arguments = named_children(call.child_by_field_name("arguments")?);
        let template = self.read(*arguments.first()?, depth + 1, hops, work)?;
        let mut rest = arguments.iter().skip(1);
        let mut out = String::with_capacity(template.len());
        let mut chars = template.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '%' {
                out.push(c);
                continue;
            }
            // Flags, width and precision come before the verb letter.
            while chars.peek().is_some_and(|next| {
                matches!(
                    next,
                    '+' | '-' | '#' | ' ' | '0'..='9' | '.' | '*' | '[' | ']'
                )
            }) {
                chars.next();
            }
            match chars.next() {
                Some('%') => out.push('%'),
                Some(_) => {
                    let argument = rest
                        .next()
                        .and_then(|node| self.argument_text(*node, depth + 1, hops, work));
                    out.push_str(argument.as_deref().unwrap_or(COMPUTED));
                }
                None => break,
            }
        }
        Some(out)
    }

    /// An argument of `Sprintf`: text, or a number written out.
    fn argument_text(
        &self,
        node: Node<'a>,
        depth: usize,
        hops: usize,
        work: &mut usize,
    ) -> Option<String> {
        match node.kind() {
            "int_literal" => Some(text_of(node, self.cx.source).to_owned()),
            _ => self.read(node, depth, hops, work),
        }
    }

    fn is_fmt_sprintf(&self, selector: Node<'a>) -> bool {
        let source = self.cx.source;
        let (Some(operand), Some(field)) = (
            selector.child_by_field_name("operand"),
            selector.child_by_field_name("field"),
        ) else {
            return false;
        };
        if text_of(field, source) != "Sprintf" || operand.kind() != "identifier" {
            return false;
        }
        let qualifier = text_of(operand, source);
        self.scopes.find(qualifier).is_none()
            && self.cx.import_named(qualifier).is_some_and(|index| {
                self.cx
                    .imports
                    .get(index)
                    .is_some_and(|import| import.shape == Shape::Named && import.spec == "fmt")
            })
    }
}

/// The value of a Go interpreted string literal, quotes included in `literal`.
fn decode(literal: &str) -> Option<String> {
    let inner = literal.get(1..literal.len().checked_sub(1)?)?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            'a' => out.push('\u{7}'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'v' => out.push('\u{b}'),
            '\\' => out.push('\\'),
            '\'' => out.push('\''),
            '"' => out.push('"'),
            'x' => out.push(char::from(u8::try_from(hex(&mut chars, 2)?).ok()?)),
            'u' => out.push(char::from_u32(hex(&mut chars, 4)?)?),
            'U' => out.push(char::from_u32(hex(&mut chars, 8)?)?),
            first @ '0'..='7' => {
                let mut value = first.to_digit(8)?;
                for _ in 0..2 {
                    value = value * 8 + chars.next()?.to_digit(8)?;
                }
                out.push(char::from(u8::try_from(value).ok()?));
            }
            _ => return None,
        }
    }
    Some(out)
}

/// The number the next `digits` hexadecimal digits write.
fn hex(chars: &mut std::str::Chars<'_>, digits: usize) -> Option<u32> {
    let mut value = 0u32;
    for _ in 0..digits {
        value = value
            .checked_mul(16)?
            .checked_add(chars.next()?.to_digit(16)?)?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn escapes_of_an_interpreted_string_are_decoded() {
        assert_eq!(
            decode(r#""a\tb\n\"c\"\\""#).as_deref(),
            Some("a\tb\n\"c\"\\")
        );
        assert_eq!(decode(r#""\x41é\101""#).as_deref(), Some("Aé\u{41}"));
        assert_eq!(decode(r#""\q""#), None);
    }
}
