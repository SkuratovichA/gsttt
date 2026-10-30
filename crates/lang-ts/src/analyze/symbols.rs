//! The declarations of a file, what each one uses, and what the file runs as it loads, recorded in the one visit of the analyzer.
//!
//! A table of the top-level bindings is made before the visit, so that the visit only has to know where it is: the group of the declaration whose code it walks, and how far that code runs as the file loads. A reference to a binding of the table goes into that group and, when the code runs at load, into the list of the file. Both hold indices, and the names are made once, in [`Recorder::finish`].

use std::borrow::Cow;
use std::collections::HashMap;

use gsttt_core::fact::{Declared, Line, LocalExport, MEMBER_SEPARATOR, Symbols};
use gsttt_core::text::LineIndex;
use oxc_ast::ast::{
    BindingPattern, ChainElement, Class, ClassElement, Declaration, ExportDefaultDeclaration,
    ExportDefaultDeclarationKind, Expression, ImportDeclaration, ImportDeclarationSpecifier,
    MethodDefinitionKind, Program, PropertyKey, SimpleAssignmentTarget, Statement,
    TSModuleReference, VariableDeclaration,
};
use oxc_span::{GetSpan, Span};

use super::roots::{constructed, heads};
use crate::extra::{Export, ExportTarget};
use crate::shape::{is_parameter_property, property_str};

/// The methods that change what a collection, a map or an array holds: a function that calls one on a top-level variable writes it.
const WRITERS: [&str; 12] = [
    "set", "add", "delete", "clear", "push", "pop", "shift", "unshift", "splice", "sort",
    "reverse", "fill",
];

/// How far the code being walked runs as the file loads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Load {
    /// A top-level class or variable statement, entered before the declaration inside it, which the walk finds by position.
    Pending,
    /// Code that waits for a call: a function body, a parameter default, an instance field.
    #[default]
    Later,
    /// Code that runs as the file loads where a name is read and nothing is called yet: an initializer, a static field or block, a computed key, `extends`.
    Static,
    /// Code under a call, `new`, a tagged template or a decorator, in code that runs as the file loads.
    Called,
    /// A top-level statement that is not a declaration: all of it runs, callbacks included.
    Always,
}

impl Load {
    /// Code in a function waits for a call, except where everything runs anyway.
    fn deferred(self) -> Self {
        match self {
            Self::Always => Self::Always,
            _ => Self::Later,
        }
    }
}

/// What a name of the table is, as far as the walk needs to tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A `var`, `let` or `const`, which other declarations may write.
    Variable,
    /// A name an import binds, which declares nothing of the file.
    Import,
    Other,
}

/// One name of the table.
#[derive(Debug)]
struct Entry<'a> {
    name: Cow<'a, str>,
    kind: Kind,
    /// The lines it spans, from the first of its declarations to the end of the last. `None` for an import.
    lines: Option<(Line, Line)>,
    /// The entries it uses: the declarations that write it at first, and the uses of its groups once the walk is done.
    uses: Vec<usize>,
    /// Whether the entry is a class with members of a name, so that `C.s` is looked up for the classes that have some.
    has_members: bool,
}

/// The code of one declaration, which is where the walk puts what it finds. A destructuring declares several names with one group, and an overload adds a group to a name that has one.
#[derive(Debug)]
struct Group {
    /// The entries it declares, as a range of `group_members`.
    start: usize,
    end: usize,
    uses: Vec<usize>,
}

/// What the walk does with a top-level statement.
#[derive(Debug, Clone, Copy)]
enum Plan {
    /// Nothing is declared and nothing runs: an import, an export of names, `export default name`.
    Skip,
    /// A statement that is not a declaration: all of it runs as the file loads.
    Load,
    /// A declaration whose code is one group, and how far that code runs at load.
    One(usize, Load),
    /// A class or a variable statement: the declarations inside are found by the start of their node.
    Inside,
}

/// A declaration inside a top-level statement, found by the start of its node.
#[derive(Debug, Clone, Copy)]
enum Region {
    Declarator(usize),
    Class(usize),
}

