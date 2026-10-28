//! Small readers over the syntax tree that the other modules share.

use gsttt_core::fact::{ImportFlags, Line};
use gsttt_core::text::LineIndex;
use ra_ap_syntax::ast::HasName;
use ra_ap_syntax::{AstNode, SyntaxNode, TextSize, ast};

/// The 1-based line that holds `offset`.
pub(crate) fn line_at(lines: &LineIndex, offset: TextSize) -> Line {
    lines.line(u32::from(offset))
}

/// The text of a node.
pub(crate) fn text(node: &impl AstNode) -> String {
    node.syntax().text().to_string()
}

/// The names along `path`, or `None` when it starts with a qualified type such as `<T as Trait>::f`.
pub(crate) fn segments(path: &ast::Path) -> Option<Vec<String>> {
    path.segments()
        .map(|segment| segment.name_ref().map(|name| text(&name)))
        .collect()
}

/// What the `#[cfg(...)]` attributes on a node and on everything around it say about it: a test, or optional.
pub(crate) fn cfg_flags(node: &SyntaxNode) -> ImportFlags {
    let mut flags = ImportFlags::NONE;
    for ancestor in node.ancestors() {
        for attr in ancestor.children().filter_map(ast::Attr::cast) {
            let Some(predicate) = cfg_predicate(&attr) else {
                continue;
            };
            flags = flags
                | if requires_test(&predicate) {
                    ImportFlags::TEST_ONLY
                } else {
                    ImportFlags::OPTIONAL
                };
        }
    }
    flags
}

/// The predicate of a `#[cfg(predicate)]` attribute.
fn cfg_predicate(attr: &ast::Attr) -> Option<String> {
    let source = text(attr);
    let inner = source
        .trim()
        .trim_start_matches('#')
        .trim_start_matches('!')
        .trim()
        .strip_prefix('[')?
        .strip_suffix(']')?
        .trim();
    let predicate = inner
        .strip_prefix("cfg")?
        .trim_start()
        .strip_prefix('(')?
        .strip_suffix(')')?;
    Some(predicate.trim().to_owned())
}

/// Whether the predicate holds only when compiling tests: `test`, or an `all(...)` that lists it.
fn requires_test(predicate: &str) -> bool {
    let predicate = predicate.trim();
    if predicate == "test" {
        return true;
    }
    predicate
        .strip_prefix("all(")
        .and_then(|rest| rest.strip_suffix(')'))
        .is_some_and(|inner| split_top_level(inner).into_iter().any(requires_test))
}

/// The comma separated parts of `text` that are not inside parentheses.
fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.extend(text.get(start..index));
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.extend(text.get(start..));
    parts
}

/// The names of the inline modules around a node, outermost first.
pub(crate) fn inline_path(node: &SyntaxNode) -> Vec<String> {
    let mut names: Vec<String> = node
        .ancestors()
        .skip(1)
        .filter_map(ast::Module::cast)
        .filter(|module| module.item_list().is_some())
        .filter_map(|module| module.name().map(|name| text(&name)))
        .collect();
    names.reverse();
    names
}

/// A path written inside inline modules, made relative to the module of the file: `super::X` in `mod a` is `self::X`
/// of the file, and `self::X` in `mod a` is `self::a::X`.
pub(crate) fn file_relative(segments: &[String], inline: &[String]) -> Vec<String> {
    let rest = |skip: usize| segments.iter().skip(skip).cloned();
    match segments.first().map(String::as_str) {
        _ if inline.is_empty() => segments.to_vec(),
        Some("self") => ["self".to_owned()]
            .into_iter()
            .chain(inline.iter().cloned())
            .chain(rest(1))
            .collect(),
        Some("super") => {
            let climbs = segments.iter().take_while(|name| *name == "super").count();
            if climbs <= inline.len() {
                ["self".to_owned()]
                    .into_iter()
                    .chain(inline.iter().take(inline.len() - climbs).cloned())
                    .chain(rest(climbs))
                    .collect()
            } else {
                std::iter::repeat_n("super".to_owned(), climbs - inline.len())
                    .chain(rest(climbs))
                    .collect()
            }
        }
        _ => segments.to_vec(),
    }
}
