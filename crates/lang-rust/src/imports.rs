//! Imports from a syntax tree: `use`, `mod`, `extern crate`, qualified paths in code and in macros, and includes.

#[cfg(test)]
mod tests;

use std::borrow::Cow;

use gsttt_core::fact::{Import, ImportFlags, ImportKind, Line};
use gsttt_core::text::LineIndex;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{HasAttrs, HasName, HasVisibility};
use ra_ap_syntax::{AstNode, AstToken, NodeOrToken, SyntaxKind, SyntaxNode, SyntaxToken, ast};

use crate::eval::{macro_args, text_of};
use crate::syntax::{cfg_flags, file_path, inline_path, line_at, segments, text};

/// Names that start a path without naming a crate or a module.
const PRIMITIVES: &[&str] = &[
    "bool", "char", "str", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16",
    "u32", "u64", "u128", "usize",
];

/// Attribute paths that name a tool, not a crate.
const TOOLS: &[&str] = &[
    "rustfmt",
    "clippy",
    "rustdoc",
    "miri",
    "diagnostic",
    "rust_analyzer",
];

/// The imports of a file written in `edition`. `query_files` also reads sqlx's `query_file!` family, whose path starts
/// at the manifest.
pub(crate) fn read(
    root: &SyntaxNode,
    lines: &LineIndex,
    query_files: bool,
    edition: Edition,
) -> Vec<Import> {
    let reader = Reader {
        lines,
        query_files,
        edition,
    };
    let mut imports = Vec::new();
    for node in root.descendants() {
        match node.kind() {
            SyntaxKind::USE => {
                if let Some(item) = ast::Use::cast(node) {
                    reader.use_item(&item, &mut imports);
                }
            }
            SyntaxKind::MODULE => {
                if let Some(module) = ast::Module::cast(node) {
                    reader.module(&module, &mut imports);
                }
            }
            SyntaxKind::EXTERN_CRATE => {
                if let Some(item) = ast::ExternCrate::cast(node) {
                    reader.extern_crate(&item, &mut imports);
                }
            }
            SyntaxKind::PATH => {
                if let Some(path) = ast::Path::cast(node) {
                    reader.path(&path, &mut imports);
                }
            }
            SyntaxKind::MACRO_CALL => {
                if let Some(call) = ast::MacroCall::cast(node) {
                    reader.macro_call(&call, &mut imports);
                }
            }
            SyntaxKind::ATTR => {
                if let Some(attr) = ast::Attr::cast(node) {
                    reader.derive(&attr, &mut imports);
                }
            }
            _ => {}
        }
    }
    imports.sort_by_key(|import| import.line);
    imports
}

struct Reader<'a> {
    lines: &'a LineIndex,
    query_files: bool,
    edition: Edition,
}

/// What a `use` item tells about every leaf below it.
struct UseContext {
    reexport: bool,
    flags: ImportFlags,
    /// Whether the path starts with `::`.
    rooted: bool,
}