/// A top-level class: the group of its decorators, `extends` and members without a name, and the group of each member by its position in the body.
#[derive(Debug)]
struct ClassPlan {
    group: usize,
    entry: usize,
    members: Vec<Option<usize>>,
}

/// Where the walk is: the group whose code it reads, how far that code runs at load, and the top-level class whose `this` it is in.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Place {
    group: Option<usize>,
    load: Load,
    class: Option<usize>,
    /// How far the code around the function being walked runs at load, which is how far the decorators of its parameters do.
    around: Load,
}

/// The table of a file and what the walk has found against it.
#[derive(Debug, Default)]
pub(super) struct Recorder<'a> {
    /// The top-level declarations and the names imports bind.
    names: HashMap<&'a str, usize>,
    /// The members of top-level classes: the class, whether the name is private, and the name.
    members: HashMap<(usize, bool, &'a str), usize>,
    /// The names at the head of the type of each field of a top-level class, keyed like `members`. They are resolved when `this.<field>` is read, because the file may declare a type after the class.
    fields: HashMap<(usize, bool, &'a str), Vec<Cow<'a, str>>>,
    entries: Vec<Entry<'a>>,
    groups: Vec<Group>,
    group_members: Vec<usize>,
    /// One per top-level statement.
    plans: Vec<Plan>,
    /// Sorted by the start of the node.
    regions: Vec<(u32, Region)>,
    class_plans: Vec<ClassPlan>,
    /// What the file refers to as it loads, in the order met.
    on_load: Vec<usize>,
    place: Place,
    /// Whether the next function met is called on the spot, as `(() => {…})()` is.
    immediate: bool,
}

impl<'a> Recorder<'a> {
    /// The table of `program`: its declarations with their lines, what each statement is, and where the declarations inside classes and variable statements start.
    pub(super) fn collect(program: &Program<'a>, index: &LineIndex) -> Self {
        let mut recorder = Self::default();
        for statement in &program.body {
            let plan = recorder.plan(statement, index);
            recorder.plans.push(plan);
        }
        recorder.regions.sort_unstable_by_key(|&(start, _)| start);
        recorder
    }

    fn plan(&mut self, statement: &Statement<'a>, index: &LineIndex) -> Plan {
        let span = statement.span();
        match statement {
            Statement::ImportDeclaration(import) => {
                self.import(import);
                Plan::Skip
            }
            Statement::ExportDeclaration(export) => {
                self.declaration(&export.declaration, span, index)
            }
            Statement::ExportDefaultDeclaration(export) => self.default_export(export, span, index),
            Statement::ExportNamedDeclaration(_)
            | Statement::ExportFromDeclaration(_)
            | Statement::ExportAllDeclaration(_)
            | Statement::TSNamespaceExportDeclaration(_) => Plan::Skip,
            Statement::TSExportAssignment(_) => Plan::Load,
            other => match other.as_declaration() {
                Some(declaration) => self.declaration(declaration, span, index),
                None => Plan::Load,
            },
        }
    }

    /// The names an import binds.
    fn import(&mut self, import: &ImportDeclaration<'a>) {
        for specifier in import.specifiers.iter().flatten() {
            let local = match specifier {
                ImportDeclarationSpecifier::ImportSpecifier(item) => &item.local,
                ImportDeclarationSpecifier::ImportDefaultSpecifier(item) => &item.local,
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(item) => &item.local,
            };
            self.entry(local.name.as_str(), Kind::Import, None);
        }
    }

    fn declaration(
        &mut self,
        declaration: &Declaration<'a>,
        span: Span,
        index: &LineIndex,
    ) -> Plan {
        let lines = line_pair(index, span);
        match declaration {
            Declaration::VariableDeclaration(variables) => {
                self.variables(variables, span, index);
                Plan::Inside
            }
            Declaration::FunctionDeclaration(function) => self.single(
                function.id.as_ref().map(|id| id.name.as_str()),
                lines,
                Load::Later,
            ),
            Declaration::ClassDeclaration(class) => match &class.id {
                Some(id) => {
                    self.class(class, id.name.as_str(), span, index);
                    Plan::Inside
                }
                None => Plan::Load,
            },
            Declaration::TSTypeAliasDeclaration(alias) => {
                self.single(Some(alias.id.name.as_str()), lines, Load::Later)
            }
            Declaration::TSInterfaceDeclaration(interface) => {
                self.single(Some(interface.id.name.as_str()), lines, Load::Later)
            }
            Declaration::TSEnumDeclaration(declared) => {
                self.single(Some(declared.id.name.as_str()), lines, Load::Static)
            }
            Declaration::TSNamespaceDeclaration(namespace) => {
                self.single(Some(namespace.id.name.as_str()), lines, Load::Static)
            }
            // `import x = require('y')` declares `x` over the lines of the require, as `const x = require('y')` does, and the require is a call that runs as the file loads. `import A = B.C` only names something else of the file.
            Declaration::TSImportEqualsDeclaration(import) => {
                let load = if matches!(
                    import.module_reference,
                    TSModuleReference::ExternalModuleReference(_)
                ) {
                    Load::Called
                } else {
                    Load::Later
                };
                self.single(Some(import.id.name.as_str()), lines, load)
            }
            // `declare module 'x' {}` and `declare global {}` declare no name of the file, and what they refer to is kept as code that runs.
            Declaration::TSExternalModuleDeclaration(_) | Declaration::TSGlobalDeclaration(_) => {
                Plan::Load
            }
        }
    }

    fn default_export(
        &mut self,
        export: &ExportDefaultDeclaration<'a>,
        span: Span,
        index: &LineIndex,
    ) -> Plan {
        let lines = line_pair(index, span);
        match &export.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                let name = function
                    .id
                    .as_ref()
                    .map_or("default", |id| id.name.as_str());
                self.single(Some(name), lines, Load::Later)
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                let name = class.id.as_ref().map_or("default", |id| id.name.as_str());
                self.class(class, name, span, index);
                Plan::Inside
            }
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                self.single(Some(interface.id.name.as_str()), lines, Load::Later)
            }
            // `export default name` declares nothing: it exports a name the file has.
            ExportDefaultDeclarationKind::Identifier(_) => Plan::Skip,
            _ => self.single(Some("default"), lines, Load::Static),
        }
    }

    /// A declaration whose code is one group.
    fn single(&mut self, name: Option<&'a str>, lines: (Line, Line), load: Load) -> Plan {
        let Some(name) = name else {
            return Plan::Load;
        };
        let entry = self.entry(name, Kind::Other, Some(lines));
        Plan::One(self.group(&[entry]), load)
    }

    /// The names of each declarator, with a group and a region for the declarator. A statement with one declarator spans its keyword and `export` too.
    fn variables(&mut self, variables: &VariableDeclaration<'a>, span: Span, index: &LineIndex) {
        let alone = variables.declarations.len() == 1;
        for declarator in &variables.declarations {
            let lines = line_pair(index, if alone { span } else { declarator.span });
            let declared: Vec<usize> = declarator
                .id
                .get_binding_identifiers()
                .into_iter()
                .map(|id| self.entry(id.name.as_str(), Kind::Variable, Some(lines)))
                .collect();
            let group = self.group(&declared);
            self.regions
                .push((declarator.span.start, Region::Declarator(group)));
        }
    }

    /// A top-level class and the members that have a name.
    fn class(&mut self, class: &Class<'a>, name: &'a str, span: Span, index: &LineIndex) {
        let start = class
            .decorators
            .first()
            .map_or(span.start, |decorator| decorator.span.start.min(span.start));
        let entry = self.entry(
            name,
            Kind::Other,
            Some(line_pair(index, Span::new(start, span.end))),
        );
        let group = self.group(&[entry]);
        let mut members = Vec::with_capacity(class.body.body.len());
        for element in &class.body.body {
            self.field_types(entry, element);
            let member = self.member(entry, element, index);
            members.push(member.map(|member| self.group(&[member])));
        }
        let plan = self.class_plans.len();
        self.class_plans.push(ClassPlan {
            group,
            entry,
            members,
        });
        self.regions.push((class.span.start, Region::Class(plan)));
    }

    /// The names at the head of the type of each field that `element` declares: a property with an annotation or a `new` initializer, and the parameter properties of a constructor.
    fn field_types(&mut self, class: usize, element: &ClassElement<'a>) {
        match element {
            ClassElement::PropertyDefinition(field) if !field.computed => {
                let (name, private) = match &field.key {
                    PropertyKey::PrivateIdentifier(identifier) => (identifier.name.as_str(), true),
                    other => match property_str(other) {
                        Some(name) => (name, false),
                        None => return,
                    },
                };
                let found = self.fields.entry((class, private, name)).or_default();
                match field.type_annotation.as_deref() {
                    Some(annotation) => {
                        heads(&annotation.type_annotation, &mut |head| found.push(head));
                    }
                    None => found.extend(field.value.as_ref().and_then(constructed)),
                }
            }
            ClassElement::MethodDefinition(method)
                if method.kind == MethodDefinitionKind::Constructor =>
            {
                for parameter in &method.value.params.items {
                    let (Some(annotation), BindingPattern::BindingIdentifier(id)) =
                        (parameter.type_annotation.as_deref(), &parameter.pattern)
                    else {
                        continue;
                    };
                    if is_parameter_property(parameter) {
                        let found = self
                            .fields
                            .entry((class, false, id.name.as_str()))
                            .or_default();
                        heads(&annotation.type_annotation, &mut |head| found.push(head));
                    }
                }
            }
            _ => {}
        }
    }

    /// The entry of a member of `class` with a name written in the source. A computed key, a static block and an index signature declare none, so their code belongs to the class.
    fn member(
        &mut self,
        class: usize,
        element: &ClassElement<'a>,
        index: &LineIndex,
    ) -> Option<usize> {
        let (key, computed, decorators, span) = match element {
            ClassElement::MethodDefinition(method) => (
                &method.key,
                method.computed,
                &method.decorators,
                method.span,
            ),
            ClassElement::PropertyDefinition(field) => {
                (&field.key, field.computed, &field.decorators, field.span)
            }
            ClassElement::AccessorProperty(accessor) => (
                &accessor.key,
                accessor.computed,
                &accessor.decorators,
                accessor.span,
            ),
            ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => return None,
        };
        if computed {
            return None;
        }
        let (name, private) = match key {
            PropertyKey::PrivateIdentifier(identifier) => (identifier.name.as_str(), true),
            other => (property_str(other)?, false),
        };
        let start = decorators
            .first()
            .map_or(span.start, |decorator| decorator.span.start.min(span.start));
        let lines = line_pair(index, Span::new(start, span.end));
        if let Some(&found) = self.members.get(&(class, private, name)) {
            self.widen(found, lines);
            return Some(found);
        }
        let owner = self
            .entries
            .get(class)
            .map_or("", |entry| entry.name.as_ref());
        let hash = if private { "#" } else { "" };
        let display = format!("{owner}{MEMBER_SEPARATOR}{hash}{name}");
        let found = self.push(Cow::Owned(display), Kind::Other, Some(lines));
        self.members.insert((class, private, name), found);
        if let Some(entry) = self.entries.get_mut(class) {
            entry.has_members = true;
        }
        Some(found)
    }

    /// The entry of `name`, made when the table has none. A declaration of a name that an overload or a merge declared before it widens the lines.
    fn entry(&mut self, name: &'a str, kind: Kind, lines: Option<(Line, Line)>) -> usize {
        if let Some(&found) = self.names.get(name) {
            if let Some(lines) = lines {
                self.widen(found, lines);
                // A variable merged with an interface or a namespace stays a variable that others may write.
                if let Some(entry) = self.entries.get_mut(found)
                    && entry.kind != Kind::Variable
                {
                    entry.kind = kind;
                }
            }
            return found;
        }
        let found = self.push(Cow::Borrowed(name), kind, lines);
        self.names.insert(name, found);
        found
    }

    fn push(&mut self, name: Cow<'a, str>, kind: Kind, lines: Option<(Line, Line)>) -> usize {
        self.entries.push(Entry {
            name,
            kind,
            lines,
            uses: Vec::new(),
            has_members: false,
        });
        self.entries.len().saturating_sub(1)
    }

    fn widen(&mut self, entry: usize, (start, end): (Line, Line)) {
        if let Some(entry) = self.entries.get_mut(entry) {
            entry.lines = Some(match entry.lines {
                Some((first, last)) => (first.min(start), last.max(end)),
                None => (start, end),
            });
        }
    }

    fn group(&mut self, declared: &[usize]) -> usize {
        let start = self.group_members.len();
        self.group_members.extend_from_slice(declared);
        self.groups.push(Group {
            start,
            end: self.group_members.len(),
            uses: Vec::new(),
        });
        self.groups.len().saturating_sub(1)
    }

    fn region(&self, start: u32) -> Option<Region> {
        let found = self
            .regions
            .binary_search_by_key(&start, |&(key, _)| key)
            .ok()?;
        self.regions.get(found).map(|&(_, region)| region)
    }

    /// Starts the walk of the top-level statement at `position`.
    pub(super) fn statement(&mut self, position: usize) {
        self.place = match self.plans.get(position) {
            Some(&Plan::One(group, load)) => Place {
                group: Some(group),
                load,
                ..Place::default()
            },
            Some(Plan::Load) => Place {
                load: Load::Always,
                ..Place::default()
            },
            Some(Plan::Inside) => Place {
                load: Load::Pending,
                ..Place::default()
            },
            Some(Plan::Skip) | None => Place::default(),
        };
    }

    /// Goes back to where the walk was, once the code that changed it has been walked.
    pub(super) fn restore(&mut self, place: Place) {
        self.place = place;
        self.immediate = false;
    }

    /// Enters the declarator that starts at `start` when it is one of a top-level statement, and gives back where the walk was.
    pub(super) fn declarator(&mut self, start: u32) -> Place {
        let outer = self.place;
        if outer.load == Load::Pending
            && let Some(Region::Declarator(group)) = self.region(start)
        {
            self.place = Place {
                group: Some(group),
                load: Load::Static,
                ..Place::default()
            };
        }
        outer
    }

    /// Enters the class that starts at `start` when it is a top-level class. A class inside it has a `this` of its own.
    pub(super) fn class_at(&mut self, start: u32) -> Place {
        let outer = self.place;
        if outer.load == Load::Pending {
            if let Some(Region::Class(plan)) = self.region(start) {
                self.place = Place {
                    group: self.class_plans.get(plan).map(|plan| plan.group),
                    load: Load::Static,
                    class: Some(plan),
                    ..Place::default()
                };
            }
        } else if outer.class.is_some() {
            self.place.class = None;
        }
        outer
    }

    /// Enters the member at `position` of the body of a top-level class: its own group when it has a name, the class's when it has none.
    pub(super) fn element(&mut self, position: usize) -> Place {
        let outer = self.place;
        if let Some(plan) = outer.class.and_then(|plan| self.class_plans.get(plan)) {
            let member = plan.members.get(position).copied().flatten();
            self.place = Place {
                group: member.or(Some(plan.group)),
                load: Load::Static,
                class: outer.class,
                ..Place::default()
            };
        }
        outer
    }

    /// Code under a call, `new`, a tagged template or a decorator: what is read there runs when the code around it runs.
    pub(super) fn run(&mut self) -> Place {
        let outer = self.place;
        if matches!(outer.load, Load::Static | Load::Called) {
            self.place.load = Load::Called;
        }
        outer
    }

    /// Like [`Recorder::run`] for a call, whose callee may be a function called on the spot.
    pub(super) fn call(&mut self, callee: &Expression<'_>) -> Place {
        let outer = self.run();
        self.immediate = self.place.load == Load::Called
            && matches!(
                callee.get_inner_expression(),
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
            );
        outer
    }

    /// A function waits for a call, unless it is called where it stands.
    pub(super) fn function(&mut self) -> Place {
        let outer = self.place;
        if std::mem::take(&mut self.immediate) {
            return outer;
        }
        self.place.around = outer.load;
        self.place.load = outer.load.deferred();
        outer
    }

    /// The decorators of a parameter run when the class is defined, with the decorators of the method, and not when the method is called.
    pub(super) fn parameter(&mut self) -> Place {
        let outer = self.place;
        if matches!(outer.around, Load::Static | Load::Called) {
            self.place.load = Load::Called;
        }
        outer
    }

    /// Code that waits for a call: the value of an instance field.
    pub(super) fn later(&mut self) -> Place {
        let outer = self.place;
        self.place.load = outer.load.deferred();
        outer
    }

    /// A name the file's code refers to.
    pub(super) fn reference(&mut self, name: &str) {
        if let Some(&binding) = self.names.get(name) {
            self.note(binding);
        }
    }

    fn note(&mut self, binding: usize) {
        if matches!(self.place.load, Load::Called | Load::Always)
            && self.on_load.last() != Some(&binding)
        {
            self.on_load.push(binding);
        }
        if let Some(group) = self
            .place
            .group
            .and_then(|group| self.groups.get_mut(group))
            && group.uses.last() != Some(&binding)
        {
            group.uses.push(binding);
        }
    }

    /// A chain of member accesses from a name or `this`, `C.s` or `this.m`. The name at its base is a use, and so is the member of a class; `this.field` also uses the names the type of the field stands for.
    pub(super) fn chain(&mut self, names: &[Option<String>]) {
        let Some(Some(root)) = names.first() else {
            return;
        };
        let second = names.get(1).and_then(Option::as_deref);
        if root == "this" {
            let class = self
                .place
                .class
                .and_then(|plan| self.class_plans.get(plan))
                .map(|plan| plan.entry);
            if let (Some(class), Some(member)) = (class, second) {
                self.member_use(class, false, member);
                self.field_use(class, false, member);
            }
            return;
        }
        let Some(&binding) = self.names.get(root.as_str()) else {
            return;
        };
        self.note(binding);
        if let Some(member) = second {
            self.member_use(binding, false, member);
        }
    }

    /// `this.#name` or `other.#name` inside a top-level class: a private name belongs to the class body that declares it.
    pub(super) fn private(&mut self, name: &str) {
        let class = self
            .place
            .class
            .and_then(|plan| self.class_plans.get(plan))
            .map(|plan| plan.entry);
        if let Some(class) = class {
            self.member_use(class, true, name);
            self.field_use(class, true, name);
        }
    }

    /// The names at the head of the type of the field `name` of `class` that the file declares or binds. A qualified head resolves through its first segment.
    fn field_use(&mut self, class: usize, private: bool, name: &str) {
        let Some(types) = self.fields.get(&(class, private, name)) else {
            return;
        };
        let bindings: Vec<usize> = types
            .iter()
            .filter_map(|head| {
                let root = head.split_once('.').map_or(head.as_ref(), |(root, _)| root);
                self.names.get(root).copied()
            })
            .collect();
        for binding in bindings {
            self.note(binding);
        }
    }

    fn member_use(&mut self, class: usize, private: bool, name: &str) {
        if self
            .entries
            .get(class)
            .is_some_and(|entry| entry.has_members)
            && let Some(&member) = self.members.get(&(class, private, name))
        {
            self.note(member);
        }
    }

    /// The declaration being walked writes the top-level variable `base`.
    pub(super) fn write(&mut self, base: &str) {
        let Some(&binding) = self.names.get(base) else {
            return;
        };
        if !self
            .entries
            .get(binding)
            .is_some_and(|entry| entry.kind == Kind::Variable)
        {
            return;
        }
        let Some(group) = self.place.group.and_then(|group| self.groups.get(group)) else {
            return;
        };
        let writers = group.start..group.end;
        for position in writers {
            if let Some(&writer) = self.group_members.get(position)
                && let Some(entry) = self.entries.get_mut(binding)
                && entry.uses.last() != Some(&writer)
            {
                entry.uses.push(writer);
            }
        }
    }

    /// `cache.set(key, value)` and `list.items.push(x)` write the variable they start from.
    pub(super) fn call_write(&mut self, names: &[Option<String>]) {
        if let [Some(root), .., Some(method)] = names
            && WRITERS.contains(&method.as_str())
        {
            self.write(root);
        }
    }

    /// The facts of the file, with every list sorted by name and every name made once.
    pub(super) fn finish(mut self, exports: &[Export]) -> Symbols {
        for group in &self.groups {
            for position in group.start..group.end {
                if let Some(&declared) = self.group_members.get(position)
                    && let Some(entry) = self.entries.get_mut(declared)
                {
                    entry.uses.extend_from_slice(&group.uses);
                }
            }
        }
        let mut order: Vec<usize> = (0..self.entries.len()).collect();
        order.sort_unstable_by(|&a, &b| self.name(a).cmp(self.name(b)));
        let mut ranks = vec![0; order.len()];
        for (rank, &entry) in order.iter().enumerate() {
            if let Some(slot) = ranks.get_mut(entry) {
                *slot = rank;
            }
        }
        let mut declared: Vec<Declared> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(position, entry)| {
                let (line, end) = entry.lines?;
                let used = entry.uses.iter().copied().filter(|&used| used != position);
                Some(Declared {
                    name: entry.name.as_ref().to_owned(),
                    line,
                    end,
                    uses: self.names_of(used, &ranks, &order),
                })
            })
            .collect();
        declared.sort_unstable_by(|a, b| (a.line, &a.name).cmp(&(b.line, &b.name)));
        Symbols {
            declared,
            on_load: self.names_of(self.on_load.iter().copied(), &ranks, &order),
            exports: local_exports(exports),
        }
    }

    fn name(&self, entry: usize) -> &str {
        self.entries
            .get(entry)
            .map_or("", |entry| entry.name.as_ref())
    }

    /// The names of `entries`, sorted, once each. `ranks` gives the place of each entry in the order of the names, and `order` the entry at each place.
    fn names_of(
        &self,
        entries: impl Iterator<Item = usize>,
        ranks: &[usize],
        order: &[usize],
    ) -> Vec<String> {
        let mut places: Vec<usize> = entries
            .filter_map(|entry| ranks.get(entry).copied())
            .collect();
        places.sort_unstable();
        places.dedup();
        places
            .into_iter()
            .filter_map(|place| order.get(place))
            .map(|&entry| self.name(entry).to_owned())
            .collect()
    }
}

