//! The walk over the code of a file: which names it reads, from which package, and the contracts its calls make.

use std::collections::BTreeSet;

use gsttt_core::fact::{ContractFact, Embedded, MEMBER_SEPARATOR, UnresolvedSite};
use gsttt_core::rules::Outcome;
use tree_sitter::Node;

use crate::file::FileCx;
use crate::names::{is_predeclared, is_predeclared_function};
use crate::routers::Router;
use crate::scope::{Head, Scopes};
use crate::site::{Callee, read_calls};
use crate::syntax::{MAX_DEPTH, field_text, fields, named_children, text_of};
use crate::value::Reader;

/// The state of one walk over a file. The names it finds are kept per declaration: [`Walk::begin`] starts a declaration and [`Walk::take_uses`] ends it.
#[derive(Debug)]
pub(crate) struct Walk<'r, 'a> {
    pub(crate) cx: &'r FileCx<'a>,
    pub(crate) scopes: Scopes<'a>,
    uses: BTreeSet<String>,
    /// What the code that runs as the package loads uses.
    pub(crate) on_load: BTreeSet<String>,
    /// The free names of the file: read, and bound by no scope, no import and no declaration of the file.
    pub(crate) free: BTreeSet<String>,
    /// The member names the file reads on a value whose type it does not know: `x.M` for a local of unknown type, `f().M`, `items[0].M` and `pkgVar.M`. A package name, a value of a type the file knows and a struct literal key read none.
    pub(crate) reads: BTreeSet<String>,
    /// The names read from each import, by its index in `cx.imports`.
    pub(crate) import_names: Vec<BTreeSet<String>>,
    pub(crate) contracts: Vec<ContractFact>,
    pub(crate) embedded: Vec<Embedded>,
    /// The `(name, of)` pairs where `name` takes the members of the head `of`, spelled as the file writes it.
    pub(crate) heirs: BTreeSet<(String, String)>,
    pub(crate) unresolved: Vec<UnresolvedSite>,
    depth: usize,
    /// Whether the code walked is a package-level initialiser, whose calls run as the package loads.
    load: bool,
    /// Whether some part of the tree nested past [`MAX_DEPTH`] and was left unread.
    pub(crate) cut: bool,
    /// The router the function literal about to be visited receives as its first parameter, set just before the argument is visited.
    pending_router: Option<Router>,
}

impl<'r, 'a> Walk<'r, 'a> {
    pub(crate) fn new(cx: &'r FileCx<'a>) -> Self {
        Self {
            cx,
            scopes: Scopes::default(),
            uses: BTreeSet::new(),
            on_load: BTreeSet::new(),
            free: BTreeSet::new(),
            reads: BTreeSet::new(),
            import_names: vec![BTreeSet::new(); cx.imports.len()],
            contracts: Vec::new(),
            embedded: Vec::new(),
            heirs: BTreeSet::new(),
            unresolved: Vec::new(),
            depth: 0,
            load: false,
            cut: false,
            pending_router: None,
        }
    }

    /// Starts the uses of a new declaration.
    pub(crate) fn begin(&mut self) {
        self.uses.clear();
    }

    /// Adds names to the uses of the declaration in hand.
    pub(crate) fn uses_extend(&mut self, names: BTreeSet<String>) {
        self.uses.extend(names);
    }

    /// The uses of the declaration since [`Walk::begin`].
    pub(crate) fn take_uses(&mut self) -> BTreeSet<String> {
        std::mem::take(&mut self.uses)
    }

    /// Walks code that runs as the package loads: its calls are what the package runs.
    pub(crate) fn as_load<T>(&mut self, walk: impl FnOnce(&mut Self) -> T) -> T {
        let before = std::mem::replace(&mut self.load, true);
        let out = walk(self);
        self.load = before;
        out
    }

