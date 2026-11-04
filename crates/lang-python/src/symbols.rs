//! The declarations of a Python file, what each of them uses and what runs as the file loads: the facts that let a walk by name tell the files that use one function of a file from those that use another.
//!
//! The reading goes in two passes. [`Table`] reads what the top level binds and what the classes hold, so that at a use the recorder knows whether a name is the file's own and which members a class has. The recorder then walks the code once and notes, under every declaration it is in, the names the code refers to.

mod locals;
mod table;

use std::collections::{HashMap, HashSet};
use std::iter;
use std::mem;
use std::rc::Rc;

use gsttt_core::fact::{Declared, Heir, Line, MEMBER_SEPARATOR, Symbols, Uses};
use gsttt_core::text::LineIndex;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    CmpOp, Comprehension, ExceptHandler, Expr, ExprCall, ExprContext, ExprLambda, ExprSubscript,
    Parameters, Pattern, Stmt, StmtAnnAssign, StmtAugAssign, StmtClassDef, StmtFor,
    StmtFunctionDef, StmtIf, StmtTypeAlias, StmtWith,
};
use ruff_python_stdlib::builtins::{is_python_builtin, python_magic_globals};
use ruff_text_size::{Ranged, TextRange};

use crate::site::write_dotted;
use table::{
    Binder, MAX_CHAIN, Table, Ty, base_of, captured, is_class_name, is_identifier, is_static,
    target_names,
};

/// The newest minor version the builtins are taken from: a name added in a later version is still not bound by the file.
const AMBIENT_MINOR: u8 = 14;

/// The name a declaration is listed under for the block that runs when the file is run as a script, which nothing imports.
const MAIN: &str = "__main__";

// A use is written with the separator the core splits a member on, and a Python path is written with a dot.
const _: () = assert!(MEMBER_SEPARATOR == '.');

/// What reading the code of a file gives.
#[derive(Debug)]
pub(crate) struct Read {
    pub(crate) symbols: Symbols,
    /// What an importer can name that the file itself binds or hands out, sorted: see `table::exports` for the imports among it. What a star import brings in is not listed, because the walk follows the star lines to the files they lead to.
    pub(crate) exports: Vec<String>,
}

/// Reads the declarations of a file that parsed without error. `star` says that it holds a `from m import *`.
///
/// A star import binds names the file does not spell out, so a use of a name nothing in the file binds may be a use of one of those, and no declaration can record which. A file that has one and reads such a name gives `None`, and the walk follows it whole.
pub(crate) fn read(suite: &[Stmt], lines: &LineIndex, star: bool) -> Option<Read> {
    let table = Table::of(suite);
    let exports = table::exports(suite);
    let mut recorder = Recorder::new(&table, lines, star);
    recorder.visit_body(suite);
    if star && recorder.unbound {
        return None;
    }
    Some(Read {
        symbols: recorder.finish(),
        exports,
    })
}

/// When the code being read runs, which decides whether a name it refers to can fail the file as it loads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Load {
    /// In a function body or an annotation: not as the file loads.
    Later,
    /// In an initialiser, a base or a class body: as the file loads, but only a call in it runs code.
    Static,
    /// Under a call or a decorator that runs as the file loads.
    Called,
    /// In a statement of the top level that declares nothing.
    Always,
}

impl Load {
    /// The load of the value a declaration is given.
    fn value(self) -> Self {
        if self == Self::Always {
            Self::Static
        } else {
            self
        }
    }

    /// The load of what a call runs: the callee and its arguments.
    fn called(self) -> Self {
        if self == Self::Static {
            Self::Called
        } else {
            self
        }
    }

    /// The load of a decorator, which runs as the definition does.
    fn decorated(self) -> Self {
        if self == Self::Later {
            self
        } else {
            Self::Called
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Function,
    Lambda,
    Comprehension,
    Class,
}

/// A scope the code being read is in.
#[derive(Debug)]
struct Scope {
    kind: Kind,
    /// The names the scope binds.
    locals: HashSet<String>,
    /// What a local is known to hold: an instance of a class, as the chain it is written with, or a builtin value.
    types: HashMap<String, Ty>,
    /// The locals that held, at some point, something the file cannot tell the type of: a parameter without a known type, a name a loop or an unknown value rebound. Such a local is no builtin however it is assigned later, because the code that reads it may come after another path.
    unsure: HashSet<String>,
    /// The locals that are bound to values of different types somewhere in the function, which are typed nowhere in it: see [`locals::mixed`].
    mixed: HashSet<String>,
    /// The first parameter of a method and the class it is a method of.
    receiver: Option<(String, Rc<str>)>,
    /// The name of a class scope, as its members are named.
    class: Option<Rc<str>>,
}

impl Scope {
    fn new(kind: Kind, locals: HashSet<String>) -> Self {
        Self {
            kind,
            locals,
            types: HashMap::new(),
            unsure: HashSet::new(),
            mixed: HashSet::new(),
            receiver: None,
            class: None,
        }
    }
}

/// What a name read in the code is.
enum Binding {
    /// The first parameter of a method of the class.
    Receiver(Rc<str>),
    /// A local that holds an instance of a class, or a builtin value.
    Typed(Ty),
    /// A local of a function that nothing is known of.
    Local,
    /// A name a class body binds, with the name of the class as its members are named.
    Member(Rc<str>),
    /// A name of the file: something it declares, or an import.
    File,
    /// A name that neither the code nor the file binds, such as a builtin.
    Unbound,
}

/// One declaration.
#[derive(Debug)]
struct Entry {
    name: String,
    line: Line,
    end: Line,
    uses: Uses,
    /// A member that a method creates by assigning `self.name`. Its span is the first assignment, and a declaration of the member in the class body takes its place.
    weak: bool,
}

struct Recorder<'a> {
    table: &'a Table,
    lines: &'a LineIndex,
    /// Whether the file holds a `from m import *`.
    star: bool,
    entries: Vec<Entry>,
    index: HashMap<String, usize>,
    /// The declarations the code being read belongs to.
    owners: Vec<usize>,
    /// The variable a function assigns through `global`, with the function.
    writers: Vec<(String, usize)>,
    on_load: Uses,
    reads: Uses,
    /// The byte offset of the code being read, where a use it finds is written.
    at: u32,
    heirs: Vec<Heir>,
    overrides: Vec<Heir>,
    scopes: Vec<Scope>,
    load: Load,
    /// Whether the expression being read is an annotation, where a string is a type.
    annotation: bool,
    /// Whether the code reads a name that nothing in the file binds and that is no builtin: with a star import, it may come from there.
    unbound: bool,
    /// The buffer a name is written into before it is noted, so that a use costs no `String` of its own.
    scratch: String,
    /// The declarations whose own name is read from outside the file: one that binds a name the file imports, such as `X = wrap(X)` after `from m import X`, and the first one of a name in a file with a star import, such as `X = X + [1]` after `from m import *`. The `X` they read is the import, or the one the star brings in.
    rebinding: HashSet<usize>,
}

impl<'a> Recorder<'a> {
    fn new(table: &'a Table, lines: &'a LineIndex, star: bool) -> Self {
        Self {
            table,
            lines,
            star,
            entries: Vec::new(),
            index: HashMap::new(),
            owners: Vec::new(),
            writers: Vec::new(),
            on_load: Uses::default(),
            reads: Uses::default(),
            at: 0,
            heirs: Vec::new(),
            overrides: Vec::new(),
            scopes: Vec::new(),
            load: Load::Always,
            annotation: false,
            unbound: false,
            scratch: String::new(),
            rebinding: HashSet::new(),
        }
    }

