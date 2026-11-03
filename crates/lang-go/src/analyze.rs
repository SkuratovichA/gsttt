//! The declarations of a file, what each one uses, and the imports with the names read from each.

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::fact::{
    ContractFact, Declared, Embedded, Import, ImportFlags, ImportKind, Line, MEMBER_SEPARATOR,
    UnresolvedSite,
};
use tree_sitter::Node;

use crate::file::{FileCx, Shape};
use crate::files::NO_PACKAGE;
use crate::syntax::{end_line_of, field_text, fields, line_of, named_children, text_of};
use crate::walk::Walk;

/// What a walk over a file found.
#[derive(Debug, Default)]
pub(crate) struct Analysis {
    pub(crate) declared: Vec<Declared>,
    pub(crate) on_load: Vec<String>,
    pub(crate) exports: Vec<String>,
    /// The name in the package clause, or [`NO_PACKAGE`] for a file that `//go:build ignore` leaves out of every build.
    pub(crate) package: String,
    pub(crate) imports: Vec<Import>,
    pub(crate) contracts: Vec<ContractFact>,
    pub(crate) embedded: Vec<Embedded>,
    pub(crate) unresolved: Vec<UnresolvedSite>,
    /// Whether some part of the tree nested too deep to be read.
    pub(crate) cut: bool,
}

/// The lines and uses of a declaration, merged when a name is declared twice.
struct Span {
    line: Line,
    end: Line,
    uses: BTreeSet<String>,
}

/// Collects the declarations of a file; a name declared twice, as functions behind different build tags are, is one declaration.
struct Declarations {
    spans: BTreeMap<String, Span>,
}

impl Declarations {
    fn add(&mut self, name: String, line: Line, end: Line, mut uses: BTreeSet<String>) {
        uses.remove(&name);
        match self.spans.get_mut(&name) {
            Some(span) => {
                span.line = span.line.min(line);
                span.end = span.end.max(end);
                span.uses.append(&mut uses);
            }
            None => {
                self.spans.insert(name, Span { line, end, uses });
            }
        }
    }

    fn finish(self) -> Vec<Declared> {
        let mut declared: Vec<Declared> = self
            .spans
            .into_iter()
            .map(|(name, span)| Declared {
                name,
                line: span.line,
                end: span.end,
                uses: span.uses.into_iter().collect(),
            })
            .collect();
        declared.sort_by(|a, b| (a.line, &a.name).cmp(&(b.line, &b.name)));
        declared
    }
}

