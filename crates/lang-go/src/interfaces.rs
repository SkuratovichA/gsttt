//! What one Go file adds to the link of method sets: the methods it declares, the interfaces and the embedded types, written down as signatures that two files can compare.

mod link;

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use crate::syntax::{MAX_DEPTH, fields, named_children, text_of};

pub(crate) use link::link;

/// The type text of each parameter and of each result of a method, normalised by [`normal_type`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Sig {
    pub(crate) params: Vec<String>,
    pub(crate) results: Vec<String>,
}

/// A method and its signature.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Method {
    pub(crate) name: String,
    pub(crate) sig: Sig,
}

/// A method declared with a receiver of the type `receiver`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MethodFact {
    pub(crate) receiver: String,
    #[serde(flatten)]
    pub(crate) method: Method,
}

/// An interface of the file with its own methods and what it embeds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct InterfaceFact {
    pub(crate) name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) methods: Vec<Method>,
    /// The embedded interfaces as the file spells them: `B`, `pkg.B`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) embeds: Vec<String>,
    /// Whether the interface has a union or a constraint term, so no value has it as its type.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) open: bool,
}

/// What one file adds to the link; empty for a file with no methods and no interfaces.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoExtra {
    /// Methods the file declares, by receiver type.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) methods: Vec<MethodFact>,
    /// Interfaces the file declares.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) interfaces: Vec<InterfaceFact>,
    /// Struct types of the file and each type they embed, as spelled: `(T, "Base")`, `(T, "pkg.Base")`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) embeds: Vec<(String, String)>,
    /// Types declared with type parameters, whose methods compare by the number of parameters and results only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) generic: Vec<String>,
}

/// The signature of a method declaration, of an interface method element or of a function type: its parameters and results.
pub(crate) fn sig_of(node: Node<'_>, source: &str) -> Sig {
    sig_at(node, source, 0)
}

fn sig_at(node: Node<'_>, source: &str, depth: usize) -> Sig {
    let params = node
        .child_by_field_name("parameters")
        .map(|list| entries(list, source, depth))
        .unwrap_or_default();
    let results = match node.child_by_field_name("result") {
        Some(result) if result.kind() == "parameter_list" => entries(result, source, depth),
        Some(result) => vec![shape_of(result, source, depth)],
        None => Vec::new(),
    };
    Sig { params, results }
}

/// One entry per parameter: `a, b int` is two, an unnamed parameter is one, and a variadic one keeps its `...`.
fn entries(list: Node<'_>, source: &str, depth: usize) -> Vec<String> {
    let mut found = Vec::new();
    for declaration in named_children(list) {
        let variadic = match declaration.kind() {
            "parameter_declaration" => false,
            "variadic_parameter_declaration" => true,
            _ => continue,
        };
        let Some(ty) = declaration.child_by_field_name("type") else {
            continue;
        };
        let mut text = shape_of(ty, source, depth);
        if variadic {
            text.insert_str(0, "...");
        }
        let count = fields(declaration, "name").len().max(1);
        found.extend(std::iter::repeat_n(text, count));
    }
    found
}

/// The text of a type as a comparison of shapes reads it: what [`normal_type`] leaves, with a function type anywhere inside it reduced to its types, because `func(path string) error` and `func(p string) error` are one type.
fn shape_of(ty: Node<'_>, source: &str, depth: usize) -> String {
    let text = text_of(ty, source);
    // A type nested past the limit is read as text: a hostile file must not overflow the stack.
    if depth > MAX_DEPTH || !text.contains("func") {
        return normal_type(text);
    }
    if ty.kind() == "function_type" {
        let sig = sig_at(ty, source, depth + 1);
        let results = match sig.results.as_slice() {
            [] => String::new(),
            [one] => one.clone(),
            many => format!("({})", many.join(",")),
        };
        return format!("func({}){results}", sig.params.join(","));
    }
    let mut out = String::new();
    let mut at = ty.start_byte();
    for child in named_children(ty) {
        out.push_str(source.get(at..child.start_byte()).unwrap_or_default());
        out.push_str(&shape_of(child, source, depth + 1));
        at = child.end_byte();
    }
    out.push_str(source.get(at..ty.end_byte()).unwrap_or_default());
    normal_type(&out)
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// A type as text that two spellings of one type share: whitespace gone but between two words, package qualifiers dropped at any depth (`[]pkg.T` is `[]T`), and `interface{}` written `any`.
pub(crate) fn normal_type(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut spaced = false;
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            spaced = true;
            continue;
        }
        if !is_word(c) {
            out.push(c);
            spaced = false;
            continue;
        }
        let mut word = String::from(c);
        while let Some(&next) = chars.peek().filter(|&&next| is_word(next)) {
            word.push(next);
            chars.next();
        }
        // A word followed by a single dot is a package qualifier: the dot is not `...`.
        if drop_qualifier_dot(&mut chars) {
            continue;
        }
        if spaced && out.chars().next_back().is_some_and(is_word) {
            out.push(' ');
        }
        spaced = false;
        if word == "interface" && empty_braces(&mut chars) {
            out.push_str("any");
        } else {
            out.push_str(&word);
        }
    }
    out
}

/// Consumes a single `.` after blanks, and the blanks after it, when a name follows that is no more dots.
fn drop_qualifier_dot(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    let mut ahead = chars.clone();
    while ahead.next_if(|c| c.is_whitespace()).is_some() {}
    if ahead.next_if_eq(&'.').is_none() {
        return false;
    }
    while ahead.next_if(|c| c.is_whitespace()).is_some() {}
    if ahead.peek().is_none_or(|&next| next == '.') {
        return false;
    }
    *chars = ahead;
    true
}

/// Consumes `{` and `}` with only whitespace around and between them, and says whether they were there.
fn empty_braces(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    let mut ahead = chars.clone();
    let skip = |ahead: &mut std::iter::Peekable<std::str::Chars<'_>>, want: char| {
        while ahead.peek().is_some_and(|c| c.is_whitespace()) {
            ahead.next();
        }
        ahead.next_if_eq(&want).is_some()
    };
    if skip(&mut ahead, '{') && skip(&mut ahead, '}') {
        *chars = ahead;
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn qualifiers_and_blanks_go_and_words_keep_apart() {
        assert_eq!(normal_type("[]pkg.T"), "[]T");
        assert_eq!(normal_type("map[string]a.b.T"), "map[string]T");
        assert_eq!(normal_type("*  pkg.T"), "*T");
        assert_eq!(normal_type("func(a pkg.X) pkg.Y"), "func(a X)Y");
        assert_eq!(normal_type("interface { }"), "any");
        assert_eq!(normal_type("[]interface{}"), "[]any");
        assert_eq!(normal_type("xinterface{}"), "xinterface{}");
        assert_eq!(normal_type("...int"), "...int");
    }

    proptest! {
        #[test]
        fn normalising_twice_is_normalising_once(text in "[a-zA-Z0-9_.*\\[\\]{}() ,]{0,40}") {
            let once = normal_type(&text);
            prop_assert_eq!(normal_type(&once), once);
        }

        #[test]
        fn a_space_only_stands_between_two_words(text in "[a-zA-Z0-9_.*\\[\\]{}() ,]{0,40}") {
            let out = normal_type(&text);
            let chars: Vec<char> = out.chars().collect();
            for (at, &c) in chars.iter().enumerate() {
                if c == ' ' {
                    let before = at.checked_sub(1).and_then(|i| chars.get(i));
                    prop_assert!(before.is_some_and(|&b| is_word(b)));
                    prop_assert!(chars.get(at + 1).is_some_and(|&a| is_word(a)));
                }
            }
        }
    }
}
