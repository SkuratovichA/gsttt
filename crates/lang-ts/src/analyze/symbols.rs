//! The declarations of a file, what each one uses, and what the file runs as it loads, recorded in the one visit of the analyzer.
//!
//! A table of the top-level bindings is made before the visit, so that the visit only has to know where it is: the group of the declaration whose code it walks, and how far that code runs as the file loads. A reference to a binding of the table goes into that group and, when the code runs at load, into the list of the file. Both hold indices, and the names are made once, in [`Recorder::finish`].

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use gsttt_core::fact::{Declared, Heir, ImportKind, Line, LocalExport, MEMBER_SEPARATOR, Symbols};
use gsttt_core::text::LineIndex;
use oxc_ast::ast::{
    BindingPattern, ChainElement, Class, ClassElement, Declaration, ExportDefaultDeclaration,
    ExportDefaultDeclarationKind, Expression, ImportDeclaration, MethodDefinitionKind,
    ObjectPropertyKind, Program, PropertyKey, SimpleAssignmentTarget, Statement,
    TSInterfaceDeclaration, TSModuleReference, VariableDeclaration, VariableDeclarationKind,
    VariableDeclarator,
};
use oxc_span::{GetSpan, Span};

use super::constants::import_locals;
use super::roots::{constructed, declared_class, heads};
use super::{load_of, peel};
use crate::extra::{Export, ExportTarget};
use crate::shape::{
    dotted_name, is_export_key, is_open_object, is_parameter_property, literal_text, property_name,
    property_str,
};

/// The member that stands for a class used by its type alone: recorded as `Class.`, so that a question about the class reaches it and a question about one of its members does not.
pub(super) const TYPE_ONLY: &str = "";

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
    /// Whether code outside every type position refers to the entry, which is what makes an imported name a value of the file.
    used_as_value: bool,
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
struct ClassPlan<'a> {
    group: usize,
    entry: usize,
    /// What the class extends, spelled as the source writes it, when that is a name.
    head: Option<Cow<'a, str>>,
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

/// The names at the head of the type of a field, shared so that reading the field clones none of them.
type Heads<'a> = Rc<Vec<Cow<'a, str>>>;