/// Reads the whole tree of a file whose context is `cx`.
pub(crate) fn analyze<'a>(cx: &FileCx<'a>, root: Node<'a>) -> Analysis {
    let mut walk = Walk::new(cx);
    let mut declarations = Declarations {
        spans: BTreeMap::new(),
    };
    let mut package_line = 1;
    let mut package = String::new();
    let mut ignored = false;
    for child in named_children(root) {
        match child.kind() {
            "comment" if package.is_empty() => {
                ignored |= text_of(child, cx.source).trim_end() == "//go:build ignore";
            }
            "package_clause" => {
                package_line = line_of(child);
                package = child
                    .named_child(0)
                    .map(|name| text_of(name, cx.source).to_owned())
                    .unwrap_or_default();
            }
            "function_declaration" => function(&mut walk, &mut declarations, child),
            "method_declaration" => method(&mut walk, &mut declarations, child),
            "type_declaration" => {
                for spec in named_children(child) {
                    type_spec(&mut walk, &mut declarations, spec);
                }
            }
            "const_declaration" | "var_declaration" => {
                for spec in named_children(child) {
                    match spec.kind() {
                        "const_spec" | "var_spec" => value_spec(&mut walk, &mut declarations, spec),
                        "var_spec_list" => {
                            for inner in named_children(spec) {
                                value_spec(&mut walk, &mut declarations, inner);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    let imports = imports_of(cx, &walk, package_line);
    Analysis {
        declared: declarations.finish(),
        on_load: walk.on_load.into_iter().collect(),
        exports: cx
            .top
            .iter()
            .chain(cx.receivers.keys())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|name| (*name).to_owned())
            .collect(),
        package: if ignored {
            NO_PACKAGE.to_owned()
        } else {
            package
        },
        imports,
        contracts: walk.contracts,
        embedded: walk.embedded,
        unresolved: walk.unresolved,
        cut: walk.cut,
    }
}

fn function<'a>(walk: &mut Walk<'_, 'a>, declarations: &mut Declarations, node: Node<'a>) {
    let Some(name) = field_text(node, "name", walk.cx.source) else {
        return;
    };
    walk.begin();
    walk.scopes.push_scope();
    walk.signature(node);
    if let Some(body) = node.child_by_field_name("body") {
        walk.visit(body);
    }
    walk.scopes.pop_scope();
    let uses = walk.take_uses();
    if name == "init" {
        walk.on_load.extend(uses);
        return;
    }
    declarations.add(name.to_owned(), line_of(node), end_line_of(node), uses);
}

fn method<'a>(walk: &mut Walk<'_, 'a>, declarations: &mut Declarations, node: Node<'a>) {
    let source = walk.cx.source;
    let (Some(name), Some(receiver)) = (
        field_text(node, "name", source),
        node.child_by_field_name("receiver"),
    ) else {
        return;
    };
    let class = named_children(receiver)
        .into_iter()
        .find_map(|parameter| parameter.child_by_field_name("type"))
        .and_then(|ty| walk.cx.head_of(ty, &|_| false))
        .map(|head| head.name);
    walk.begin();
    walk.scopes.push_scope();
    walk.signature(node);
    if let Some(body) = node.child_by_field_name("body") {
        walk.visit(body);
    }
    walk.scopes.pop_scope();
    let uses = walk.take_uses();
    let name = match class {
        Some(class) => format!("{class}{MEMBER_SEPARATOR}{name}"),
        None => name.to_owned(),
    };
    declarations.add(name, line_of(node), end_line_of(node), uses);
}

fn type_spec<'a>(walk: &mut Walk<'_, 'a>, declarations: &mut Declarations, spec: Node<'a>) {
    let source = walk.cx.source;
    if !matches!(spec.kind(), "type_spec" | "type_alias") {
        return;
    }
    let (Some(name), Some(ty)) = (
        field_text(spec, "name", source),
        spec.child_by_field_name("type"),
    ) else {
        return;
    };
    walk.begin();
    walk.scopes.push_scope();
    if let Some(list) = spec.child_by_field_name("type_parameters") {
        walk.type_parameters(list);
    }
    match ty.kind() {
        "struct_type" => {
            let own = walk.take_uses();
            declarations.add(name.to_owned(), line_of(spec), end_line_of(spec), own);
            struct_fields(walk, declarations, name, ty);
        }
        "interface_type" => {
            interface_members(walk, declarations, name, ty);
            let own = walk.take_uses();
            declarations.add(name.to_owned(), line_of(spec), end_line_of(spec), own);
        }
        _ => {
            walk.visit_type(ty);
            let own = walk.take_uses();
            declarations.add(name.to_owned(), line_of(spec), end_line_of(spec), own);
        }
    }
    walk.scopes.pop_scope();
}

/// A struct carries no uses from the types of its fields: each field is a declaration of its own.
fn struct_fields<'a>(
    walk: &mut Walk<'_, 'a>,
    declarations: &mut Declarations,
    class: &str,
    ty: Node<'a>,
) {
    let source = walk.cx.source;
    for list in named_children(ty) {
        for field in named_children(list) {
            if field.kind() != "field_declaration" {
                continue;
            }
            let Some(field_type) = field.child_by_field_name("type") else {
                continue;
            };
            let names: Vec<String> = fields(field, "name")
                .into_iter()
                .map(|name| text_of(name, source).to_owned())
                .collect();
            let names = if names.is_empty() {
                // An embedded field is named after its type.
                walk.cx
                    .head_of(field_type, &|_| false)
                    .map(|head| head.name)
                    .into_iter()
                    .collect()
            } else {
                names
            };
            walk.begin();
            walk.visit_type(field_type);
            let uses = walk.take_uses();
            for name in names {
                declarations.add(
                    format!("{class}{MEMBER_SEPARATOR}{name}"),
                    line_of(field),
                    end_line_of(field),
                    uses.clone(),
                );
            }
        }
    }
}

/// The methods of an interface are declarations of their own; the embedded interfaces and unions stay with the interface.
fn interface_members<'a>(
    walk: &mut Walk<'_, 'a>,
    declarations: &mut Declarations,
    class: &str,
    ty: Node<'a>,
) {
    let source = walk.cx.source;
    for member in named_children(ty) {
        match member.kind() {
            "method_elem" => {
                let Some(name) = field_text(member, "name", source) else {
                    continue;
                };
                let carried = walk.take_uses();
                walk.scopes.push_scope();
                walk.signature(member);
                walk.scopes.pop_scope();
                let uses = walk.take_uses();
                walk.uses_extend(carried);
                declarations.add(
                    format!("{class}{MEMBER_SEPARATOR}{name}"),
                    line_of(member),
                    end_line_of(member),
                    uses,
                );
            }
            _ => walk.visit(member),
        }
    }
}

