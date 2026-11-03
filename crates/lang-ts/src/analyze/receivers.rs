//! The names a file reads members on whose value is known to be a builtin, found before the walk: `Object` in `Object.entries(x)`, `path` of `const path = require('node:path')`, `server` of `const server = http.createServer()`. A member read on such a name is a read of the builtin and not of a class of the repository, so it says nothing about the callers of a method.

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::fact::ImportKind;
use oxc_ast::ast::{
    AssignmentExpression, AssignmentOperator, AssignmentTarget, BindingIdentifier, BindingPattern,
    ComputedMemberExpression, Expression, ForInStatement, ForOfStatement, ForStatementLeft,
    IdentifierReference, ImportDeclaration, ImportDeclarationSpecifier, ObjectExpression,
    ObjectPropertyKind, Program, StaticMemberExpression, UpdateExpression, VariableDeclaration,
    VariableDeclarationKind, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};

use super::load_of;
use crate::builtins::{is_builtin, is_global_builtin};
use crate::shape::{literal_text, property_name};

/// The names whose members are read on a builtin.
#[derive(Debug, Default)]
pub(super) struct Receivers {
    /// The names bound once or more, every time to a Node module or to a value made by one or by a global constructor, and never otherwise.
    known: BTreeSet<String>,
    /// The global names that the file binds somewhere, in any scope: such a name stands for the file's own value wherever it is read.
    bound_globals: BTreeSet<String>,
    /// The names that the file reads anywhere other than as the object of a member access: called, passed, returned or exported.
    bare: BTreeSet<String>,
    /// The shape of the object literal that each name is bound to, once and by `const`, and never otherwise.
    shapes: BTreeMap<String, Shape>,
}

/// The keys of an object literal, each with the shape of the literal it holds, `None` for any other value: a key can hold anything.
#[derive(Debug, Default)]
struct Shape {
    keys: BTreeMap<String, Option<Shape>>,
}

impl Shape {
    /// The keys that `object` names. A computed key adds none, and a spread may replace any key before it, so what those keys hold is no longer known.
    fn of(object: &ObjectExpression<'_>) -> Self {
        let mut shape = Shape::default();
        for property in &object.properties {
            match property {
                ObjectPropertyKind::ObjectProperty(property) if !property.computed => {
                    if let Some(key) = property_name(&property.key) {
                        let held = match property.value.get_inner_expression() {
                            Expression::ObjectExpression(inner) if !property.method => {
                                Some(Shape::of(inner))
                            }
                            _ => None,
                        };
                        shape.keys.insert(key, held);
                    }
                }
                ObjectPropertyKind::ObjectProperty(_) => {}
                ObjectPropertyKind::SpreadProperty(_) => {
                    shape.keys.values_mut().for_each(|held| *held = None);
                }
            }
        }
        shape
    }
}

impl Receivers {
    /// Reads the bindings and assignments of `program`.
    pub(super) fn collect(program: &Program<'_>) -> Self {
        let mut found = Found::default();
        found.visit_program(program);
        found.finish()
    }

    /// Whether the file uses `name` other than as the object of a member access, in any scope.
    pub(super) fn is_used_bare(&self, name: &str) -> bool {
        self.bare.contains(name)
    }

    /// How many links of `chain`, the names that follow `name` in a member chain, are keys of the object literal that `name` is bound to: those links are no reads, since the file wrote them.
    pub(super) fn literal_links(&self, name: &str, chain: &[Option<String>]) -> usize {
        let mut shape = self.shapes.get(name);
        let mut links = 0;
        for link in chain {
            let Some(held) = shape.and_then(|shape| shape.keys.get(link.as_deref()?)) else {
                break;
            };
            links += 1;
            shape = held.as_ref();
        }
        links
    }

    /// Whether a member read on `name` is a read on a builtin value.
    pub(super) fn is_known(&self, name: &str) -> bool {
        self.known.contains(name) || (is_global_builtin(name) && !self.bound_globals.contains(name))
    }
}

/// What a value given to a name is made from: the name its callee starts with.
#[derive(Debug)]
enum Root {
    /// `new Map()`, `new http.Agent()`.
    Constructed(String),
    /// `http.createServer()`, `createServer()`.
    Called(String),
}