    /// The lines a range starts and ends on.
    fn span(&self, range: TextRange) -> (Line, Line) {
        let start = u32::from(range.start());
        let last = u32::from(range.end()).saturating_sub(1).max(start);
        (self.lines.line(start), self.lines.line(last))
    }

    /// The range of a definition, from its first decorator.
    fn definition(range: TextRange, decorators: &[ruff_python_ast::Decorator]) -> TextRange {
        let start = decorators.first().map_or(range.start(), |decorator| {
            decorator.start().min(range.start())
        });
        TextRange::new(start, range.end())
    }

    /// Whether what the code declares is a declaration of the file: outside every function.
    fn declaring(&self) -> bool {
        self.scopes.iter().all(|scope| scope.kind == Kind::Class)
    }

    /// The class whose body the code is directly in.
    fn class_body(&self) -> Option<&Rc<str>> {
        if !self.declaring() {
            return None;
        }
        self.scopes.last()?.class.as_ref()
    }

    /// `name` as a declaration of the file lists it: `Class.name` in a class body.
    fn qualified(&self, name: &str) -> String {
        self.class_body().map_or_else(
            || name.to_owned(),
            |class| format!("{class}{MEMBER_SEPARATOR}{name}"),
        )
    }

    fn declare(&mut self, name: String, span: (Line, Line), weak: bool) -> usize {
        if let Some(&position) = self.index.get(&name) {
            if let Some(entry) = self.entries.get_mut(position)
                && !weak
            {
                if entry.weak {
                    entry.line = span.0;
                    entry.end = span.1;
                    entry.weak = false;
                } else {
                    entry.line = entry.line.min(span.0);
                    entry.end = entry.end.max(span.1);
                }
            }
            return position;
        }
        let position = self.entries.len();
        self.index.insert(name.clone(), position);
        self.entries.push(Entry {
            name,
            line: span.0,
            end: span.1,
            uses: Uses::default(),
            weak,
        });
        position
    }

    /// Notes that the code refers to `used`: under every declaration it is in and, when it runs as the file loads, in what the file runs then.
    fn note(&mut self, used: &str) {
        let line = self.lines.line(self.at);
        for &owner in &self.owners {
            if let Some(entry) = self.entries.get_mut(owner) {
                entry.uses.insert(used, line);
            }
        }
        if matches!(self.load, Load::Called | Load::Always) {
            self.on_load.insert(used, line);
        }
    }

    /// Notes the name that `parts` make, joined as a declaration lists a member.
    fn note_path<'p>(&mut self, parts: impl IntoIterator<Item = &'p str>) {
        let mut name = mem::take(&mut self.scratch);
        name.clear();
        for (at, part) in parts.into_iter().enumerate() {
            if at > 0 {
                name.push(MEMBER_SEPARATOR);
            }
            name.push_str(part);
        }
        self.note(&name);
        self.scratch = name;
    }

    fn read_member(&mut self, member: &str) {
        let line = self.lines.line(self.at);
        self.reads.insert(member, line);
    }

    /// Notes each member as read, and each as read right on the one before it. A walk by name needs the pairs to tell `self.poetry.file.read()` from a file that reads `file` and `read` apart.
    fn read_members(&mut self, members: &[&str]) {
        for member in members {
            self.read_member(member);
        }
        self.read_pairs(members);
    }

    /// Notes each member as read right on the one before it, as `on.member`, and not the members themselves.
    fn read_pairs(&mut self, members: &[&str]) {
        for (on, member) in members.iter().zip(members.iter().skip(1)) {
            self.read_member(&[on, ".", member].concat());
        }
    }

    /// Notes the first of `rest` as read right on `first`, which is read on a value that may be of the repository.
    fn read_after(&mut self, first: &str, rest: &[&str]) {
        if let Some(next) = rest.first() {
            self.read_member(&[first, ".", next].concat());
        }
    }

    fn lookup(&self, root: &str) -> Binding {
        let innermost = self.scopes.len();
        for (depth, scope) in self.scopes.iter().enumerate().rev() {
            // The names of a class body are visible to that body and to nothing inside it.
            if scope.kind == Kind::Class && depth + 1 != innermost {
                continue;
            }
            if let Some((name, class)) = &scope.receiver
                && name == root
            {
                return Binding::Receiver(Rc::clone(class));
            }
            if let Some(ty) = scope.types.get(root) {
                return Binding::Typed(ty.clone());
            }
            if scope.locals.contains(root) {
                return match &scope.class {
                    Some(class) if scope.kind == Kind::Class => Binding::Member(Rc::clone(class)),
                    _ => Binding::Local,
                };
            }
        }
        if self.table.knows(root) {
            Binding::File
        } else {
            Binding::Unbound
        }
    }