impl Reader<'_> {
    fn import(
        &self,
        spec: String,
        kind: ImportKind,
        node: &SyntaxNode,
        flags: ImportFlags,
    ) -> Import {
        let line = line_at(self.lines, node.text_range().start());
        Self::import_at(spec, kind, line, flags)
    }

    fn import_at(spec: String, kind: ImportKind, line: Line, flags: ImportFlags) -> Import {
        let mut import = Import::new(spec, kind, line);
        import.flags = flags;
        import
    }

    fn use_item(&self, item: &ast::Use, out: &mut Vec<Import>) {
        let Some(tree) = item.use_tree() else {
            return;
        };
        let context = UseContext {
            reexport: item.visibility().is_some(),
            flags: cfg_flags(item.syntax()),
            rooted: tree
                .syntax()
                .first_token()
                .is_some_and(|token| token.kind() == SyntaxKind::COLON2),
        };
        self.use_tree(&tree, &[], &context, out);
    }

    fn use_tree(
        &self,
        tree: &ast::UseTree,
        prefix: &[String],
        cx: &UseContext,
        out: &mut Vec<Import>,
    ) {
        let mut path = prefix.to_vec();
        if let Some(own) = tree.path() {
            let Some(names) = segments(&own) else {
                return;
            };
            path.extend(names);
        }
        if let Some(list) = tree.use_tree_list() {
            for child in list.use_trees() {
                self.use_tree(&child, &path, cx, out);
            }
            return;
        }
        // `use a::b::{self}` names `a::b` itself.
        if path.len() > 1 && path.last().is_some_and(|name| name == "self") {
            path.pop();
        }
        if path.is_empty() {
            return;
        }
        let glob = tree.star_token().is_some();
        let binding = match tree.rename() {
            Some(rename) if rename.underscore_token().is_some() => None,
            Some(rename) => rename.name().map(|name| text(&name)),
            None => path.last().cloned(),
        };
        let spec = self.use_spec(tree, &path, cx.rooted);
        let kind = match (glob, cx.reexport) {
            (true, true) => ImportKind::ReExportAll,
            (false, true) => ImportKind::ReExport,
            (_, false) => ImportKind::Static,
        };
        let mut import = self.import(spec, kind, tree.syntax(), cx.flags);
        import.names = if glob {
            vec!["*".to_owned()]
        } else {
            binding.into_iter().collect()
        };
        out.push(import);
    }

    /// The specifier of a `use` leaf. A leading `::` is kept, since the resolver reads it by edition. In 2015 a `use`
    /// path that does not start with `crate`, `self` or `super` is relative to the crate root, so it is written the
    /// way `::` writes that.
    fn use_spec(&self, tree: &ast::UseTree, path: &[String], rooted: bool) -> String {
        let relative = matches!(
            path.first().map(String::as_str),
            Some("crate" | "self" | "super")
        );
        if rooted || (self.edition == Edition::Edition2015 && !relative) {
            return format!("::{}", path.join("::"));
        }
        file_path(tree.syntax(), path).join("::")
    }

    fn module(&self, module: &ast::Module, out: &mut Vec<Import>) {
        if module.item_list().is_some() {
            return;
        }
        let Some(name) = module.name() else {
            return;
        };
        let mut spec = inline_path(module.syntax());
        spec.push(text(&name));
        let start = module.mod_token().map_or_else(
            || module.syntax().text_range().start(),
            |token| token.text_range().start(),
        );
        let line = line_at(self.lines, start);
        let mut import = Self::import_at(
            spec.join("::"),
            ImportKind::Module,
            line,
            cfg_flags(module.syntax()),
        );
        import.names = path_attribute(module).into_iter().collect();
        out.push(import);
    }

    fn extern_crate(&self, item: &ast::ExternCrate, out: &mut Vec<Import>) {
        let Some(name_ref) = item.name_ref() else {
            return;
        };
        let name = text(&name_ref);
        if name == "self" {
            return;
        }
        let binding = item
            .rename()
            .and_then(|rename| rename.name())
            .map_or_else(|| name.clone(), |alias| text(&alias));
        let mut import = self.import(
            name,
            ImportKind::Static,
            item.syntax(),
            cfg_flags(item.syntax()),
        );
        import.names = vec![binding];
        out.push(import);
    }

    /// A path in code, in a type, a pattern or the name of a macro or attribute.
    fn path(&self, path: &ast::Path, out: &mut Vec<Import>) {
        let node = path.syntax();
        if node
            .parent()
            .is_some_and(|parent| parent.kind() == SyntaxKind::PATH)
        {
            return;
        }
        if node
            .ancestors()
            .any(|ancestor| matches!(ancestor.kind(), SyntaxKind::USE | SyntaxKind::VISIBILITY))
        {
            return;
        }
        let Some(names) = segments(path) else {
            return;
        };
        let in_attribute = node
            .ancestors()
            .any(|ancestor| ancestor.kind() == SyntaxKind::ATTR);
        if !qualifies(&names)
            || (in_attribute
                && names
                    .first()
                    .is_some_and(|first| TOOLS.contains(&first.as_str())))
        {
            return;
        }
        let spec = file_path(node, &names).join("::");
        out.push(self.import(spec, ImportKind::Static, node, cfg_flags(node)));
    }

    fn macro_call(&self, call: &ast::MacroCall, out: &mut Vec<Import>) {
        let Some(tree) = call.token_tree() else {
            return;
        };
        let name = call
            .path()
            .and_then(|path| segments(&path))
            .and_then(|mut names| names.pop())
            .unwrap_or_default();
        let flags = cfg_flags(call.syntax());
        // The arguments are reparsed from tokens, so only the macros that read one pay for it.
        let spec = match name.as_str() {
            "include" | "include_str" | "include_bytes" => self
                .first_argument(call)
                .and_then(|arg| text_of(&arg, true, self.edition)),
            name if self.query_files && name.starts_with("query_file") => self
                .first_argument(call)
                .and_then(|arg| text_of(&arg, false, self.edition))
                .map(|file| format!("${{CARGO_MANIFEST_DIR}}/{file}")),
            _ => None,
        };
        if let Some(spec) = spec {
            out.push(self.import(spec, ImportKind::Include, call.syntax(), flags));
        }
        let tokens = significant_tokens(tree.syntax());
        if name == "cfg_if" {
            self.macro_modules(call, &tokens, out);
        }
        self.token_paths(tree.syntax(), &tokens, flags, out);
    }

    fn first_argument(&self, call: &ast::MacroCall) -> Option<ast::Expr> {
        macro_args(call, self.edition).into_iter().next()
    }

    /// `mod name;` inside a `cfg_if!` block, whose items the parser leaves as tokens. Such a module is conditional,
    /// whatever the macro call itself sits under.
    fn macro_modules(&self, call: &ast::MacroCall, tokens: &[SyntaxToken], out: &mut Vec<Import>) {
        for window in tokens.windows(3) {
            let [keyword, name, end] = window else {
                continue;
            };
            if keyword.kind() != SyntaxKind::MOD_KW
                || name.kind() != SyntaxKind::IDENT
                || end.kind() != SyntaxKind::SEMICOLON
            {
                continue;
            }
            let mut spec = inline_path(call.syntax());
            spec.push(name.text().to_owned());
            out.push(Self::import_at(
                spec.join("::"),
                ImportKind::Module,
                line_at(self.lines, keyword.text_range().start()),
                cfg_flags(call.syntax()) | ImportFlags::OPTIONAL,
            ));
        }
    }

    /// `#[derive(serde::Serialize)]`, and the same inside `#[cfg_attr(.., derive(..))]`, name a crate in a token tree.
    fn derive(&self, attr: &ast::Attr, out: &mut Vec<Import>) {
        if !matches!(attr.simple_name().as_deref(), Some("derive" | "cfg_attr")) {
            return;
        }
        let tokens = significant_tokens(attr.syntax());
        let flags = cfg_flags(attr.syntax());
        for (index, token) in tokens.iter().enumerate() {
            let opens = tokens
                .get(index + 1)
                .is_some_and(|open| open.kind() == SyntaxKind::L_PAREN);
            if token.kind() != SyntaxKind::IDENT || token.text() != "derive" || !opens {
                continue;
            }
            let inside: Vec<SyntaxToken> = tokens
                .iter()
                .skip(index + 2)
                .scan(1_usize, |depth, token| {
                    match token.kind() {
                        SyntaxKind::L_PAREN => *depth += 1,
                        SyntaxKind::R_PAREN => *depth = depth.saturating_sub(1),
                        _ => {}
                    }
                    (*depth > 0).then(|| token.clone())
                })
                .collect();
            self.token_paths(attr.syntax(), &inside, flags, out);
        }
    }

    /// The qualified paths among `tokens`, the arguments of a macro or an attribute, which are not parsed. `context`
    /// is the node they are written in.
    fn token_paths(
        &self,
        context: &SyntaxNode,
        tokens: &[SyntaxToken],
        flags: ImportFlags,
        out: &mut Vec<Import>,
    ) {
        let mut index = 0;
        while let Some(token) = tokens.get(index) {
            let after_access = index
                .checked_sub(1)
                .and_then(|before| tokens.get(before))
                .is_some_and(|before| {
                    matches!(before.kind(), SyntaxKind::DOT | SyntaxKind::DOLLAR)
                });
            if after_access || !starts_segment(token.kind()) {
                index += 1;
                continue;
            }
            let mut names = vec![token.text().to_owned()];
            let mut next = index + 1;
            while let Some(width) = separator_width(tokens, next)
                && let Some(segment) = tokens.get(next + width)
                && starts_segment(segment.kind())
            {
                names.push(segment.text().to_owned());
                next += width + 1;
            }
            if qualifies(&names) {
                let line = line_at(self.lines, token.text_range().start());
                out.push(Self::import_at(
                    file_path(context, &names).join("::"),
                    ImportKind::Static,
                    line,
                    flags,
                ));
            }
            index = next;
        }
    }
}