    pub(crate) fn visit(&mut self, node: Node<'a>) {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.cut = true;
        } else {
            self.dispatch(node);
        }
        self.depth -= 1;
    }

    fn dispatch(&mut self, node: Node<'a>) {
        match node.kind() {
            "identifier" => self.use_name(text_of(node, self.cx.source)),
            "field_identifier"
            | "package_identifier"
            | "label_name"
            | "blank_identifier"
            | "dot"
            | "comment"
            | "interpreted_string_literal"
            | "raw_string_literal" => {}
            "type_identifier"
            | "qualified_type"
            | "generic_type"
            | "pointer_type"
            | "slice_type"
            | "array_type"
            | "implicit_length_array_type"
            | "map_type"
            | "channel_type"
            | "parenthesized_type" => self.visit_type(node),
            "selector_expression" => self.selector(node),
            "call_expression" => self.call(node),
            "composite_literal" => self.composite(node),
            "func_literal" => self.func_literal(node),
            "block"
            | "if_statement"
            | "for_statement"
            | "expression_switch_statement"
            | "select_statement"
            | "expression_case"
            | "default_case"
            | "communication_case" => {
                self.scoped(node);
            }
            "type_switch_statement" => self.type_switch(node),
            "short_var_declaration" => self.short_var(node),
            "var_declaration" | "const_declaration" => self.local_values(node),
            "type_declaration" => self.local_types(node),
            "range_clause" => self.range_clause(node),
            "receive_statement" => self.receive(node),
            "parameter_list" => self.params(node, false),
            _ => self.visit_children(node),
        }
    }

    pub(crate) fn visit_children(&mut self, node: Node<'a>) {
        let mut cursor = node.walk();
        if !cursor.goto_first_child() {
            return;
        }
        loop {
            let child = cursor.node();
            if child.is_named() {
                self.visit(child);
            }
            if !cursor.goto_next_sibling() {
                return;
            }
        }
    }

    fn scoped(&mut self, node: Node<'a>) {
        self.scopes.push_scope();
        self.visit_children(node);
        self.scopes.pop_scope();
    }

    /// A name in an expression.
    fn use_name(&mut self, name: &'a str) {
        if name == "_"
            || self.scopes.find(name).is_some()
            || self.cx.import_named(name).is_some()
            || is_predeclared(name)
        {
            return;
        }
        self.uses.insert(name.to_owned());
        self.note_package_name(name);
    }

    /// A name of the package that no declaration of this file gives: another file of the package may.
    fn note_package_name(&mut self, name: &str) {
        if self.cx.top.contains(name) {
            return;
        }
        self.free.insert(name.to_owned());
    }

    /// The free names that may be read from the package itself. A name that is the package name of an import whose path ends in another segment is among them: it gets an edge only when a sibling file declares it, so nothing is lost by keeping it.
    pub(crate) fn package_names(&self) -> Vec<String> {
        self.free.iter().cloned().collect()
    }

    /// Notes that the file reads the type `head` from its package or its import.
    fn note_head(&mut self, head: &Head) {
        match head.import {
            Some(index) => {
                if let Some(names) = self.import_names.get_mut(index) {
                    names.insert(head.name.clone());
                }
            }
            None => self.note_package_name(&head.name),
        }
    }

    pub(crate) fn head_of(&self, ty: Node<'a>) -> Option<Head> {
        let scopes = &self.scopes;
        self.cx.head_of(ty, &|name| scopes.find(name).is_some())
    }

    /// The use of a type, by its bare name: a file that names the type and reads a member reaches the type's callers through it.
    fn use_type(&mut self, head: &Head) {
        let spelled = self.cx.spell(head);
        self.uses.insert(spelled);
        self.note_head(head);
    }

    /// Records that `name` takes the members of the type `head`.
    pub(crate) fn heir(&mut self, name: &str, head: &Head) {
        self.heirs.insert((name.to_owned(), self.cx.spell(head)));
    }

    /// What a package-level value is an heir of: the type it holds, or the function whose result it holds.
    pub(crate) fn held_name(&self, value: Node<'a>) -> Option<String> {
        if let Some(head) = self.holds_of(value) {
            return Some(self.cx.spell(&head));
        }
        if value.kind() != "call_expression" {
            return None;
        }
        let function = value.child_by_field_name("function")?;
        let source = self.cx.source;
        match function.kind() {
            "identifier" => {
                let name = text_of(function, source);
                (self.scopes.find(name).is_none() && !is_predeclared_function(name))
                    .then(|| name.to_owned())
            }
            "selector_expression" => {
                let operand = function.child_by_field_name("operand")?;
                let name = field_text(function, "field", source)?;
                let qualifier = text_of(operand, source);
                (operand.kind() == "identifier"
                    && self.scopes.find(qualifier).is_none()
                    && self.cx.import_named(qualifier).is_some())
                .then(|| format!("{qualifier}.{name}"))
            }
            _ => None,
        }
    }

    /// The use of a member of a type: `Type.member`.
    fn use_member(&mut self, head: &Head, member: &str) {
        self.uses
            .insert(format!("{}{MEMBER_SEPARATOR}{member}", self.cx.spell(head)));
        self.note_head(head);
        // The file that declares the type does not declare every member: a sibling may hold the method, so the file reaches it.
        if head.import.is_none()
            && self.cx.top.contains(head.name.as_str())
            && !self.cx.declares_member(&head.name, member)
        {
            self.free.insert(head.name.clone());
        }
    }

    /// Walks a type. The head of the type, which is a pointer, a slice or a map's value away, is a use of the type and of no member.
    pub(crate) fn visit_type(&mut self, node: Node<'a>) {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.cut = true;
        } else {
            self.type_node(node);
        }
        self.depth -= 1;
    }

    fn type_node(&mut self, node: Node<'a>) {
        match node.kind() {
            "type_identifier" | "qualified_type" => {
                if let Some(head) = self.head_of(node) {
                    self.use_type(&head);
                }
            }
            "generic_type" => {
                if let Some(ty) = node.child_by_field_name("type") {
                    self.visit_type(ty);
                }
                if let Some(arguments) = node.child_by_field_name("type_arguments") {
                    self.visit_children(arguments);
                }
            }
            "pointer_type" | "parenthesized_type" => {
                if let Some(inner) = node.named_child(0) {
                    self.visit_type(inner);
                }
            }
            "slice_type" | "array_type" | "implicit_length_array_type" => {
                if let Some(length) = node.child_by_field_name("length") {
                    self.visit(length);
                }
                if let Some(element) = node.child_by_field_name("element") {
                    self.visit_type(element);
                }
            }
            "channel_type" => {
                if let Some(value) = node.child_by_field_name("value") {
                    self.visit_type(value);
                }
            }
            "map_type" => {
                if let Some(key) = node.child_by_field_name("key") {
                    self.visit_type(key);
                }
                if let Some(value) = node.child_by_field_name("value") {
                    self.visit_type(value);
                }
            }
            _ => self.visit_children(node),
        }
    }

    fn selector(&mut self, node: Node<'a>) {
        let (Some(operand), Some(field)) = (
            node.child_by_field_name("operand"),
            field_text(node, "field", self.cx.source),
        ) else {
            self.visit_children(node);
            return;
        };
        match operand.kind() {
            "identifier" => {
                let name = text_of(operand, self.cx.source);
                let binding = self.scopes.find(name);
                if let Some(binding) = binding {
                    match binding.holds.clone() {
                        Some(head) => self.use_member(&head, field),
                        None => self.read(field),
                    }
                    return;
                }
                if let Some(index) = self.cx.import_named(name) {
                    self.use_qualified(index, field);
                    return;
                }
                self.read(field);
                self.visit(operand);
            }
            "selector_expression" => {
                if !self.field_chain(operand, field) {
                    self.read(field);
                    self.visit(operand);
                }
            }
            _ => {
                self.read(field);
                self.visit(operand);
            }
        }
    }

    /// Records `member` as read on a value of unknown type. A name already seen allocates nothing.
    fn read(&mut self, member: &str) {
        if !self.reads.contains(member) {
            self.reads.insert(member.to_owned());
        }
    }

    fn use_qualified(&mut self, index: usize, name: &str) {
        let Some(import) = self.cx.imports.get(index) else {
            return;
        };
        self.uses
            .insert(format!("{}{MEMBER_SEPARATOR}{name}", import.qualifier));
        if let Some(names) = self.import_names.get_mut(index) {
            names.insert(name.to_owned());
        }
    }

    /// `x.f.M` where `x` holds a struct of this file and `f` is written as the type `U`: the use of `U.M` and of the field `T.f`.
    fn field_chain(&mut self, inner: Node<'a>, member: &str) -> bool {
        let (Some(operand), Some(field)) = (
            inner.child_by_field_name("operand"),
            field_text(inner, "field", self.cx.source),
        ) else {
            return false;
        };
        if operand.kind() != "identifier" {
            return false;
        }
        let Some(holder) = self
            .scopes
            .find(text_of(operand, self.cx.source))
            .and_then(|binding| binding.holds.clone())
        else {
            return false;
        };
        let cx = self.cx;
        let Some(Some(field_type)) = (holder.import.is_none())
            .then(|| cx.structs.get(&holder.name))
            .flatten()
            .and_then(|found| found.get(field))
        else {
            return false;
        };
        self.use_member(field_type, member);
        self.use_member(&holder, field);
        true
    }

    fn call(&mut self, node: Node<'a>) {
        let Some(function) = node.child_by_field_name("function") else {
            self.visit_children(node);
            return;
        };
        let arguments = node.child_by_field_name("arguments");
        self.read_contracts(node, function, arguments);
        if self.load {
            self.note_load_call(function);
        }
        self.visit(function);
        let Some(arguments) = arguments else {
            return;
        };
        let closure = self.closure_router(function, &named_children(arguments));
        let Some((literal, router)) = closure else {
            self.visit_children(arguments);
            return;
        };
        for argument in named_children(arguments) {
            if argument == literal {
                self.pending_router = Some(router.clone());
            }
            self.visit(argument);
        }
    }

    /// A call a package-level initialiser makes runs as the package loads.
    fn note_load_call(&mut self, function: Node<'a>) {
        match function.kind() {
            "identifier" => {
                let name = text_of(function, self.cx.source);
                if self.scopes.find(name).is_none()
                    && !is_predeclared(name)
                    && !is_predeclared_function(name)
                {
                    self.on_load.insert(name.to_owned());
                }
            }
            "selector_expression" => {
                if let (Some(operand), Some(field)) = (
                    function.child_by_field_name("operand"),
                    field_text(function, "field", self.cx.source),
                ) && operand.kind() == "identifier"
                {
                    let name = text_of(operand, self.cx.source);
                    if self.scopes.find(name).is_none()
                        && let Some(index) = self.cx.import_named(name)
                        && let Some(import) = self.cx.imports.get(index)
                        && import.local
                    {
                        self.on_load
                            .insert(format!("{}{MEMBER_SEPARATOR}{field}", import.qualifier));
                    }
                }
            }
            _ => {}
        }
    }

    fn read_contracts(&mut self, call: Node<'a>, function: Node<'a>, arguments: Option<Node<'a>>) {
        let Some(callee) = self.callee_of(function) else {
            return;
        };
        if callee.local {
            return;
        }
        let arguments = arguments.map(named_children).unwrap_or_default();
        let operand = self.receiver_operand(function);
        let receiver = || {
            operand
                .map(|node| self.receiver_of(node))
                .unwrap_or_default()
        };
        let read = read_calls(
            Reader {
                cx: self.cx,
                scopes: &self.scopes,
            },
            call,
            &arguments,
            &callee,
            &receiver,
            &self.methods_of(call),
        );
        for outcome in read.outcomes {
            match outcome {
                Outcome::Fact(fact) => self.contracts.push(fact),
                Outcome::Blind(site) => self.unresolved.push(site),
                Outcome::Skip => {}
            }
        }
        self.embedded.extend(read.embedded);
        self.unresolved.extend(read.unresolved);
    }

    /// The callee as the rules of the packs see it: dotted, with the qualifier of an import replaced by the name the package is known by.
    pub(crate) fn callee_of(&self, function: Node<'a>) -> Option<Callee> {
        let mut segments: Vec<&str> = Vec::new();
        let mut node = function;
        let first = loop {
            match node.kind() {
                "identifier" => break text_of(node, self.cx.source),
                "selector_expression" => {
                    segments.push(field_text(node, "field", self.cx.source)?);
                    node = node.child_by_field_name("operand")?;
                }
                "call_expression" => node = self.through_with(node)?,
                _ => return None,
            }
        };
        let imported = if self.scopes.find(first).is_none() {
            self.cx
                .import_named(first)
                .and_then(|index| self.cx.imports.get(index))
        } else {
            None
        };
        let (head, local) = match imported {
            Some(import) => (import.callee_name.as_str(), import.local),
            None => (first, false),
        };
        let mut written = String::from(head);
        for segment in segments.iter().rev() {
            written.push(MEMBER_SEPARATOR);
            written.push_str(segment);
        }
        Some(Callee { written, local })
    }

    fn composite(&mut self, node: Node<'a>) {
        let ty = node.child_by_field_name("type");
        if let Some(ty) = ty {
            self.visit(ty);
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.literal_value(body, ty);
        }
    }

    /// The elements of a literal of the type `ty`. A key is a field name when the literal is a struct, and a value when it is a map, a slice or an array; a literal with no type, an element that leaves its type out in a literal of unknown type, is read as a struct.
    fn literal_value(&mut self, body: Node<'a>, ty: Option<Node<'a>>) {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.cut = true;
            self.depth -= 1;
            return;
        }
        let keys_are_values = ty.is_some_and(|ty| {
            matches!(
                ty.kind(),
                "slice_type" | "array_type" | "implicit_length_array_type" | "map_type"
            )
        });
        let element_type = ty.and_then(|ty| match ty.kind() {
            "slice_type" | "array_type" | "implicit_length_array_type" => {
                ty.child_by_field_name("element")
            }
            "map_type" => ty.child_by_field_name("value"),
            _ => None,
        });
        let element_type = element_type.map(strip_pointer);
        for element in named_children(body) {
            if element.kind() == "keyed_element" {
                let parts = named_children(element);
                if let (Some(key), Some(value)) = (parts.first(), parts.get(1)) {
                    let inner = unwrap_element(*key);
                    if keys_are_values || inner.kind() != "identifier" {
                        self.element(*key, None);
                    }
                    self.element(*value, element_type);
                }
            } else {
                self.element(element, element_type);
            }
        }
        self.depth -= 1;
    }

    fn element(&mut self, element: Node<'a>, ty: Option<Node<'a>>) {
        let inner = unwrap_element(element);
        if inner.kind() == "literal_value" {
            self.literal_value(inner, ty);
        } else {
            self.visit(inner);
        }
    }

    fn func_literal(&mut self, node: Node<'a>) {
        self.scopes.push_scope();
        let router = self.pending_router.take();
        let load = std::mem::replace(&mut self.load, false);
        self.signature(node);
        if let Some(router) = router {
            self.bind_closure_router(node, router);
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.visit(body);
        }
        self.load = load;
        self.scopes.pop_scope();
    }

    /// Binds and reads the type parameters, the receiver, the parameters and the results of a function.
    pub(crate) fn signature(&mut self, node: Node<'a>) {
        if let Some(list) = node.child_by_field_name("type_parameters") {
            self.type_parameters(list);
        }
        if let Some(receiver) = node.child_by_field_name("receiver") {
            self.bind_receiver_arguments(receiver);
            self.params(receiver, true);
        }
        if let Some(parameters) = node.child_by_field_name("parameters") {
            self.params(parameters, true);
        }
        match node.child_by_field_name("result") {
            Some(result) if result.kind() == "parameter_list" => self.params(result, true),
            Some(result) => self.visit_type(result),
            None => {}
        }
    }

    /// Binds the names of the type parameters, then reads their constraints.
    pub(crate) fn type_parameters(&mut self, list: Node<'a>) {
        let declarations = named_children(list);
        for declaration in &declarations {
            for name in fields(*declaration, "name") {
                self.scopes.bind(text_of(name, self.cx.source), None, None);
            }
        }
        for declaration in declarations {
            if let Some(constraint) = declaration.child_by_field_name("type") {
                self.visit(constraint);
            }
        }
    }

    /// `func (r *T[K]) M()` declares `K` as a type parameter of the method.
    fn bind_receiver_arguments(&mut self, receiver: Node<'a>) {
        for declaration in named_children(receiver) {
            let Some(mut ty) = declaration.child_by_field_name("type") else {
                continue;
            };
            if ty.kind() == "pointer_type"
                && let Some(inner) = ty.named_child(0)
            {
                ty = inner;
            }
            let Some(arguments) = (ty.kind() == "generic_type")
                .then(|| ty.child_by_field_name("type_arguments"))
                .flatten()
            else {
                continue;
            };
            for element in named_children(arguments) {
                let name = if element.kind() == "type_elem" {
                    element.named_child(0)
                } else {
                    Some(element)
                };
                if let Some(name) = name.filter(|name| name.kind() == "type_identifier") {
                    self.scopes.bind(text_of(name, self.cx.source), None, None);
                }
            }
        }
    }

    /// The parameters of a signature. With `bind`, their names are bound after their types are read, and their types are those of bindings; without it the list is the signature of a function type, which names no one.
    fn params(&mut self, list: Node<'a>, bind: bool) {
        for declaration in named_children(list) {
            if !matches!(
                declaration.kind(),
                "parameter_declaration" | "variadic_parameter_declaration"
            ) {
                continue;
            }
            let Some(ty) = declaration.child_by_field_name("type") else {
                continue;
            };
            self.visit_type(ty);
            if bind {
                let holds = self.head_of(ty);
                for name in fields(declaration, "name") {
                    self.scopes
                        .bind(text_of(name, self.cx.source), holds.clone(), None);
                }
            }
        }
    }

    fn type_switch(&mut self, node: Node<'a>) {
        self.scopes.push_scope();
        let mut aliases = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "type_case" => self.type_case(child, &aliases),
                "default_case" => self.scoped(child),
                _ => {
                    if node.child_by_field_name("alias") == Some(child) {
                        aliases = named_children(child);
                    } else {
                        self.visit(child);
                    }
                }
            }
        }
        self.scopes.pop_scope();
    }

    fn type_case(&mut self, case: Node<'a>, aliases: &[Node<'a>]) {
        self.scopes.push_scope();
        let types = fields(case, "type");
        for ty in &types {
            self.visit_type(*ty);
        }
        let holds = match types.as_slice() {
            [only] => self.head_of(*only),
            _ => None,
        };
        for alias in aliases {
            self.scopes
                .bind(text_of(*alias, self.cx.source), holds.clone(), None);
        }
        for statement in named_children(case) {
            if !types.contains(&statement) {
                self.visit(statement);
            }
        }
        self.scopes.pop_scope();
    }

    fn short_var(&mut self, node: Node<'a>) {
        let (left, right) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        );
        if let Some(right) = right {
            self.visit(right);
        }
        self.bind_pairs(left, right);
    }

    /// Binds the names on the left to the values on the right, after the values are read: in `a, b := b, a` the right side sees the old names.
    fn bind_pairs(&mut self, left: Option<Node<'a>>, right: Option<Node<'a>>) {
        let names = left.map(named_children).unwrap_or_default();
        let values = right.map(named_children).unwrap_or_default();
        let paired = names.len() == values.len();
        let mut pending = Vec::with_capacity(names.len());
        for (index, name) in names.iter().enumerate() {
            if name.kind() != "identifier" {
                continue;
            }
            let text = text_of(*name, self.cx.source);
            let value = if paired { values.get(index) } else { None };
            let holds =
                first_result(&values, paired, index).and_then(|value| self.holds_of(*value));
            let known = value
                .filter(|_| !self.cx.assigned.contains(text))
                .and_then(|value| {
                    Reader {
                        cx: self.cx,
                        scopes: &self.scopes,
                    }
                    .text(*value)
                });
            let router = value.and_then(|value| self.router_of(*value));
            pending.push((text, holds, known, router));
        }
        for (name, holds, text, router) in pending {
            self.scopes.bind(name, holds, text);
            if let Some(router) = router {
                self.scopes.set_router(name, router);
            }
        }
    }

    /// The type an expression holds, as far as the way it is written tells.
    fn holds_of(&self, value: Node<'a>) -> Option<Head> {
        self.holds_at(value, 0)
    }

    fn holds_at(&self, value: Node<'a>, depth: usize) -> Option<Head> {
        if depth > 8 {
            return None;
        }
        match value.kind() {
            "composite_literal" | "type_assertion_expression" => {
                self.head_of(value.child_by_field_name("type")?)
            }
            "unary_expression" | "index_expression" => {
                self.holds_at(value.child_by_field_name("operand")?, depth + 1)
            }
            "parenthesized_expression" => self.holds_at(value.named_child(0)?, depth + 1),
            "identifier" => self
                .scopes
                .find(text_of(value, self.cx.source))?
                .holds
                .clone(),
            "call_expression" => self.holds_by_call(value),
            _ => None,
        }
    }

    /// `new(T)`: any other call gives no holding, since the name of a function does not tell what it returns.
    fn holds_by_call(&self, call: Node<'a>) -> Option<Head> {
        let function = call.child_by_field_name("function")?;
        let source = self.cx.source;
        match function.kind() {
            "identifier" => {
                let name = text_of(function, source);
                if self.scopes.find(name).is_some() {
                    return None;
                }
                if name == "new" {
                    let argument = named_children(call.child_by_field_name("arguments")?)
                        .into_iter()
                        .next()?;
                    return self.head_of_expression(argument);
                }
                None
            }
            _ => None,
        }
    }

    /// The head of a type that stands in an expression, as the argument of `new` does.
    fn head_of_expression(&self, node: Node<'a>) -> Option<Head> {
        let source = self.cx.source;
        match node.kind() {
            "identifier" | "type_identifier" => {
                let name = text_of(node, source);
                (self.scopes.find(name).is_none() && !is_predeclared(name)).then(|| Head {
                    import: None,
                    name: name.to_owned(),
                })
            }
            "selector_expression" | "qualified_type" => {
                let (operand, name) = if node.kind() == "qualified_type" {
                    (
                        text_of(node.child_by_field_name("package")?, source),
                        field_text(node, "name", source)?,
                    )
                } else {
                    (
                        text_of(node.child_by_field_name("operand")?, source),
                        field_text(node, "field", source)?,
                    )
                };
                if self.scopes.find(operand).is_some() {
                    return None;
                }
                Some(Head {
                    import: Some(self.cx.import_named(operand)?),
                    name: name.to_owned(),
                })
            }
            _ => self.head_of(node),
        }
    }

    fn range_clause(&mut self, node: Node<'a>) {
        let right = node.child_by_field_name("right");
        if let Some(right) = right {
            self.visit(right);
        }
        let Some(left) = node.child_by_field_name("left") else {
            return;
        };
        if !has_token(node, ":=") {
            self.visit(left);
            return;
        }
        let holds = right.and_then(|right| self.holds_of(right));
        let names = named_children(left);
        for (index, name) in names.iter().enumerate() {
            // The first variable of a range is the index or the key; only the last one holds an element.
            let holds = (index > 0).then(|| holds.clone()).flatten();
            self.scopes
                .bind(text_of(*name, self.cx.source), holds, None);
        }
    }

    fn receive(&mut self, node: Node<'a>) {
        let (left, right) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        );
        if has_token(node, ":=") {
            if let Some(right) = right {
                self.visit(right);
            }
            self.bind_pairs(left, None);
        } else {
            self.visit_children(node);
        }
    }

    /// `var` and `const` inside a function.
    fn local_values(&mut self, node: Node<'a>) {
        for spec in named_children(node) {
            match spec.kind() {
                "var_spec" | "const_spec" => self.local_spec(spec),
                "var_spec_list" => {
                    for inner in named_children(spec) {
                        self.local_spec(inner);
                    }
                }
                _ => {}
            }
        }
    }

    fn local_spec(&mut self, spec: Node<'a>) {
        let ty = spec.child_by_field_name("type");
        if let Some(ty) = ty {
            self.visit_type(ty);
        }
        let value = spec.child_by_field_name("value");
        if let Some(value) = value {
            self.visit(value);
        }
        let declared = ty.and_then(|ty| self.head_of(ty));
        let names = fields(spec, "name");
        let values = value.map(named_children).unwrap_or_default();
        let paired = names.len() == values.len();
        let mut pending = Vec::with_capacity(names.len());
        for (index, name) in names.iter().enumerate() {
            let text = text_of(*name, self.cx.source);
            let value = if paired { values.get(index) } else { None };
            let holds = declared.clone().or_else(|| {
                first_result(&values, paired, index).and_then(|value| self.holds_of(*value))
            });
            let known = value
                .filter(|_| !self.cx.assigned.contains(text))
                .and_then(|value| {
                    Reader {
                        cx: self.cx,
                        scopes: &self.scopes,
                    }
                    .text(*value)
                });
            let router = value.and_then(|value| self.router_of(*value));
            pending.push((text, holds, known, router));
        }
        for (name, holds, text, router) in pending {
            self.scopes.bind(name, holds, text);
            if let Some(router) = router {
                self.scopes.set_router(name, router);
            }
        }
    }

    /// `type` inside a function. The name is bound first, because a type may refer to itself.
    fn local_types(&mut self, node: Node<'a>) {
        for spec in named_children(node) {
            let Some(name) = field_text(spec, "name", self.cx.source) else {
                continue;
            };
            self.scopes.bind(name, None, None);
            self.scopes.push_scope();
            if let Some(list) = spec.child_by_field_name("type_parameters") {
                self.type_parameters(list);
            }
            if let Some(ty) = spec.child_by_field_name("type") {
                self.visit(ty);
            }
            self.scopes.pop_scope();
        }
    }
}

/// The value that gives the type of the name at `index`: its own value, or for the first name of `a, err := NewT()` the one call that makes every name.
pub(crate) fn first_result<'n, 'a>(
    values: &'n [Node<'a>],
    paired: bool,
    index: usize,
) -> Option<&'n Node<'a>> {
    if paired {
        return values.get(index);
    }
    match values {
        [call] if index == 0 && call.kind() == "call_expression" => Some(call),
        _ => None,
    }
}

/// The type behind a pointer: the literal `&T{}` is elided as `{}` in a `[]*T`.
fn strip_pointer(ty: Node<'_>) -> Node<'_> {
    if ty.kind() == "pointer_type" {
        ty.named_child(0).unwrap_or(ty)
    } else {
        ty
    }
}

fn unwrap_element(element: Node<'_>) -> Node<'_> {
    if element.kind() == "literal_element" {
        element.named_child(0).unwrap_or(element)
    } else {
        element
    }
}

/// Whether the node has the token `token` among its children.
fn has_token(node: Node<'_>, token: &str) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| child.kind() == token)
}