    /// Notes a name read in the code and the attributes read on it.
    fn chain(&mut self, root: &str, segs: &[&str]) {
        let binding = self.lookup(root);
        // Whatever is read on a builtin value is no declaration of the file.
        if matches!(binding, Binding::Typed(Ty::Builtin)) {
            return;
        }
        let (segs, beyond) = segs.split_at(segs.len().min(MAX_CHAIN));
        self.read_members(beyond);
        match binding {
            Binding::Receiver(class) => self.through_receiver(&class, segs),
            Binding::Typed(Ty::Class(class)) => self.through_type(&class, segs),
            Binding::Typed(Ty::Builtin) => {}
            Binding::Local => self.read_members(segs),
            Binding::Unbound => {
                self.unbound |= !is_ambient(root);
                self.read_members(segs);
            }
            Binding::Member(class) => {
                self.note_path([&*class, root].into_iter().chain(segs.iter().copied()));
                self.read_pairs(segs);
            }
            Binding::File => {
                self.note_path(iter::once(root).chain(segs.iter().copied()));
                self.read_pairs(segs);
            }
        }
    }

    /// Notes what a read of `member` through an instance of `class` stands for: the declaration of the member, or the member of the base that has it. A class of the file without it is read as a member by name, and a class the file does not bind is used through the member.
    fn member_of(&mut self, class: &str, member: &str) {
        let table = self.table;
        if table.member_uses(class, member, &mut |owner, name| {
            self.note_path([owner, name]);
        }) {
            return;
        }
        if table.has_class(class) {
            self.read_member(member);
        } else {
            self.note_path([class, member]);
        }
    }

    /// `self.member` and what is read on it, in a method of `class`.
    fn through_receiver(&mut self, class: &str, segs: &[&str]) {
        let Some((first, rest)) = segs.split_first() else {
            return;
        };
        self.member_of(class, first);
        match self.table.field_type(class, first) {
            Some(Ty::Class(ty)) => {
                self.read_after(first, rest);
                self.through_type(&ty, rest);
            }
            Some(Ty::Builtin) => {}
            None => {
                self.read_after(first, rest);
                self.read_members(rest);
            }
        }
    }

    /// The attributes `segs` read on an instance of `ty`, each followed into the type of the field it is when that is known. A field that is a builtin value ends the chain: what is read on it is no declaration of the file.
    fn through_type(&mut self, ty: &Rc<str>, segs: &[&str]) {
        let mut ty = Rc::clone(ty);
        let mut segs = segs;
        while let Some((first, rest)) = segs.split_first() {
            self.member_of(&ty, first);
            match self.table.field_type(&ty, first) {
                Some(Ty::Class(next)) => {
                    self.read_after(first, rest);
                    ty = next;
                    segs = rest;
                }
                Some(Ty::Builtin) => return,
                None => {
                    self.read_after(first, rest);
                    self.read_members(rest);
                    return;
                }
            }
        }
    }

    /// The class a call makes an instance of, when it calls a class the file binds.
    fn class_called(&mut self, call: &ExprCall) -> Option<Rc<str>> {
        let mut chain = mem::take(&mut self.scratch);
        chain.clear();
        let called = write_dotted(&call.func, &mut chain)
            && !chain.contains('?')
            && is_class_name(&chain)
            && chain
                .split(MEMBER_SEPARATOR)
                .next()
                .is_some_and(|root| matches!(self.lookup(root), Binding::File));
        let class = called.then(|| Rc::from(chain.as_str()));
        self.scratch = chain;
        class
    }

    /// Making an instance runs the `__init__` of the class, or of the base that has it.
    fn constructed(&mut self, class: &str) {
        let table = self.table;
        if table.has_class(class) {
            table.member_uses(class, "__init__", &mut |owner, name| {
                self.note_path([owner, name]);
            });
        } else {
            self.note_path([class, "__init__"]);
        }
    }

