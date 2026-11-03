//! What the whole file says before any code is walked: the imports and the names they are written by, the top-level declarations and the fields of the structs.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use gsttt_core::fact::Line;
use gsttt_core::rules::Pack;
use tree_sitter::Node;

use crate::names::{default_qualifier, is_predeclared, squeezed};
use crate::scope::Head;
use crate::syntax::{field_text, fields, line_of, named_children, text_of};

/// How an import spec writes the package.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    /// `"path"` or `alias "path"`: the package is used through a qualifier.
    Named,
    /// `_ "path"`: the package is loaded and nothing is read from it.
    Blank,
    /// `. "path"`: the package's exported names are the file's own.
    Dot,
}

/// One import spec of the file.
#[derive(Clone, Debug)]
pub(crate) struct ImportInfo {
    pub(crate) spec: String,
    pub(crate) line: Line,
    pub(crate) shape: Shape,
    /// The name the file writes the package by, empty when it writes none that could be found.
    pub(crate) qualifier: String,
    /// The name rules see the package by in a callee: the last segment of its path.
    pub(crate) callee_name: String,
    /// Whether the package belongs to this module or another module of the repository, whose calls no rule of a library applies to.
    pub(crate) local: bool,
}

/// The fields of one struct type of the file, by name, with the type each one is written as.
pub(crate) type Fields = BTreeMap<String, Option<Head>>;

/// Everything about a file that the walks need and do not change.
#[derive(Debug)]
pub(crate) struct FileCx<'a> {
    pub(crate) source: &'a str,
    pub(crate) imports: Vec<ImportInfo>,
    /// The names the file declares at the top level, methods and `init` aside.
    pub(crate) top: BTreeSet<&'a str>,
    /// The types the file declares methods of, with the names of those methods. They are not declared here, so a use of one still looks for its file, but the file belongs to the type's exports: `impact` asked about `Service.Charge` follows the importers of the file that holds `Charge`.
    pub(crate) receivers: BTreeMap<&'a str, BTreeSet<&'a str>>,
    /// The value of each constant and single-assignment variable the file declares at the top level.
    pub(crate) values: BTreeMap<&'a str, Node<'a>>,
    /// The fields of each struct type the file declares.
    pub(crate) structs: BTreeMap<String, Fields>,
    /// The names the file assigns to after they are declared: such a name stands for no one text.
    pub(crate) assigned: HashSet<&'a str>,
    /// The packs in force that are written for Go.
    pub(crate) packs: &'a [Pack],
}

impl<'a> FileCx<'a> {
    /// Whether the file itself declares `member` of the type `ty`: a method it holds, or a field of the struct it declares.
    pub(crate) fn declares_member(&self, ty: &str, member: &str) -> bool {
        self.receivers
            .get(ty)
            .is_some_and(|methods| methods.contains(member))
            || self
                .structs
                .get(ty)
                .is_some_and(|fields| fields.contains_key(member))
    }

    /// Reads the file from the root of its tree. `modules` are the module paths of every `go.mod` of the repository.
    pub(crate) fn build(
        source: &'a str,
        root: Node<'a>,
        packs: &'a [Pack],
        modules: &[String],
    ) -> Self {
        let survey = survey(root, source);
        let mut cx = Self {
            source,
            imports: Vec::new(),
            top: BTreeSet::new(),
            receivers: BTreeMap::new(),
            values: BTreeMap::new(),
            structs: BTreeMap::new(),
            assigned: survey.assigned,
            packs,
        };
        cx.read_imports(root, &survey.qualifiers, modules);
        for child in named_children(root) {
            cx.read_top_level(child);
        }
        cx
    }

    /// The import the qualifier `name` stands for.
    pub(crate) fn import_named(&self, name: &str) -> Option<usize> {
        if name.is_empty() {
            return None;
        }
        self.imports
            .iter()
            .position(|import| import.shape == Shape::Named && import.qualifier == name)
    }

    /// The head of the type written as `ty`, when `bound` says no local binding hides the name. A predeclared type holds nothing worth following.
    pub(crate) fn head_of(&self, ty: Node<'_>, bound: &dyn Fn(&str) -> bool) -> Option<Head> {
        let node = head_node(ty)?;
        match node.kind() {
            "type_identifier" => {
                let name = text_of(node, self.source);
                (!bound(name) && !is_predeclared(name)).then(|| Head {
                    import: None,
                    name: name.to_owned(),
                })
            }
            "qualified_type" => {
                let package = field_text(node, "package", self.source)?;
                let name = field_text(node, "name", self.source)?;
                if bound(package) {
                    return None;
                }
                Some(Head {
                    import: Some(self.import_named(package)?),
                    name: name.to_owned(),
                })
            }
            _ => None,
        }
    }

