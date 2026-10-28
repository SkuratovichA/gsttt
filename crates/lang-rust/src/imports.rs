//! Imports from a syntax tree: `use`, `mod`, `extern crate`, qualified paths in code and in macros, and includes.

use gsttt_core::fact::{Import, ImportFlags, ImportKind, Line};
use gsttt_core::text::LineIndex;
use ra_ap_syntax::ast::{HasAttrs, HasName, HasVisibility};
use ra_ap_syntax::{AstNode, NodeOrToken, SyntaxKind, SyntaxNode, SyntaxToken, ast};

use crate::eval::{macro_args, text_of};
use crate::syntax::{cfg_flags, file_relative, inline_path, line_at, segments, text};

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

/// The imports of a file. `query_files` also reads sqlx's `query_file!` family, whose path starts at the manifest.
pub(crate) fn read(root: &SyntaxNode, lines: &LineIndex, query_files: bool) -> Vec<Import> {
    let reader = Reader { lines, query_files };
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
}

/// What a `use` item tells about every leaf below it.
struct UseContext<'a> {
    reexport: bool,
    flags: ImportFlags,
    inline: &'a [String],
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
        let inline = inline_path(item.syntax());
        let context = UseContext {
            reexport: item.visibility().is_some(),
            flags: cfg_flags(item.syntax()),
            inline: &inline,
        };
        self.use_tree(&tree, &[], &context, out);
    }

    fn use_tree(
        &self,
        tree: &ast::UseTree,
        prefix: &[String],
        cx: &UseContext<'_>,
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
        let spec = file_relative(&path, cx.inline).join("::");
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
        let spec = file_relative(&names, &inline_path(node)).join("::");
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
        let first = macro_args(call).into_iter().next();
        let spec = match name.as_str() {
            "include" | "include_str" | "include_bytes" => {
                first.and_then(|arg| text_of(&arg, true))
            }
            name if self.query_files && name.starts_with("query_file") => first
                .and_then(|arg| text_of(&arg, false))
                .map(|file| format!("${{CARGO_MANIFEST_DIR}}/{file}")),
            _ => None,
        };
        if let Some(spec) = spec {
            out.push(self.import(spec, ImportKind::Include, call.syntax(), flags));
        }
        self.token_paths(tree.syntax(), flags, out);
    }

    /// `#[derive(serde::Serialize)]` names a crate in a token tree.
    fn derive(&self, attr: &ast::Attr, out: &mut Vec<Import>) {
        if attr.simple_name().as_deref() != Some("derive") {
            return;
        }
        if let Some(tree) = attr
            .syntax()
            .descendants()
            .find(|node| node.kind() == SyntaxKind::TOKEN_TREE)
        {
            self.token_paths(&tree, cfg_flags(attr.syntax()), out);
        }
    }

    /// The qualified paths among the tokens of a macro's arguments, which are not parsed.
    fn token_paths(&self, tree: &SyntaxNode, flags: ImportFlags, out: &mut Vec<Import>) {
        let tokens: Vec<SyntaxToken> = tree
            .descendants_with_tokens()
            .filter_map(NodeOrToken::into_token)
            .filter(|token| !token.kind().is_trivia())
            .collect();
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
            while let Some(width) = separator_width(&tokens, next)
                && let Some(segment) = tokens.get(next + width)
                && starts_segment(segment.kind())
            {
                names.push(segment.text().to_owned());
                next += width + 1;
            }
            if qualifies(&names) {
                let line = line_at(self.lines, token.text_range().start());
                out.push(Self::import_at(
                    names.join("::"),
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

/// The file named by `#[path = "..."]` on a module.
fn path_attribute(module: &ast::Module) -> Option<String> {
    module.attrs().find_map(|attr| {
        let source = text(&attr);
        let inner = source.trim().strip_prefix("#[")?.strip_suffix(']')?.trim();
        let value = inner
            .strip_prefix("path")?
            .trim_start()
            .strip_prefix('=')?
            .trim();
        Some(value.strip_prefix('"')?.strip_suffix('"')?.to_owned())
    })
}