    /// `super().member`, which is that member of each base of the class the code is in.
    fn through_super(&mut self, segs: &[&str]) {
        let Some((member, rest)) = segs.split_first() else {
            return;
        };
        let class = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.receiver.as_ref().map(|(_, class)| Rc::clone(class)));
        self.read_after(member, rest);
        let table = self.table;
        for base in class.iter().flat_map(|class| table.bases(class)) {
            let found = table.member_uses(base, member, &mut |owner, name| {
                self.note_path([owner, name]);
            });
            if !found {
                self.note_path([base.as_str(), member]);
            }
        }
        self.read_members(rest);
    }

    /// `expr`, an attribute chain, read as a name and what is read on it.
    fn attribute(&mut self, expr: &'a Expr) {
        // A use is written where its last attribute is, which is on a later line than the root of a chain broken across lines.
        let member = match expr {
            Expr::Attribute(outer) => Some(u32::from(outer.attr.range().start())),
            _ => None,
        };
        if let Some(at) = member {
            self.at = at;
        }
        let mut segs: Vec<&'a str> = Vec::new();
        let mut root = expr;
        while let Expr::Attribute(attribute) = root {
            segs.push(attribute.attr.as_str());
            root = &attribute.value;
        }
        segs.reverse();
        match root {
            Expr::Name(name) => self.chain(name.id.as_str(), &segs),
            Expr::Call(call) if is_super(call) => self.through_super(&segs),
            Expr::Call(call) => {
                self.call(call);
                // The call may have been a chain of its own, which moved the position on.
                if let Some(at) = member {
                    self.at = at;
                }
                match self.class_called(call) {
                    Some(class) => self.through_type(&class, &segs),
                    None if self.is_builtin_value(root) => {}
                    None => {
                        // What a method call gives is read on the method, which a chain through its return type needs.
                        if let Expr::Attribute(method) = &*call.func {
                            self.read_after(method.attr.as_str(), &segs);
                        }
                        self.read_members(&segs);
                    }
                }
            }
            other => {
                self.visit_expr(other);
                if let Some(at) = member {
                    self.at = at;
                }
                if !self.is_builtin_value(other) {
                    self.read_members(&segs);
                }
            }
        }
    }

    fn call(&mut self, call: &'a ExprCall) {
        let saved = self.load;
        self.load = saved.called();
        self.visit_expr(&call.func);
        if let Some(class) = self.class_called(call) {
            self.at = u32::from(call.range().start());
            self.constructed(&class);
        }
        visitor::walk_arguments(self, &call.arguments);
        self.load = saved;
    }

    fn lambda(&mut self, lambda: &'a ExprLambda) {
        let saved = self.load;
        self.load = saved.value();
        let mut locals = HashSet::new();
        if let Some(parameters) = &lambda.parameters {
            self.parameters(parameters);
            locals.extend(
                parameters
                    .iter()
                    .map(|parameter| parameter.name().as_str().to_owned()),
            );
        }
        // A lambda runs when it is called, which is as the file loads only where the code around it runs.
        self.load = if saved == Load::Static {
            Load::Later
        } else {
            saved
        };
        self.scopes.push(Scope::new(Kind::Lambda, locals));
        self.visit_expr(&lambda.body);
        self.scopes.pop();
        self.load = saved;
    }

    fn comprehension(&mut self, generators: &'a [Comprehension], elements: &[&'a Expr]) {
        let mut names = Vec::new();
        for generator in generators {
            target_names(&generator.target, &mut names);
        }
        let locals = names.into_iter().map(str::to_owned).collect();
        // The first iterable is evaluated where the comprehension stands, so in a class body it sees what the body binds.
        if let Some(first) = generators.first() {
            self.visit_expr(&first.iter);
        }
        self.scopes.push(Scope::new(Kind::Comprehension, locals));
        for (at, generator) in generators.iter().enumerate() {
            if at > 0 {
                self.visit_expr(&generator.iter);
            }
            for condition in &generator.ifs {
                self.visit_expr(condition);
            }
            self.visit_expr(&generator.target);
        }
        for element in elements {
            self.visit_expr(element);
        }
        self.scopes.pop();
    }

    /// The names a string annotation writes: a string is a type where an annotation is read.
    fn string_annotation(&mut self, text: &str) {
        let pieces = text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'));
        for piece in pieces {
            let mut parts = piece.split(MEMBER_SEPARATOR);
            let Some(root) = parts.next() else { continue };
            let rest: Vec<&str> = parts.collect();
            if is_identifier(root) && rest.iter().all(|part| is_identifier(part)) {
                self.chain(root, &rest);
            }
        }
    }

    /// The typing wrapper a subscript is of, when it is `Literal` or `Annotated`: what follows the type of an `Annotated`, and everything in a `Literal`, is a value and no type, so a string there names nothing.
    fn typing_wrapper(&mut self, value: &Expr) -> Option<&'static str> {
        let mut chain = mem::take(&mut self.scratch);
        chain.clear();
        let found = write_dotted(value, &mut chain) && !chain.contains('?');
        let wrapper = ["Literal", "Annotated"]
            .into_iter()
            .find(|&name| found && self.table.is_library_name(&chain, &[name]));
        self.scratch = chain;
        wrapper
    }

    /// A subscript inside an annotation: `Literal["a"]` and the metadata of `Annotated[T, "a"]` hold strings that are no types.
    fn annotation_subscript(&mut self, subscript: &'a ExprSubscript) {
        self.visit_expr(&subscript.value);
        match (self.typing_wrapper(&subscript.value), &*subscript.slice) {
            (Some("Annotated"), Expr::Tuple(tuple)) => {
                let mut elements = tuple.elts.iter();
                if let Some(first) = elements.next() {
                    self.visit_expr(first);
                }
                let was = mem::replace(&mut self.annotation, false);
                for metadata in elements {
                    self.visit_expr(metadata);
                }
                self.annotation = was;
            }
            (Some("Literal"), slice) => {
                let was = mem::replace(&mut self.annotation, false);
                self.visit_expr(slice);
                self.annotation = was;
            }
            (_, slice) => self.visit_expr(slice),
        }
    }

    fn annotation(&mut self, annotation: &'a Expr) {
        let (saved, was) = (self.load, mem::replace(&mut self.annotation, true));
        self.load = Load::Later;
        self.visit_expr(annotation);
        self.load = saved;
        self.annotation = was;
    }

    /// The annotations and the defaults of parameters, which are read where the function is defined.
    fn parameters(&mut self, parameters: &'a Parameters) {
        for parameter in parameters {
            if let Some(annotation) = parameter.annotation() {
                self.annotation(annotation);
            }
            if let Some(default) = parameter.default() {
                self.visit_expr(default);
            }
        }
    }

    fn function(&mut self, function: &'a StmtFunctionDef) {
        let saved_owners = self.owners.clone();
        let saved_load = self.load;
        let class = self.class_body().cloned();
        self.rebind(function.name.as_str());
        if self.declaring() {
            let range = Self::definition(function.range(), &function.decorator_list);
            let name = self.qualified(function.name.as_str());
            let position = self.declare(name, self.span(range), false);
            self.owners = vec![position];
        }
        self.load = saved_load.decorated();
        for decorator in &function.decorator_list {
            self.visit_expr(&decorator.expression);
        }
        self.load = saved_load.value();
        self.parameters(&function.parameters);
        if let Some(returns) = &function.returns {
            self.annotation(returns);
        }
        let binder = Binder::of(&function.body, false);
        for name in binder.written_globals() {
            if self.table.module.contains(&name) {
                self.writers
                    .extend(self.owners.iter().map(|&owner| (name.clone(), owner)));
            }
        }
        let mut locals: HashSet<String> = binder.locals().into_iter().collect();
        locals.extend(
            function
                .parameters
                .iter()
                .map(|parameter| parameter.name().as_str().to_owned()),
        );
        let mut scope = Scope::new(Kind::Function, locals);
        scope.types = self.table.parameter_types(function);
        scope.receiver = class.filter(|_| !is_static(function)).and_then(|class| {
            let first = function
                .parameters
                .posonlyargs
                .iter()
                .chain(&function.parameters.args)
                .next()?;
            Some((first.parameter.name.as_str().to_owned(), class))
        });
        // The receiver is typed so that `buffer = self.field` can take the type of the field. The lookup reads the receiver first, so this changes no chain.
        if let Some((name, class)) = &scope.receiver {
            scope
                .types
                .insert(name.clone(), Ty::Class(Rc::clone(class)));
        }
        scope.mixed = locals::mixed(self.table, function, &scope.types);
        scope.types.retain(|name, _| !scope.mixed.contains(name));
        scope.unsure = function
            .parameters
            .iter()
            .map(|parameter| parameter.name().as_str())
            .filter(|name| !scope.types.contains_key(*name))
            .map(str::to_owned)
            .collect();
        self.scopes.push(scope);
        self.load = Load::Later;
        self.visit_body(&function.body);
        self.scopes.pop();
        self.owners = saved_owners;
        self.load = saved_load;
    }

    fn class(&mut self, class: &'a StmtClassDef) {
        let saved_owners = self.owners.clone();
        let saved_load = self.load;
        let name = self.qualified(class.name.as_str());
        self.rebind(class.name.as_str());
        let declaring = self.declaring();
        if declaring {
            let range = Self::definition(class.range(), &class.decorator_list);
            let position = self.declare(name.clone(), self.span(range), false);
            self.owners = vec![position];
        }
        self.load = saved_load.decorated();
        for decorator in &class.decorator_list {
            self.visit_expr(&decorator.expression);
        }
        self.load = saved_load.value();
        if let Some(arguments) = &class.arguments {
            for base in &arguments.args {
                self.visit_expr(base);
                // A class local to a function goes by a name that another class of the file may have.
                if declaring && let Some(of) = self.table.type_chain(base_of(base)) {
                    let heir = Heir {
                        name: name.clone(),
                        of,
                    };
                    self.heirs.push(heir.clone());
                    self.overrides.push(heir);
                }
            }
            for keyword in &arguments.keywords {
                self.visit_expr(&keyword.value);
            }
        }
        let locals = Binder::of(&class.body, false).names.into_iter().collect();
        let mut scope = Scope::new(Kind::Class, locals);
        scope.class = Some(name.into());
        self.scopes.push(scope);
        self.visit_body(&class.body);
        self.scopes.pop();
        self.owners = saved_owners;
        self.load = saved_load;
    }

    /// The class and the member for `receiver.member`, when `target` assigns an attribute of the instance of a class of the file.
    fn receiver_member<'e>(&self, target: &'e Expr) -> Option<(Rc<str>, &'e str)> {
        let Expr::Attribute(attribute) = target else {
            return None;
        };
        let Expr::Name(root) = &*attribute.value else {
            return None;
        };
        match self.lookup(root.id.as_str()) {
            Binding::Receiver(class) if self.table.has_class(&class) => {
                Some((class, attribute.attr.as_str()))
            }
            _ => None,
        }
    }

    /// The scope a local that code binds belongs to: the nearest function or lambda. A comprehension keeps its loop variables to itself but binds a walrus in the scope around it, and a class body keeps no types.
    fn local_scope(&self) -> Option<&Scope> {
        self.scopes
            .iter()
            .rev()
            .find(|scope| scope.kind != Kind::Comprehension)
            .filter(|scope| scope.kind != Kind::Class)
    }

    fn local_scope_mut(&mut self) -> Option<&mut Scope> {
        self.scopes
            .iter_mut()
            .rev()
            .find(|scope| scope.kind != Kind::Comprehension)
            .filter(|scope| scope.kind != Kind::Class)
    }

    /// Records what a local holds, where it is assigned: a class, a builtin, or `None` for a value the file cannot tell. A local that held something unknown is no builtin afterwards, because the code after a branch may see either, and a local bound to different types in the function holds none.
    fn type_local(&mut self, name: &str, ty: Option<Ty>) {
        let Some(scope) = self.local_scope_mut() else {
            return;
        };
        let ty = ty.filter(|_| !scope.mixed.contains(name));
        match ty {
            Some(Ty::Builtin) if scope.unsure.contains(name) => {
                scope.types.remove(name);
            }
            Some(ty) => {
                scope.types.insert(name.to_owned(), ty);
            }
            None => {
                scope.types.remove(name);
                scope.unsure.insert(name.to_owned());
            }
        }
    }

    /// A local is bound to something the file cannot tell the type of: a loop variable, a name a `with`, an `except` or a `match` captures, a name that is deleted or defined again.
    fn rebind(&mut self, name: &str) {
        self.type_local(name, None);
    }

    /// An assignment: a declaration of each name it binds at the top level or in a class, and a member of the class for each `self.name` in a method.
    fn assignment(
        &mut self,
        range: TextRange,
        targets: &[&'a Expr],
        annotation: Option<&'a Expr>,
        value: Option<&'a Expr>,
    ) {
        let span = self.span(range);
        let saved_owners = self.owners.clone();
        let saved_load = self.load;
        let declaring = self.declaring();
        let mut declared = Vec::new();
        let mut written = Vec::new();
        let mut loose = Vec::new();
        // The first binding of a name in a file with a star import reads the name from there when it reads it at all.
        let unseen = self.star
            && self.scopes.is_empty()
            && matches!(targets, [Expr::Name(name)] if !self.index.contains_key(name.id.as_str()));
        for &target in targets {
            let mut names = Vec::new();
            target_names(target, &mut names);
            if declaring && !names.is_empty() {
                for name in names {
                    let name = self.qualified(name);
                    declared.push(self.declare(name, span, false));
                }
            } else if let Some((class, member)) = self.receiver_member(target) {
                written.push(self.declare(
                    format!("{class}{MEMBER_SEPARATOR}{member}"),
                    span,
                    true,
                ));
            } else {
                loose.push(target);
            }
        }
        let ty = if declaring {
            None
        } else {
            match (annotation, value) {
                (Some(annotation), _) => self.table.annotation_type(annotation),
                (None, Some(value)) => self.value_type(value),
                (None, None) => None,
            }
        };
        if declared.is_empty() && !written.is_empty() {
            let writers: Vec<String> = self
                .owners
                .iter()
                .filter_map(|&owner| self.entries.get(owner))
                .map(|entry| entry.name.clone())
                .collect();
            // The member is written where the assignment starts.
            for &member in &written {
                if let Some(entry) = self.entries.get_mut(member) {
                    for writer in &writers {
                        entry.uses.insert(writer, span.0);
                    }
                }
            }
            self.owners.extend(written);
        } else if !declared.is_empty() {
            if self.scopes.is_empty()
                && let [Expr::Name(name)] = targets
                && let Some(Ty::Class(of)) = match (annotation, value) {
                    (Some(annotation), _) => self.table.annotation_type(annotation),
                    (None, Some(value)) => self.table.value_type(value, &HashMap::new()),
                    (None, None) => None,
                }
            {
                self.heirs.push(Heir {
                    name: name.id.as_str().to_owned(),
                    of: of.as_ref().to_owned(),
                });
            }
            if self.scopes.is_empty()
                && let [Expr::Name(name)] = targets
                && (unseen || self.table.imports(name.id.as_str()))
            {
                self.rebinding.extend(declared.iter().copied());
            }
            self.owners = declared;
        }
        self.load = saved_load.value();
        if let Some(annotation) = annotation {
            self.annotation(annotation);
        }
        if let Some(value) = value {
            self.visit_expr(value);
        }
        self.owners = saved_owners;
        self.load = saved_load;
        // A plain name that is assigned is typed below, and reading it as a target notes nothing.
        for target in loose {
            if !matches!(target, Expr::Name(_)) {
                self.visit_expr(target);
            }
        }
        // The names are bound after the value is read, and after the targets were visited, which rebind them.
        if !declaring {
            for &target in targets {
                if let Expr::Name(name) = target {
                    self.type_local(name.id.as_str(), ty.clone());
                }
            }
        }
    }

    /// Binds `target` to a value of type `ty`: a name takes the type, and any other target is read as it is.
    fn bind_target(&mut self, target: &'a Expr, ty: Option<Ty>) {
        match target {
            Expr::Name(name) => self.type_local(name.id.as_str(), ty),
            other => self.visit_expr(other),
        }
    }

    /// Whether `value` calls a name the function binds itself, a parameter `re` or a local `open`, which is then not the module or the builtin of that name. A local of a known type says what its methods give, and is not this.
    fn calls_a_local(&self, value: &Expr) -> bool {
        let Expr::Call(call) = value else {
            return false;
        };
        let mut root = &*call.func;
        while let Expr::Attribute(attribute) = root {
            root = &attribute.value;
        }
        let Expr::Name(root) = root else {
            return false;
        };
        let name = root.id.as_str();
        self.local_scope()
            .is_some_and(|scope| scope.locals.contains(name) && !scope.types.contains_key(name))
    }

    /// A `with`: the name of an item takes the type that [`Table::with_type`] says, and none otherwise.
    fn with(&mut self, with: &'a StmtWith) {
        for item in &with.items {
            self.visit_expr(&item.context_expr);
            if let Some(target) = &item.optional_vars {
                let ty = self
                    .local_scope()
                    .filter(|_| !with.is_async && !self.calls_a_local(&item.context_expr))
                    .and_then(|scope| self.table.with_type(&item.context_expr, &scope.types));
                self.bind_target(target, ty);
            }
        }
        self.visit_body(&with.body);
    }

    /// A `for`: the variable takes the type that [`Table::iteration_type`] says, and none otherwise.
    fn looped(&mut self, looped: &'a StmtFor) {
        self.visit_expr(&looped.iter);
        let ty = if looped.is_async || self.calls_a_local(&looped.iter) {
            None
        } else {
            self.table.iteration_type(&looped.iter)
        };
        self.bind_target(&looped.target, ty);
        self.visit_body(&looped.body);
        self.visit_body(&looped.orelse);
    }

    /// What the value of an assignment in a function is: an instance of a class, or a builtin.
    fn value_type(&self, value: &Expr) -> Option<Ty> {
        if self.calls_a_local(value) {
            return None;
        }
        let types = &self.local_scope()?.types;
        self.table.value_type(value, types)
    }

    /// Whether `value` is a builtin whatever its members are, as a literal or a call of a builtin type is.
    fn is_builtin_value(&self, value: &Expr) -> bool {
        self.value_type(value) == Some(Ty::Builtin)
    }

    fn annotated(&mut self, stmt: &'a StmtAnnAssign) {
        self.assignment(
            stmt.range(),
            &[&stmt.target],
            Some(&stmt.annotation),
            stmt.value.as_deref(),
        );
    }

    fn augmented(&mut self, stmt: &'a StmtAugAssign) {
        if let Expr::Name(name) = &*stmt.target {
            self.at = u32::from(name.range().start());
            self.chain(name.id.as_str(), &[]);
        } else {
            self.visit_expr(&stmt.target);
        }
        self.visit_expr(&stmt.value);
        if let Expr::Name(name) = &*stmt.target {
            self.rebind(name.id.as_str());
        }
    }

    fn alias(&mut self, stmt: &'a StmtTypeAlias) {
        let saved = self.load;
        self.load = Load::Later;
        let declaring = self.declaring();
        let mut names = Vec::new();
        target_names(&stmt.name, &mut names);
        let saved_owners = self.owners.clone();
        if declaring {
            let span = self.span(stmt.range());
            let declared: Vec<usize> = names
                .into_iter()
                .map(|name| {
                    let name = self.qualified(name);
                    self.declare(name, span, false)
                })
                .collect();
            self.owners = declared;
        }
        self.visit_expr(&stmt.value);
        self.owners = saved_owners;
        self.load = saved;
    }

    /// The block under `if __name__ == "__main__":`, which runs when the file does and not when it is imported. It declares a name of its own, so that what it calls is used by it and a script is not read as loading what it calls.
    fn main_guard(&mut self, stmt: &'a Stmt, branch: &'a StmtIf) {
        let span = self.span(branch.range());
        let position = self.declare(MAIN.to_owned(), span, false);
        let owners = mem::replace(&mut self.owners, vec![position]);
        let load = mem::replace(&mut self.load, Load::Later);
        visitor::walk_stmt(self, stmt);
        self.owners = owners;
        self.load = load;
    }

    fn finish(self) -> Symbols {
        let mut holds: Vec<Heir> = self
            .table
            .holders()
            .map(|(name, of)| Heir {
                name,
                of: of.as_ref().to_owned(),
            })
            .collect();
        let Self {
            mut entries,
            index,
            writers,
            on_load,
            reads,
            mut heirs,
            mut overrides,
            rebinding,
            ..
        } = self;
        for (variable, owner) in writers {
            let writer = entries
                .get(owner)
                .map(|entry| (entry.name.clone(), entry.line));
            let variable = index.get(&variable).and_then(|&at| entries.get_mut(at));
            if let (Some(variable), Some((writer, line))) = (variable, writer) {
                variable.uses.insert_owned(writer, line);
            }
        }
        let mut declared: Vec<Declared> = entries
            .into_iter()
            .enumerate()
            .map(|(at, entry)| Declared {
                // The `X` that `X = wrap(X)` reads is the import it rebinds.
                uses: entry
                    .uses
                    .into_vec()
                    .into_iter()
                    .filter(|used| used.name != entry.name || rebinding.contains(&at))
                    .collect(),
                name: entry.name,
                line: entry.line,
                end: entry.end,
            })
            .collect();
        declared.sort_unstable_by(|a, b| (a.line, &a.name).cmp(&(b.line, &b.name)));
        heirs.sort_unstable_by(|a, b| (&a.name, &a.of).cmp(&(&b.name, &b.of)));
        heirs.dedup();
        overrides.sort_unstable_by(|a, b| (&a.name, &a.of).cmp(&(&b.name, &b.of)));
        overrides.dedup();
        holds.sort_unstable_by(|a, b| (&a.name, &a.of).cmp(&(&b.name, &b.of)));
        holds.dedup();
        Symbols {
            declared,
            on_load: on_load.into_vec(),
            exports: Vec::new(),
            heirs,
            overrides,
            holds,
            reads: reads.into_vec(),
        }
    }
}