    /// How the file spells the type `head`: the qualifier of its import in front of its name.
    pub(crate) fn spell(&self, head: &Head) -> String {
        match head.import.and_then(|index| self.imports.get(index)) {
            Some(import) => format!("{}.{}", import.qualifier, head.name),
            None => head.name.clone(),
        }
    }

    fn read_imports(&mut self, root: Node<'a>, qualifiers: &BTreeSet<&'a str>, modules: &[String]) {
        let mut specs = Vec::new();
        for declaration in named_children(root) {
            if declaration.kind() != "import_declaration" {
                continue;
            }
            for child in named_children(declaration) {
                match child.kind() {
                    "import_spec" => specs.push(child),
                    "import_spec_list" => specs.extend(
                        named_children(child)
                            .into_iter()
                            .filter(|spec| spec.kind() == "import_spec"),
                    ),
                    _ => {}
                }
            }
        }
        let mut claimed: HashSet<&str> = HashSet::new();
        for spec in specs {
            let Some(path) = spec
                .child_by_field_name("path")
                .and_then(|node| unquote(text_of(node, self.source)))
            else {
                continue;
            };
            let shape = match spec.child_by_field_name("name").map(|name| name.kind()) {
                Some("blank_identifier") => Shape::Blank,
                Some("dot") => Shape::Dot,
                _ => Shape::Named,
            };
            let alias = field_text(spec, "name", self.source).filter(|_| shape == Shape::Named);
            let callee_name = default_qualifier(path);
            let qualifier = match (shape, alias) {
                (Shape::Named, Some(alias)) => alias,
                (Shape::Named, None) => qualifier_among(callee_name, qualifiers, &claimed),
                _ => "",
            };
            if !qualifier.is_empty() {
                claimed.insert(qualifier);
            }
            self.imports.push(ImportInfo {
                spec: path.to_owned(),
                line: line_of(spec),
                shape,
                qualifier: qualifier.to_owned(),
                callee_name: callee_name.to_owned(),
                local: modules.iter().any(|module| is_under(path, module)),
            });
        }
    }