/// The lines a span starts and ends on.
fn line_pair(index: &LineIndex, span: Span) -> (Line, Line) {
    let last = span.end.saturating_sub(1).max(span.start);
    (index.line(span.start), index.line(last))
}

/// The name an assignment target or a chain of accesses starts from: `cache` for `cache`, `cache.size` and `cache[key].items`.
pub(super) fn base_name<'a>(expr: &Expression<'a>) -> Option<&'a str> {
    match expr.get_inner_expression() {
        Expression::Identifier(identifier) => Some(identifier.name.as_str()),
        Expression::StaticMemberExpression(member) => base_name(&member.object),
        Expression::ComputedMemberExpression(member) => base_name(&member.object),
        Expression::PrivateFieldExpression(member) => base_name(&member.object),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::StaticMemberExpression(member) => base_name(&member.object),
            ChainElement::ComputedMemberExpression(member) => base_name(&member.object),
            ChainElement::PrivateFieldExpression(member) => base_name(&member.object),
            _ => None,
        },
        _ => None,
    }
}

/// The name an assignment target starts from: `count` for `count = 1`, `count++` and `count.total = 1`.
pub(super) fn target_base<'a>(target: &SimpleAssignmentTarget<'a>) -> Option<&'a str> {
    match target {
        SimpleAssignmentTarget::AssignmentTargetIdentifier(identifier) => {
            Some(identifier.name.as_str())
        }
        SimpleAssignmentTarget::StaticMemberExpression(member) => base_name(&member.object),
        SimpleAssignmentTarget::ComputedMemberExpression(member) => base_name(&member.object),
        SimpleAssignmentTarget::PrivateFieldExpression(member) => base_name(&member.object),
        other => other.get_expression().and_then(base_name),
    }
}

/// The exports of a local under another name, sorted by the name they are exported as.
fn local_exports(exports: &[Export]) -> Vec<LocalExport> {
    let mut found: Vec<LocalExport> = exports
        .iter()
        .filter_map(|export| match &export.target {
            ExportTarget::Local(local) if *local != export.name => Some(LocalExport {
                name: export.name.clone(),
                local: local.clone(),
            }),
            _ => None,
        })
        .collect();
    found.sort_unstable_by(|a, b| (&a.name, &a.local).cmp(&(&b.name, &b.local)));
    found.dedup();
    found
}