impl<'a> Visitor<'a> for Recorder<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(function) => self.function(function),
            Stmt::ClassDef(class) => self.class(class),
            Stmt::Assign(assign) => {
                let targets: Vec<&Expr> = assign.targets.iter().collect();
                self.assignment(assign.range(), &targets, None, Some(&assign.value));
            }
            Stmt::AnnAssign(assign) => self.annotated(assign),
            Stmt::AugAssign(assign) => self.augmented(assign),
            Stmt::TypeAlias(alias) => self.alias(alias),
            Stmt::With(with) => self.with(with),
            Stmt::For(looped) => self.looped(looped),
            Stmt::If(branch) if self.scopes.is_empty() && is_main_guard(&branch.test) => {
                self.main_guard(stmt, branch);
            }
            Stmt::Import(_) | Stmt::ImportFrom(_) | Stmt::Global(_) | Stmt::Nonlocal(_) => {}
            other => visitor::walk_stmt(self, other),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) => match name.ctx {
                ExprContext::Load => {
                    self.at = u32::from(name.range().start());
                    self.chain(name.id.as_str(), &[]);
                }
                // A comprehension binds its own variables, which the scope around never sees.
                ExprContext::Store | ExprContext::Del
                    if self
                        .scopes
                        .last()
                        .is_none_or(|scope| scope.kind != Kind::Comprehension) =>
                {
                    self.rebind(name.id.as_str());
                }
                _ => {}
            },
            Expr::Named(named) => {
                self.visit_expr(&named.value);
                if let Expr::Name(target) = &*named.target {
                    let ty = self.value_type(&named.value);
                    self.type_local(target.id.as_str(), ty);
                }
            }
            Expr::Subscript(subscript) if self.annotation => self.annotation_subscript(subscript),
            Expr::Attribute(_) => self.attribute(expr),
            Expr::Call(call) => self.call(call),
            Expr::Lambda(lambda) => self.lambda(lambda),
            Expr::ListComp(comp) => self.comprehension(&comp.generators, &[&comp.elt]),
            Expr::SetComp(comp) => self.comprehension(&comp.generators, &[&comp.elt]),
            Expr::Generator(comp) => self.comprehension(&comp.generators, &[&comp.elt]),
            Expr::DictComp(comp) => {
                let elements: Vec<&Expr> = comp
                    .key
                    .as_deref()
                    .into_iter()
                    .chain([&*comp.value])
                    .collect();
                self.comprehension(&comp.generators, &elements);
            }
            Expr::StringLiteral(text) if self.annotation => {
                self.at = u32::from(text.range().start());
                self.string_annotation(text.value.to_str());
            }
            other => visitor::walk_expr(self, other),
        }
    }

    fn visit_except_handler(&mut self, handler: &'a ExceptHandler) {
        let ExceptHandler::ExceptHandler(inner) = handler;
        if let Some(name) = &inner.name {
            self.rebind(name.as_str());
        }
        visitor::walk_except_handler(self, handler);
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        if let Some(name) = captured(pattern) {
            self.rebind(name);
        }
        visitor::walk_pattern(self, pattern);
    }
}