/// Everything the file does to one name.
#[derive(Debug, Default)]
struct Fact {
    /// The name is bound or assigned in a way that gives it no value this can follow.
    bad: bool,
    /// A site binds the name to a Node module, or to a name of one.
    node: bool,
    /// The values given to the name by an initialiser or a plain assignment.
    values: Vec<Root>,
    /// The shape of the object literal a `const` gives the name, and how many sites bind the name to one.
    literal: Option<Shape>,
    literals: u32,
}

#[derive(Default)]
struct Found {
    facts: BTreeMap<String, Fact>,
    /// The walk is in the target of an assignment, where a name is written and not read.
    writing: bool,
    /// The walk is in the left side of a `for` loop that declares its name, which the loop gives a new value at each turn.
    loop_left: bool,
    /// The declaration being walked is a `const`.
    constant: bool,
    /// Where the identifier that is the object of the member access being walked starts.
    object_at: Option<u32>,
    /// The names read other than as the object of a member access.
    bare: BTreeSet<String>,
}

impl Found {
    /// Changes the fact of `name`, made when there is none. A name met again allocates nothing.
    fn edit(&mut self, name: &str, change: impl FnOnce(&mut Fact)) {
        if let Some(fact) = self.facts.get_mut(name) {
            change(fact);
        } else {
            let mut fact = Fact::default();
            change(&mut fact);
            self.facts.insert(name.to_owned(), fact);
        }
    }

    /// Whether `name` is bound only to Node modules.
    fn pure_node(&self, name: &str) -> bool {
        self.facts
            .get(name)
            .is_some_and(|fact| !fact.bad && fact.node && fact.values.is_empty())
    }

    fn resolves(&self, root: &Root) -> bool {
        match root {
            Root::Constructed(name) => {
                (is_global_builtin(name) && !self.facts.contains_key(name)) || self.pure_node(name)
            }
            Root::Called(name) => self.pure_node(name),
        }
    }

    fn finish(self) -> Receivers {
        let known = self
            .facts
            .iter()
            .filter(|(_, fact)| {
                !fact.bad
                    && (fact.node || !fact.values.is_empty())
                    && fact.values.iter().all(|root| self.resolves(root))
            })
            .map(|(name, _)| name.clone())
            .collect();
        let bound_globals = self
            .facts
            .keys()
            .filter(|name| is_global_builtin(name))
            .map(|name| (*name).clone())
            .collect();
        let shapes = self
            .facts
            .into_iter()
            .filter(|(_, fact)| {
                !fact.bad && !fact.node && fact.values.is_empty() && fact.literals == 1
            })
            .filter_map(|(name, fact)| Some((name, fact.literal?)))
            .collect();
        Receivers {
            known,
            bound_globals,
            bare: self.bare,
            shapes,
        }
    }

    /// Records the value `init` given to `name`: a value this cannot follow makes the name unknown at every read.
    fn given(&mut self, name: &str, init: &Expression<'_>) {
        match root_of(init) {
            Some(Root::Called(_)) if is_node_require(init) => {
                self.edit(name, |fact| fact.node = true);
            }
            Some(root) => self.edit(name, |fact| fact.values.push(root)),
            None => self.edit(name, |fact| fact.bad = true),
        }
    }
}

/// The identifier a chain of plain member accesses starts with: `http` of `http.createServer`.
fn chain_root<'e>(expr: &'e Expression<'_>) -> Option<&'e str> {
    match expr.get_inner_expression() {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(member) => chain_root(&member.object),
        _ => None,
    }
}

/// What `expr` is made from, when it is a constructor or a function called on a name, and not on the result of another call, which may hand back anything it was given.
fn root_of(expr: &Expression<'_>) -> Option<Root> {
    match expr.get_inner_expression() {
        Expression::NewExpression(new) => {
            chain_root(&new.callee).map(|root| Root::Constructed(root.to_owned()))
        }
        Expression::CallExpression(call) => {
            chain_root(&call.callee).map(|root| Root::Called(root.to_owned()))
        }
        _ => None,
    }
}

/// Whether `expr` is `require('x')` for a module Node ships.
fn is_node_require(expr: &Expression<'_>) -> bool {
    load_of(expr).is_some_and(|load| {
        load.kind == ImportKind::Require
            && load
                .argument
                .and_then(literal_text)
                .is_some_and(|spec| is_builtin(&spec))
    })
}