/// How many tokens the `::` at `index` takes: one `COLON2`, or two touching `COLON`s, which is how a token tree keeps
/// the separator. `None` when the token is no separator.
fn separator_width(tokens: &[SyntaxToken], index: usize) -> Option<usize> {
    let first = tokens.get(index)?;
    match first.kind() {
        SyntaxKind::COLON2 => Some(1),
        SyntaxKind::COLON => {
            let second = tokens.get(index + 1)?;
            (second.kind() == SyntaxKind::COLON
                && first.text_range().end() == second.text_range().start())
            .then_some(2)
        }
        _ => None,
    }
}

fn starts_segment(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::IDENT | SyntaxKind::CRATE_KW | SyntaxKind::SELF_KW | SyntaxKind::SUPER_KW
    )
}

/// Whether a path of a module or a crate is meant: two or more segments, starting with `crate`, `self`, `super` or a
/// lower case name that is no primitive type. `Vec::new` and `u32::MAX` are not.
fn qualifies(names: &[String]) -> bool {
    let Some(first) = names.first() else {
        return false;
    };
    names.len() >= 2
        && (matches!(first.as_str(), "crate" | "self" | "super")
            || (first.starts_with(|ch: char| ch.is_lowercase() || ch == '_')
                && !PRIMITIVES.contains(&first.as_str())))
}

/// The tokens below `node` that are not whitespace or comments, in order.
fn significant_tokens(node: &SyntaxNode) -> Vec<SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .filter(|token| !token.kind().is_trivia())
        .collect()
}

/// The file named by `#[path = "..."]` on a module, written as a plain or a raw string, or inside
/// `#[cfg_attr(.., path = "..")]`. Which `cfg` the latter holds under is not evaluated, so the first one wins.
fn path_attribute(module: &ast::Module) -> Option<String> {
    module.attrs().find_map(|attr| {
        if !matches!(attr.simple_name().as_deref(), Some("path" | "cfg_attr")) {
            return None;
        }
        significant_tokens(attr.syntax())
            .windows(3)
            .find_map(|window| {
                let [name, equals, value] = window else {
                    return None;
                };
                let given = name.kind() == SyntaxKind::IDENT
                    && name.text() == "path"
                    && equals.kind() == SyntaxKind::EQ;
                if !given {
                    return None;
                }
                ast::String::cast(value.clone())?
                    .value()
                    .ok()
                    .map(Cow::into_owned)
            })
    })
}