/// The table of a file and what the walk has found against it.
#[derive(Debug, Default)]
pub(super) struct Recorder<'a> {
    /// The top-level declarations and the names imports bind.
    names: HashMap<&'a str, usize>,
    /// The members of top-level classes: the class, whether the name is private, and the name.
    members: HashMap<(usize, bool, &'a str), usize>,
    /// The names at the head of the type of each field of a top-level class, keyed like `members`. They are resolved when `this.<field>` is read, because the file may declare a type after the class.
    fields: HashMap<(usize, bool, &'a str), Heads<'a>>,
    /// The entry that stands for `Type.member`, by the type and the member, for a member read through a field whose type the file does not declare as a class with that member. It declares nothing: it has no lines, and only its name is kept.
    typed: HashMap<String, HashMap<String, usize>>,
    entries: Vec<Entry<'a>>,
    groups: Vec<Group>,
    group_members: Vec<usize>,
    /// One per top-level statement.
    plans: Vec<Plan>,
    /// Sorted by the start of the node.
    regions: Vec<(u32, Region)>,
    class_plans: Vec<ClassPlan<'a>>,
    /// The names that take the members of another, in the order met.
    heirs: Vec<Heir>,
    /// The entries that appear in `heirs` as the name that takes the members.
    heir_entries: HashSet<usize>,
    /// The start of each declarator that binds the names of a `require` as locals.
    modules: HashSet<u32>,
    /// The top-level functions and classes, and the variables that hold a function, an arrow or a class, by name.
    callables: HashSet<&'a str>,
    /// The top-level `const` variables that hold an object literal which names all its keys, with the keys: what `module.exports = variable` exports besides the variable.
    objects: HashMap<&'a str, Vec<String>>,
    /// The member names the file reads on a value of a class it does not know, sorted by the set.
    reads: BTreeSet<String>,
    /// What the file refers to as it loads, in the order met.
    on_load: Vec<usize>,
    place: Place,
    /// Whether the walk is inside a type, where a name is no use of its value.
    in_type: bool,
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
        for local in import_locals(import) {
            self.entry(local, Kind::Import, None);
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
            Declaration::FunctionDeclaration(function) => {
                let name = function.id.as_ref().map(|id| id.name.as_str());
                self.callables.extend(name);
                self.single(name, lines, Load::Later)
            }
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
                let plan = self.single(Some(interface.id.name.as_str()), lines, Load::Later);
                self.interface(interface);
                plan
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
                let plan = self.single(Some(interface.id.name.as_str()), lines, Load::Later);
                self.interface(interface);
                plan
            }
            // `export default name` declares nothing: it exports a name the file has.
            ExportDefaultDeclarationKind::Identifier(_) => Plan::Skip,
            other => {
                let plan = self.single(Some("default"), lines, Load::Static);
                if let Some(held) = other.as_expression().and_then(constructed) {
                    self.heir("default", &held);
                }
                plan
            }
        }
    }

    /// Records that `name` takes the members of `of`.
    fn heir(&mut self, name: &str, of: &str) {
        if let Some(&entry) = self.names.get(name) {
            self.heir_entries.insert(entry);
        }
        self.heirs.push(Heir {
            name: name.to_owned(),
            of: of.to_owned(),
        });
    }

    /// An interface takes the members of each interface it extends.
    fn interface(&mut self, interface: &TSInterfaceDeclaration<'a>) {
        let name = interface.id.name.as_str();
        for base in &interface.extends {
            self.heir(name, &base.type_name.to_string());
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
            if binds_a_require(declarator) {
                // The names declare nothing of the file, as the names of an import do.
                for id in declarator.id.get_binding_identifiers() {
                    self.entry(id.name.as_str(), Kind::Import, None);
                }
                self.modules.insert(declarator.span.start);
                continue;
            }
            let declared: Vec<usize> = declarator
                .id
                .get_binding_identifiers()
                .into_iter()
                .map(|id| self.entry(id.name.as_str(), Kind::Variable, Some(lines)))
                .collect();
            if let BindingPattern::BindingIdentifier(id) = &declarator.id
                && declarator.init.as_ref().is_some_and(|init| {
                    matches!(
                        init.get_inner_expression(),
                        Expression::FunctionExpression(_)
                            | Expression::ArrowFunctionExpression(_)
                            | Expression::ClassExpression(_)
                    )
                })
            {
                self.callables.insert(id.name.as_str());
            }
            // Only a `const` holds the same value for good.
            if variables.kind == VariableDeclarationKind::Const
                && let BindingPattern::BindingIdentifier(id) = &declarator.id
                && let Some(Expression::ObjectExpression(object)) = declarator
                    .init
                    .as_ref()
                    .map(Expression::get_inner_expression)
                && !object.properties.is_empty()
                && !is_open_object(object)
            {
                let keys = object
                    .properties
                    .iter()
                    .filter_map(|property| match property {
                        ObjectPropertyKind::ObjectProperty(property) => {
                            property_name(&property.key)
                        }
                        ObjectPropertyKind::SpreadProperty(_) => None,
                    });
                self.objects.insert(id.name.as_str(), keys.collect());
            }
            if variables.kind == VariableDeclarationKind::Const
                && let BindingPattern::BindingIdentifier(id) = &declarator.id
                && let Some(held) = declared_class(declarator)
            {
                self.heir(id.name.as_str(), &held);
            }
            let group = self.group(&declared);
            self.regions
                .push((declarator.span.start, Region::Declarator(group)));
        }
    }

    /// Whether the declarator that starts at `start` binds the names of a `require` as locals.
    pub(super) fn binds_module(&self, start: u32) -> bool {
        self.modules.contains(&start)
    }

    /// Whether `name` is a top-level function or class, or a variable that holds one.
    pub(super) fn is_callable(&self, name: &str) -> bool {
        self.callables.contains(name)
    }

    /// The keys of the object literal that the top-level `const` `name` holds, when it names them all.
    pub(super) fn object_keys(&self, name: &str) -> Option<&[String]> {
        self.objects.get(name).map(Vec::as_slice)
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
        self.callables.insert(name);
        let group = self.group(&[entry]);
        let head = class
            .heritage
            .as_ref()
            .and_then(|heritage| dotted_name(&heritage.expression));
        if let Some(head) = &head {
            self.heir(name, head);
        }
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
            head,
            members,
        });
        self.regions.push((class.span.start, Region::Class(plan)));
    }

    /// The names at the head of the type of each field that `element` declares: a property with an annotation or a `new` initializer, and the parameter properties of a constructor.
    fn field_types(&mut self, class: usize, element: &ClassElement<'a>) {
        match element {
            ClassElement::PropertyDefinition(field) if !field.computed => {
                let Some((name, private)) = member_key(&field.key) else {
                    return;
                };
                let found = Rc::make_mut(self.fields.entry((class, private, name)).or_default());
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
                        let found = Rc::make_mut(
                            self.fields
                                .entry((class, false, id.name.as_str()))
                                .or_default(),
                        );
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
        let (name, private) = member_key(key)?;
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
        let display = member_name(owner, private, name);
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
            used_as_value: false,
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

    /// Enters a type, and gives back whether the walk was in one.
    pub(super) fn enter_type(&mut self) -> bool {
        std::mem::replace(&mut self.in_type, true)
    }

    /// Goes back to where the walk was, once the type has been walked.
    pub(super) fn leave_type(&mut self, outer: bool) {
        self.in_type = outer;
    }

    /// Whether the file uses the top-level name `name` outside every type: a call, an argument, `Name.member`, `typeof Name` as an expression, `export default Name`.
    pub(super) fn used_as_value(&self, name: &str) -> bool {
        self.names
            .get(name)
            .and_then(|&binding| self.entries.get(binding))
            .is_some_and(|entry| entry.used_as_value)
    }

    /// A name the file's code refers to.
    pub(super) fn reference(&mut self, name: &str) {
        if let Some(&binding) = self.names.get(name) {
            self.mark_value(binding);
            self.note(binding);
        }
    }

    /// Records that the source names `binding` itself outside a type. A use through `this.field` does not: the type of the field is no use of the value of the name it is written with.
    fn mark_value(&mut self, binding: usize) {
        if !self.in_type
            && let Some(entry) = self.entries.get_mut(binding)
        {
            entry.used_as_value = true;
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

    /// Records that the file reads the member `name` on a value of a class it does not know. A name met before costs no allocation.
    pub(super) fn read(&mut self, name: &str) {
        if !self.reads.contains(name) {
            self.reads.insert(name.to_owned());
        }
    }

    /// A chain of member accesses from a name or `this`, `C.s` or `this.m`. The name at its base is a use, and so is the member of a class; `this.field` also uses the names the type of the field stands for, and `this.field.member` the member of each.
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
                // A member the class does not declare is one it takes from what it extends.
                if self.member_entry(class, false, member).is_none() {
                    self.super_use(member);
                }
                self.member_use(class, false, member);
                let read = names.get(2).and_then(Option::as_deref);
                if !self.field_use(class, false, member, read)
                    && let Some(read) = read
                {
                    self.read(read);
                }
            } else if let Some(read) = names.get(2).and_then(Option::as_deref) {
                // A class below the top level keeps no typed fields: the member is read on a value of unknown class.
                self.read(read);
            }
            return;
        }
        let Some(&binding) = self.names.get(root.as_str()) else {
            return;
        };
        self.mark_value(binding);
        let Some(member) = second else {
            self.note(binding);
            return;
        };
        let declared = self.member_entry(binding, false, member);
        let imported = self
            .entries
            .get(binding)
            .is_some_and(|entry| entry.kind == Kind::Import);
        // A name that takes the members of another is used by its name, like an import, so that the walk can pass the member on.
        match declared {
            Some(member) => self.note(member),
            None if imported || self.heir_entries.contains(&binding) => {
                let used = self.typed_entry(root, member);
                self.note(used);
            }
            None => self.note(binding),
        }
    }

    /// `super.member`, or `this.member` for a member the class does not declare: a use of `member` of what the class extends. A class that extends no name uses nothing.
    pub(super) fn super_use(&mut self, member: &str) {
        let head = self
            .place
            .class
            .and_then(|plan| self.class_plans.get(plan))
            .and_then(|plan| plan.head.clone());
        if let Some(head) = head {
            self.through_class(&head, Some(member));
        }
    }

    /// `this.#name` or `other.#name` inside a top-level class: a private name belongs to the class body that declares it. `member` is the member read from the field, as in `this.#name.member`.
    pub(super) fn private(&mut self, name: &str, member: Option<&str>) {
        let class = self
            .place
            .class
            .and_then(|plan| self.class_plans.get(plan))
            .map(|plan| plan.entry);
        if let Some(class) = class {
            self.member_use(class, true, name);
            // A field without a type declared in the file leaves the class of the member unknown.
            if !self.field_use(class, true, name, member)
                && let Some(member) = member
            {
                self.read(member);
            }
        }
    }

    /// The names at the head of the type of the field `name` of `class` that the file declares or binds, each spelled as the type writes it. A qualified head resolves through its first segment. A field read for one of its `member`s uses that member of each, and a field read as a value uses the type alone. A class of the file that declares the member is used through that member; any other type through an entry made for `Type.member`. Returns whether the field has a type, so that a caller can read the member on a value of unknown class when it has none.
    fn field_use(&mut self, class: usize, private: bool, name: &str, member: Option<&str>) -> bool {
        let Some(types) = self
            .fields
            .get(&(class, private, name))
            .map(Rc::clone)
            .filter(|types| !types.is_empty())
        else {
            return false;
        };
        for head in types.iter() {
            self.through_class(head, member);
        }
        true
    }

    /// A use of the type `head`, written as the source writes it, for its `member`, or for the type alone when there is none. A class of the file that declares the member is used through that member; any other type through an entry made for `Type.member`. The member [`TYPE_ONLY`] uses the type and none of its members.
    pub(super) fn through_class(&mut self, head: &str, member: Option<&str>) {
        let (root, qualified) = match head.split_once('.') {
            Some((root, rest)) => (root, Some(rest)),
            None => (head, None),
        };
        let Some(&binding) = self.names.get(root) else {
            return;
        };
        // A qualified head used for itself is the use of the name it writes, `lib.Foo`, and not of the bare root, which the walk reads as every name of the namespace.
        if let Some(rest) = qualified
            && member.is_none_or(|member| member == TYPE_ONLY)
        {
            let used = self.typed_entry(root, rest);
            self.note(used);
            return;
        }
        let Some(member) = member else {
            self.note(binding);
            return;
        };
        let declared = if qualified.is_some() {
            None
        } else {
            self.member_entry(binding, false, member)
        };
        let used = declared.unwrap_or_else(|| self.typed_entry(head, member));
        self.note(used);
    }

    /// Records that the source names the top-level `root` outside a type, as `new Root()` does.
    pub(super) fn value_of(&mut self, root: &str) {
        if let Some(&binding) = self.names.get(root) {
            self.mark_value(binding);
        }
    }

    /// The entry for `head.member`, made once.
    fn typed_entry(&mut self, head: &str, member: &str) -> usize {
        if let Some(&found) = self.typed.get(head).and_then(|members| members.get(member)) {
            return found;
        }
        let display = member_name(head, false, member);
        let found = self.push(Cow::Owned(display), Kind::Other, None);
        self.typed
            .entry(head.to_owned())
            .or_default()
            .insert(member.to_owned(), found);
        found
    }

    fn member_use(&mut self, class: usize, private: bool, name: &str) {
        if let Some(member) = self.member_entry(class, private, name) {
            self.note(member);
        }
    }

    /// The entry of the member `name` of `class`, when the class is one of the file with members.
    fn member_entry(&self, class: usize, private: bool, name: &str) -> Option<usize> {
        self.entries
            .get(class)
            .filter(|entry| entry.has_members)
            .and_then(|_| self.members.get(&(class, private, name)).copied())
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
        let mut heirs = std::mem::take(&mut self.heirs);
        heirs.sort_unstable_by(|a, b| (&a.name, &a.of).cmp(&(&b.name, &b.of)));
        heirs.dedup();
        Symbols {
            declared,
            on_load: self.names_of(self.on_load.iter().copied(), &ranks, &order),
            exports: local_exports(exports),
            heirs,
            reads: std::mem::take(&mut self.reads).into_iter().collect(),
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

/// The name a declaration lists a member under: `Class.member`, and `Class.#member` for a private one.
fn member_name(class: &str, private: bool, name: &str) -> String {
    let hash = if private { "#" } else { "" };
    format!("{class}{MEMBER_SEPARATOR}{hash}{name}")
}

/// The name a member of a class is declared under and whether it is private, which is how `members` and `fields` key it: `#cache` is `cache`, private. A key with no name written in the source has none.
fn member_key<'a>(key: &PropertyKey<'a>) -> Option<(&'a str, bool)> {
    match key {
        PropertyKey::PrivateIdentifier(identifier) => Some((identifier.name.as_str(), true)),
        other => property_str(other).map(|name| (name, false)),
    }
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

/// Whether the declarator is a `require` of a literal path whose names are plain locals: `const lib = require('x')` or `const { a, b: c } = require('x')`. A pattern with a rest element, a computed key, a default or a nested pattern may take names this cannot list.
fn binds_a_require(declarator: &VariableDeclarator<'_>) -> bool {
    let Some(load) = declarator
        .init
        .as_ref()
        .and_then(|init| load_of(peel(init)))
    else {
        return false;
    };
    if load.kind != ImportKind::Require || load.argument.and_then(literal_text).is_none() {
        return false;
    }
    match &declarator.id {
        BindingPattern::BindingIdentifier(_) => true,
        BindingPattern::ObjectPattern(object) => {
            object.rest.is_none()
                && !object.properties.is_empty()
                && object.properties.iter().all(|property| {
                    !property.computed
                        && property_name(&property.key).is_some()
                        && matches!(property.value, BindingPattern::BindingIdentifier(_))
                })
        }
        _ => false,
    }
}

/// The exports of a local under another name, sorted by the name they are exported as.
fn local_exports(exports: &[Export]) -> Vec<LocalExport> {
    let mut found: Vec<LocalExport> = exports
        .iter()
        .filter_map(|export| match &export.target {
            ExportTarget::Local(local) if *local != export.name && !is_export_key(local) => {
                Some(LocalExport {
                    name: export.name.clone(),
                    local: local.clone(),
                })
            }
            _ => None,
        })
        .collect();
    found.sort_unstable_by(|a, b| (&a.name, &a.local).cmp(&(&b.name, &b.local)));
    found.dedup();
    found
}