impl<'a> Visit<'a> for Found {
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.edit(it.name.as_str(), |fact| fact.bad = true);
    }

    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if self.writing {
            self.edit(it.name.as_str(), |fact| fact.bad = true);
        }
        if self.object_at == Some(it.span.start) {
            self.object_at = None;
        } else {
            self.bare.insert(it.name.as_str().to_owned());
        }
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        if let Expression::Identifier(object) = &it.object {
            self.object_at = Some(object.span.start);
        }
        walk::walk_static_member_expression(self, it);
    }

    fn visit_computed_member_expression(&mut self, it: &ComputedMemberExpression<'a>) {
        if let Expression::Identifier(object) = &it.object {
            self.object_at = Some(object.span.start);
        }
        walk::walk_computed_member_expression(self, it);
    }

    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        let outer = std::mem::replace(
            &mut self.constant,
            it.kind == VariableDeclarationKind::Const,
        );
        walk::walk_variable_declaration(self, it);
        self.constant = outer;
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        match (&it.id, &it.init) {
            // `const o = { … }` gives the name the shape of the literal, which holds only while nothing else binds the name.
            (BindingPattern::BindingIdentifier(id), Some(Expression::ObjectExpression(object)))
                if self.constant && !self.loop_left =>
            {
                let shape = Shape::of(object);
                self.edit(id.name.as_str(), |fact| {
                    fact.literals += 1;
                    fact.literal = Some(shape);
                });
                self.visit_object_expression(object);
            }
            (BindingPattern::BindingIdentifier(id), Some(init)) if !self.loop_left => {
                self.given(id.name.as_str(), init);
                self.visit_expression(init);
            }
            // `let t;` gives no value, so the assignments after it say what `t` is.
            (BindingPattern::BindingIdentifier(id), None) if !self.loop_left => {
                self.edit(id.name.as_str(), |_| {});
            }
            // `const { createServer } = require('node:http')` binds each local to a name of a Node module.
            (BindingPattern::ObjectPattern(object), Some(init))
                if is_node_require(init)
                    && object.rest.is_none()
                    && object.properties.iter().all(|property| {
                        !property.computed
                            && property_name(&property.key).is_some()
                            && matches!(property.value, BindingPattern::BindingIdentifier(_))
                    }) =>
            {
                for property in &object.properties {
                    if let BindingPattern::BindingIdentifier(local) = &property.value {
                        self.edit(local.name.as_str(), |fact| fact.node = true);
                    }
                }
            }
            _ => walk::walk_variable_declarator(self, it),
        }
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if let (AssignmentTarget::AssignmentTargetIdentifier(id), AssignmentOperator::Assign) =
            (&it.left, it.operator)
        {
            self.given(id.name.as_str(), &it.right);
        } else {
            let outer = std::mem::replace(&mut self.writing, true);
            self.visit_assignment_target(&it.left);
            self.writing = outer;
        }
        self.visit_expression(&it.right);
    }

    fn visit_update_expression(&mut self, it: &UpdateExpression<'a>) {
        let outer = std::mem::replace(&mut self.writing, true);
        self.visit_simple_assignment_target(&it.argument);
        self.writing = outer;
    }

    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        self.for_left(&it.left);
        self.visit_expression(&it.right);
        self.visit_statement(&it.body);
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        self.for_left(&it.left);
        self.visit_expression(&it.right);
        self.visit_statement(&it.body);
    }

    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        let node = it.source.value.as_str().is_some_and(is_builtin);
        for specifier in it.specifiers.iter().flatten() {
            let local = match specifier {
                ImportDeclarationSpecifier::ImportSpecifier(specifier) => &specifier.local,
                ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => &specifier.local,
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => &specifier.local,
            };
            if node {
                self.edit(local.name.as_str(), |fact| fact.node = true);
            } else {
                self.edit(local.name.as_str(), |fact| fact.bad = true);
            }
        }
    }
}

impl Found {
    /// A `for` loop gives its name a new value at each turn, so the name is none this follows.
    fn for_left(&mut self, left: &ForStatementLeft<'_>) {
        let (outer_writing, outer_loop) = (self.writing, self.loop_left);
        match left {
            ForStatementLeft::VariableDeclaration(declaration) => {
                self.loop_left = true;
                self.visit_variable_declaration(declaration);
            }
            other => {
                self.writing = true;
                self.visit_assignment_target(other.to_assignment_target());
            }
        }
        (self.writing, self.loop_left) = (outer_writing, outer_loop);
    }
}