/// `const` and `var` at the top level, a declaration for each name.
fn value_spec<'a>(walk: &mut Walk<'_, 'a>, declarations: &mut Declarations, spec: Node<'a>) {
    let source = walk.cx.source;
    let names = fields(spec, "name");
    let ty = spec.child_by_field_name("type");
    let values = spec
        .child_by_field_name("value")
        .map(named_children)
        .unwrap_or_default();
    let (line, end) = (line_of(spec), end_line_of(spec));
    let paired = names.len() == values.len();
    let mut shared = BTreeSet::new();
    if !paired {
        walk.begin();
        read_value(walk, ty, &values);
        shared = walk.take_uses();
    }
    for (index, name) in names.iter().enumerate() {
        let text = text_of(*name, source);
        let uses = if paired {
            walk.begin();
            read_value(
                walk,
                ty,
                values
                    .get(index)
                    .map(std::slice::from_ref)
                    .unwrap_or_default(),
            );
            walk.take_uses()
        } else {
            shared.clone()
        };
        if text == "_" {
            walk.on_load.extend(uses);
        } else {
            declarations.add(text.to_owned(), line, end, uses);
        }
    }
}

/// Reads a type and the values of a declaration; the values run as the package loads.
fn read_value<'a>(walk: &mut Walk<'_, 'a>, ty: Option<Node<'a>>, values: &[Node<'a>]) {
    if let Some(ty) = ty {
        walk.visit_type(ty);
    }
    walk.as_load(|walk| {
        for value in values {
            walk.visit(*value);
        }
    });
}

/// The imports of the file with the names read from each, and the names read from the package itself.
fn imports_of(cx: &FileCx<'_>, walk: &Walk<'_, '_>, package_line: Line) -> Vec<Import> {
    let mut imports = Vec::new();
    for (index, info) in cx.imports.iter().enumerate() {
        let import = match info.shape {
            Shape::Blank => Import::new(&info.spec, ImportKind::SideEffect, info.line),
            Shape::Dot => {
                let mut import = Import::new(&info.spec, ImportKind::Static, info.line);
                import.names = walk.package_names();
                import.exported = import.names.clone();
                import.flags = ImportFlags::PACKAGE;
                import
            }
            Shape::Named => {
                let mut import = Import::new(&info.spec, ImportKind::Static, info.line);
                import.flags = ImportFlags::PACKAGE;
                if let Some(names) = walk.import_names.get(index) {
                    import.names = names.iter().cloned().collect();
                    import.exported = names
                        .iter()
                        .map(|name| format!("{}{MEMBER_SEPARATOR}{name}", info.qualifier))
                        .collect();
                }
                import
            }
        };
        imports.push(import);
    }
    let free = walk.package_names();
    if !free.is_empty() {
        let mut same = Import::new(".", ImportKind::Static, package_line);
        same.names = free;
        same.exported.clone_from(&same.names);
        same.flags = ImportFlags::PACKAGE;
        imports.push(same);
    }
    imports
}