/// Whether `name` is there without being bound: a builtin or one of the names a module starts with.
fn is_ambient(name: &str) -> bool {
    is_python_builtin(name, AMBIENT_MINOR, false)
        || python_magic_globals(AMBIENT_MINOR).any(|magic| magic == name)
}

/// Whether the call is `super(...)`.
fn is_super(call: &ExprCall) -> bool {
    matches!(&*call.func, Expr::Name(name) if name.id.as_str() == "super")
}

/// Whether `test` is `__name__ == "__main__"`, in either order.
fn is_main_guard(test: &Expr) -> bool {
    let Expr::Compare(compare) = test else {
        return false;
    };
    let Some((left, CmpOp::Eq, right)) = compare.as_single() else {
        return false;
    };
    let is_name = |expr: &Expr| matches!(expr, Expr::Name(name) if name.id.as_str() == "__name__");
    let is_main =
        |expr: &Expr| matches!(expr, Expr::StringLiteral(text) if text.value.to_str() == MAIN);
    (is_name(left) && is_main(right)) || (is_main(left) && is_name(right))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use proptest::prelude::*;
    use ruff_python_ast::PySourceType;
    use ruff_python_parser::parse_unchecked_source;

    use super::*;

    /// Whole statements of the shapes a program is made of, so that a tree of them always parses and the recorder meets every construct it reads.
    const SNIPPETS: &[&str] = &[
        "import os",
        "import a.b as c",
        "import a.b",
        "from m import x as y, z",
        "from . import sub",
        "from .pkg import K as K",
        "X = 1",
        "Y: int = f(X)",
        "(P, Q) = 1, 2",
        "def f(a, b=X):\n    return g(a).h + os.getcwd()",
        "class K(Base):\n    v = 1\n    def __init__(self):\n        self.v = f()\n    def m(self, p: K):\n        super().m()\n        return self.v + p.q",
        "class L(K, Mixin):\n    pass",
        "@deco.reg\ndef g():\n    k = K()\n    k.m(1)\n    return k",
        "type T = list[int]",
        "if __name__ == '__main__':\n    f(1)",
        "for i in range(3):\n    run(i)",
        "v = [run(i) for i in xs]",
        "with open(p) as fh:\n    fh.read()",
        "try:\n    import x\nexcept ImportError:\n    x = None",
        "__all__ = ['f', 'K']",
        "async def a():\n    await f()",
        "h = lambda q: q.z",
        "app = FastAPI()",
        "def f(X):\n    X = 2\n    return X",
        "from m import X\nX = wrap(X)",
    ];

    /// The recorded `symbols` and `exports` of `text`, or `None` for a text with a syntax error or one the recorder gives up on.
    fn recorded(text: &str, star: bool) -> Option<Read> {
        let parsed = parse_unchecked_source(text, PySourceType::Python);
        if !parsed.errors().is_empty() {
            return None;
        }
        read(parsed.suite(), &LineIndex::new(text), star)
    }

    fn strictly_increasing<T: Ord>(items: &[T]) -> bool {
        items.windows(2).all(|pair| pair[0] < pair[1])
    }

    fn names_of(uses: &[gsttt_core::fact::Use]) -> Vec<&str> {
        uses.iter().map(|used| used.name.as_str()).collect()
    }

    /// What the contract of `Symbols` promises: ordered, once each, within the file, never itself.
    fn check(text: &str, read: &Read, star: bool) {
        let lines = u32::try_from(text.split('\n').count()).unwrap_or(u32::MAX);
        let symbols = &read.symbols;
        let parsed = parse_unchecked_source(text, PySourceType::Python);
        let table = Table::of(parsed.suite());
        let order: Vec<(u32, &str)> = symbols
            .declared
            .iter()
            .map(|declared| (declared.line, declared.name.as_str()))
            .collect();
        assert!(strictly_increasing(&order), "{order:?}");
        let names: BTreeSet<&str> = order.iter().map(|&(_, name)| name).collect();
        assert_eq!(
            names.len(),
            order.len(),
            "a name is declared twice: {order:?}"
        );
        for declared in &symbols.declared {
            assert!(
                1 <= declared.line && declared.line <= declared.end && declared.end <= lines,
                "{declared:?} in {lines} lines"
            );
            let used = names_of(&declared.uses);
            assert!(strictly_increasing(&used), "{declared:?}");
            assert!(
                !used.contains(&declared.name.as_str()) || star || table.imports(&declared.name),
                "{declared:?}"
            );
        }
        // A use is never given a line the file does not have.
        for used in symbols
            .declared
            .iter()
            .flat_map(|declared| &declared.uses)
            .chain(&symbols.on_load)
            .chain(&symbols.reads)
        {
            assert!(
                1 <= used.line && used.line <= lines,
                "{used:?} in {lines} lines"
            );
        }
        let on_load = names_of(&symbols.on_load);
        assert!(strictly_increasing(&on_load), "{on_load:?}");
        let reads = names_of(&symbols.reads);
        assert!(strictly_increasing(&reads), "{reads:?}");
        for pairs in [&symbols.heirs, &symbols.overrides, &symbols.holds] {
            let pairs: Vec<(&str, &str)> = pairs
                .iter()
                .map(|heir| (heir.name.as_str(), heir.of.as_str()))
                .collect();
            assert!(strictly_increasing(&pairs), "{pairs:?}");
        }
        assert!(
            symbols.exports.is_empty(),
            "Python hands names out in `exports` of the file"
        );
        assert!(strictly_increasing(&read.exports), "{:?}", read.exports);
    }

    proptest! {
        #[test]
        fn a_program_of_known_statements_is_recorded_in_order_and_the_same_each_time(
            parts in prop::collection::vec(prop::sample::select(SNIPPETS), 0..8),
            star in any::<bool>(),
        ) {
            let text = parts.join("\n");
            if let Some(first) = recorded(&text, star) {
                check(&text, &first, star);
                let again = recorded(&text, star).expect("a second reading gives the same answer");
                prop_assert_eq!(first.symbols, again.symbols);
                prop_assert_eq!(first.exports, again.exports);
            }
        }

        #[test]
        fn a_program_with_a_piece_cut_out_is_recorded_without_a_panic(
            parts in prop::collection::vec(prop::sample::select(SNIPPETS), 1..6),
            start in 0_usize..400,
            len in 0_usize..6,
        ) {
            // Every snippet is ASCII, so any byte offset is a boundary.
            let mut text = parts.join("\n");
            let start = start.min(text.len());
            let end = (start + len).min(text.len());
            text.replace_range(start..end, "");
            if let Some(read) = recorded(&text, false) {
                check(&text, &read, false);
            }
        }

        #[test]
        fn a_text_that_parses_is_recorded_without_a_panic(
            text in "[ -~\n\t]{0,160}",
            star in any::<bool>(),
        ) {
            if let Some(read) = recorded(&text, star) {
                check(&text, &read, star);
            }
        }
    }
}