    fn read_top_level(&mut self, node: Node<'a>) {
        match node.kind() {
            "function_declaration" => {
                if let Some(name) = field_text(node, "name", self.source)
                    && name != "init"
                    && name != "_"
                {
                    self.top.insert(name);
                }
            }
            "method_declaration" => {
                if let Some(receiver) = node.child_by_field_name("receiver")
                    && let Some(ty) = named_children(receiver)
                        .into_iter()
                        .find_map(|parameter| parameter.child_by_field_name("type"))
                    && let Some(name) = receiver_name(ty, self.source)
                {
                    let methods = self.receivers.entry(name).or_default();
                    if let Some(method) = node.child_by_field_name("name") {
                        methods.insert(text_of(method, self.source));
                    }
                }
            }
            "type_declaration" => {
                for spec in named_children(node) {
                    self.read_type_spec(spec);
                }
            }
            "const_declaration" | "var_declaration" => {
                for spec in named_children(node) {
                    match spec.kind() {
                        "const_spec" | "var_spec" => self.read_value_spec(spec),
                        "var_spec_list" => {
                            for inner in named_children(spec) {
                                self.read_value_spec(inner);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn read_value_spec(&mut self, spec: Node<'a>) {
        let names = fields(spec, "name");
        let values = spec
            .child_by_field_name("value")
            .map(named_children)
            .unwrap_or_default();
        for (index, name) in names.iter().enumerate() {
            let text = text_of(*name, self.source);
            if text == "_" {
                continue;
            }
            self.top.insert(text);
            if names.len() == values.len()
                && !self.assigned.contains(text)
                && let Some(value) = values.get(index)
            {
                self.values.insert(text, *value);
            }
        }
    }

    fn read_type_spec(&mut self, spec: Node<'a>) {
        if !matches!(spec.kind(), "type_spec" | "type_alias") {
            return;
        }
        let Some(name) = field_text(spec, "name", self.source) else {
            return;
        };
        self.top.insert(name);
        let Some(ty) = spec.child_by_field_name("type") else {
            return;
        };
        if ty.kind() != "struct_type" {
            return;
        }
        let parameters = type_parameter_names(spec, self.source);
        let bound = |candidate: &str| parameters.contains(&candidate);
        let mut found = Fields::new();
        for list in named_children(ty) {
            for field in named_children(list) {
                if field.kind() != "field_declaration" {
                    continue;
                }
                let Some(written) = field.child_by_field_name("type") else {
                    continue;
                };
                let head = self.head_of(written, &bound);
                let names = fields(field, "name");
                if names.is_empty() {
                    // An embedded field is a field named after its type.
                    if let Some(head) = head.clone() {
                        found.insert(head.name.clone(), Some(head));
                    }
                }
                for field_name in names {
                    found.insert(text_of(field_name, self.source).to_owned(), head.clone());
                }
            }
        }
        self.structs.insert(name.to_owned(), found);
    }
}

/// The names of the type parameters of a type declaration.
pub(crate) fn type_parameter_names<'s>(spec: Node<'_>, source: &'s str) -> Vec<&'s str> {
    let Some(list) = spec.child_by_field_name("type_parameters") else {
        return Vec::new();
    };
    named_children(list)
        .into_iter()
        .flat_map(|declaration| fields(declaration, "name"))
        .map(|name| text_of(name, source))
        .collect()
}

/// The name a package is written by among the `qualifiers` the file uses: its default name when the file uses that, otherwise the one name that is the default spelled without separators. A package no name is found for is read whole.
fn qualifier_among<'q>(
    default: &'q str,
    qualifiers: &BTreeSet<&'q str>,
    claimed: &HashSet<&str>,
) -> &'q str {
    if qualifiers.contains(default) {
        return default;
    }
    let squeezed = squeezed(default);
    qualifiers
        .iter()
        .copied()
        .find(|candidate| *candidate == squeezed && !claimed.contains(candidate))
        .unwrap_or_default()
}

/// Whether the import path `path` is the module `module` or a package below it.
pub(crate) fn is_under(path: &str, module: &str) -> bool {
    !module.is_empty()
        && path
            .strip_prefix(module)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// The path an import spec writes, without its quotes.
fn unquote(literal: &str) -> Option<&str> {
    let inner = literal.get(1..literal.len().checked_sub(1)?)?;
    (!inner.is_empty()).then_some(inner)
}

/// The type node whose name stands for a value of the written type: the type itself, past a pointer, a slice, an array, a channel, a map's value and the arguments of a generic.
pub(crate) fn head_node(mut ty: Node<'_>) -> Option<Node<'_>> {
    // A written type nests as deep as the file does; the loop takes one level per turn.
    loop {
        ty = match ty.kind() {
            "type_identifier" | "qualified_type" => return Some(ty),
            "generic_type" => ty.child_by_field_name("type")?,
            "map_type" | "channel_type" => ty.child_by_field_name("value")?,
            "slice_type" | "array_type" | "implicit_length_array_type" => {
                ty.child_by_field_name("element")?
            }
            "pointer_type" | "parenthesized_type" => ty.named_child(0)?,
            _ => return None,
        };
    }
}

/// What a pass over the whole tree finds.
struct Survey<'a> {
    /// The identifiers that stand before a dot, as the operand of a selector or the package of a qualified type: every name a package could be written by.
    qualifiers: BTreeSet<&'a str>,
    assigned: HashSet<&'a str>,
}

fn survey<'a>(root: Node<'a>, source: &'a str) -> Survey<'a> {
    let mut found = Survey {
        qualifiers: BTreeSet::new(),
        assigned: HashSet::new(),
    };
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        match node.kind() {
            "selector_expression" => {
                if let Some(operand) = node.child_by_field_name("operand")
                    && operand.kind() == "identifier"
                {
                    found.qualifiers.insert(text_of(operand, source));
                }
            }
            "qualified_type" => {
                if let Some(package) = field_text(node, "package", source) {
                    found.qualifiers.insert(package);
                }
            }
            "assignment_statement" => {
                if let Some(left) = node.child_by_field_name("left") {
                    for target in named_children(left) {
                        if target.kind() == "identifier" {
                            found.assigned.insert(text_of(target, source));
                        }
                    }
                }
            }
            "inc_dec_statement" => {
                if let Some(target) = node.named_child(0)
                    && target.kind() == "identifier"
                {
                    found.assigned.insert(text_of(target, source));
                }
            }
            _ => {}
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return found;
            }
        }
    }
}

/// The name of the type a method is declared on, through a pointer and the type arguments of a generic type.
fn receiver_name<'a>(ty: Node<'_>, source: &'a str) -> Option<&'a str> {
    match ty.kind() {
        "type_identifier" => Some(text_of(ty, source)),
        "pointer_type" | "parenthesized_type" => receiver_name(ty.named_child(0)?, source),
        "generic_type" => receiver_name(ty.child_by_field_name("type")?, source),
        _ => None,
    }
}
