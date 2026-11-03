//! What a file binds and what its classes hold, read before any body is: the recorder needs to know, at a use, whether a name is the file's own and which members a class has.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::mem;
use std::rc::Rc;

use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    Expr, ExprContext, ExprSubscript, Operator, Pattern, Stmt, StmtClassDef, StmtFunctionDef,
    StmtImport, StmtImportFrom,
};

use super::is_ambient;
use crate::parse::STAR;
use crate::site::dotted;

/// The longest chain a use is recorded with. A longer one reads the attributes of a value, not a declaration.
pub(super) const MAX_CHAIN: usize = 6;

/// How many bases a member is looked up through.
const MAX_DEPTH: usize = 8;

/// The modules whose names stand for builtin collections or for a loose annotation: what `from typing import List` and `import collections` bring in. None of their names is a class of the file.
const LIBRARY_MODULES: &[&str] = &[
    "typing",
    "typing_extensions",
    "collections",
    "collections.abc",
    "types",
];

/// The types an annotation names a builtin value with, `list[int]` and `Mapping[str, int]` among them.
const BUILTIN_TYPES: &[&str] = &[
    "list",
    "dict",
    "set",
    "frozenset",
    "tuple",
    "str",
    "bytes",
    "int",
    "float",
    "bool",
    "List",
    "Dict",
    "Set",
    "FrozenSet",
    "Tuple",
    "Sequence",
    "Mapping",
    "MutableMapping",
    "Iterable",
    "Iterator",
    "DefaultDict",
    "Deque",
    "Counter",
    "OrderedDict",
];

/// The callables whose result is a builtin value.
const BUILTIN_CALLS: &[&str] = &[
    "list",
    "dict",
    "set",
    "tuple",
    "frozenset",
    "str",
    "bytes",
    "sorted",
    "defaultdict",
    "OrderedDict",
    "deque",
    "Counter",
];

/// What a local or a field is known to hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Ty {
    /// An instance of a class the file binds, as the chain it is written with.
    Class(Rc<str>),
    /// A builtin value: a list, a mapping, a string, a number. None of its members is a declaration of the file, so a member read on it reads no declaration.
    Builtin,
}

impl Ty {
    fn class(chain: String) -> Self {
        Self::Class(chain.into())
    }
}

/// The type that a value of one of several types has: the one they all have. Nothing is known of a value that may be of another class, or of a type the file cannot tell, because a read on it may land on either.
fn either(types: impl IntoIterator<Item = Option<Ty>>) -> Option<Ty> {
    let mut common: Option<Ty> = None;
    for ty in types {
        let ty = ty?;
        if common.as_ref().is_some_and(|seen| *seen != ty) {
            return None;
        }
        common = Some(ty);
    }
    common
}

/// What the file binds at its top level, and what each of its classes holds.
#[derive(Default)]
pub(super) struct Table {
    /// Every name bound at the top level of the file, an import's among them: what the file exports.
    pub(super) module: BTreeSet<String>,
    /// Names an import binds inside a function or a class, which the rest of the file reads as it reads those of the top level.
    imported: HashSet<String>,
    /// Names bound by an import of one of [`LIBRARY_MODULES`]: `List` and `t` for `import typing as t`.
    library: HashSet<String>,
    /// Classes by the name they are declared under: `Outer.Inner` for a class nested in a class.
    classes: HashMap<String, ClassInfo>,
}

/// One class: the names it declares, the types of its fields and the bases that name something the file binds.
#[derive(Default)]
struct ClassInfo {
    members: HashSet<String>,
    /// Every field a class body or a method assigns.
    fields: HashMap<String, Field>,
    bases: Vec<String>,
}

/// What the assignments and the annotations of one field give.
#[derive(Default)]
struct Field {
    /// What the first annotation that gave a type says the field holds, which the values it is given do not change.
    annotated: Option<Ty>,
    /// What every assignment gave, as long as they agree.
    held: Held,
}

/// What the assignments of a field give together.
#[derive(Default)]
enum Held {
    #[default]
    Nothing,
    /// Every assignment gave this type.
    Just(Ty),
    /// Two assignments gave different types, or one gave a value the file cannot tell, so that a read may land on any of them.
    Mixed,
}

impl Field {
    /// Notes the assignment of `value`, which has the type `ty`. `None` leaves no mark: a field is set to it before it is given its value.
    fn assigned(&mut self, value: &Expr, ty: Option<Ty>) {
        if !matches!(value, Expr::NoneLiteral(_)) {
            self.give(ty);
        }
    }

    fn annotated(&mut self, ty: Option<Ty>) {
        match ty {
            Some(ty) => {
                self.annotated.get_or_insert(ty);
            }
            None => self.give(None),
        }
    }

    fn give(&mut self, ty: Option<Ty>) {
        self.held = match (mem::take(&mut self.held), ty) {
            (Held::Nothing, Some(ty)) => Held::Just(ty),
            (Held::Just(seen), Some(ty)) if seen == ty => Held::Just(seen),
            _ => Held::Mixed,
        };
    }

    /// What an annotation says the field holds, else the type every assignment gave.
    fn ty(&self) -> Option<Ty> {
        self.annotated.clone().or_else(|| match &self.held {
            Held::Just(ty) => Some(ty.clone()),
            Held::Nothing | Held::Mixed => None,
        })
    }
}

impl Table {
    pub(super) fn of(suite: &[Stmt]) -> Self {
        let mut table = Self {
            module: Binder::of(suite, true).names,
            ..Self::default()
        };
        let mut scan = ImportScan {
            imported: &mut table.imported,
            library: &mut table.library,
        };
        scan.visit_body(suite);
        table.read_classes(suite);
        table
    }

    /// Whether `name` is something the file binds outside any function, or an import anywhere.
    pub(super) fn knows(&self, name: &str) -> bool {
        self.module.contains(name) || self.imported.contains(name)
    }

    /// Whether an import of the file binds `name`.
    pub(super) fn imports(&self, name: &str) -> bool {
        self.imported.contains(name)
    }

    /// Whether `name` is a class of this file.
    pub(super) fn has_class(&self, name: &str) -> bool {
        self.classes.contains_key(name)
    }

    /// The type of the field `member` of `class`, looked up in its bases too.
    pub(super) fn field_type(&self, class: &str, member: &str) -> Option<Ty> {
        self.field_in(class, member, false, 0)
    }

    /// `unknown` says that a class above assigns the field a value of no known type, so that a builtin a base gives it is no longer the field's.
    fn field_in(&self, class: &str, member: &str, unknown: bool, depth: usize) -> Option<Ty> {
        let info = self.classes.get(class)?;
        let mut unknown = unknown;
        if let Some(field) = info.fields.get(member) {
            match field.ty() {
                Some(Ty::Builtin) if unknown => return None,
                Some(ty) => return Some(ty),
                None => unknown = true,
            }
        }
        if depth >= MAX_DEPTH {
            return None;
        }
        info.bases
            .iter()
            .find_map(|base| self.field_in(base, member, unknown, depth + 1))
    }

    /// Hands `found` the class and the member of what a use of `member` through an instance of `class` stands for: the declaration of that class or of the local base that has it, and the member of each base the file does not declare. Tells whether a class of the file declares it, so that a base the file does not declare leaves the read to be made by name too.
    pub(super) fn member_uses(
        &self,
        class: &str,
        member: &str,
        found: &mut impl FnMut(&str, &str),
    ) -> bool {
        self.collect_member(class, member, 0, found)
    }

    fn collect_member(
        &self,
        class: &str,
        member: &str,
        depth: usize,
        found: &mut impl FnMut(&str, &str),
    ) -> bool {
        let Some(info) = self.classes.get(class) else {
            return false;
        };
        if info.members.contains(member) {
            found(class, member);
            return true;
        }
        if depth >= MAX_DEPTH {
            return false;
        }
        let mut any = false;
        for base in &info.bases {
            if self.classes.contains_key(base) {
                any |= self.collect_member(base, member, depth + 1, found);
            } else {
                // The base the file does not declare is used through the member, and that is no proof that no other class has it: the read stays.
                found(base, member);
            }
        }
        any
    }

    /// The bases of `class` that name something the file binds.
    pub(super) fn bases(&self, class: &str) -> &[String] {
        self.classes
            .get(class)
            .map_or(&[], |info| info.bases.as_slice())
    }

    /// Records the classes of `suite`, which are those of the top level, of its compound statements and of the bodies of classes, but never of a function. Every name is entered before any class is read, so that an annotation may name a class defined further down.
    fn read_classes(&mut self, suite: &[Stmt]) {
        let mut defined = Vec::new();
        class_defs(suite, &mut Vec::new(), &mut defined);
        for (name, _) in &defined {
            self.classes.insert(name.clone(), ClassInfo::default());
        }
        for (name, class) in defined {
            let info = self.class_info(class);
            self.classes.insert(name, info);
        }
    }

    fn class_info(&self, class: &StmtClassDef) -> ClassInfo {
        let mut info = ClassInfo::default();
        if let Some(arguments) = class.arguments.as_deref() {
            info.bases = arguments
                .args
                .iter()
                .filter_map(|base| self.type_chain(base_of(base)))
                .collect();
        }
        let mut methods = Vec::new();
        self.read_body(&class.body, &mut info, &mut methods);
        for method in methods {
            self.read_method(method, &mut info);
        }
        info
    }

    fn read_body<'a>(
        &self,
        body: &'a [Stmt],
        info: &mut ClassInfo,
        methods: &mut Vec<&'a StmtFunctionDef>,
    ) {
        for stmt in body {
            match stmt {
                Stmt::FunctionDef(function) => {
                    info.members.insert(function.name.as_str().to_owned());
                    methods.push(function);
                }
                Stmt::ClassDef(inner) => {
                    info.members.insert(inner.name.as_str().to_owned());
                }
                Stmt::Assign(assign) => {
                    let ty = self.value_type(&assign.value, &HashMap::new());
                    for target in &assign.targets {
                        Self::field(target, info, |field| {
                            field.assigned(&assign.value, ty.clone());
                        });
                    }
                }
                Stmt::AnnAssign(assign) => {
                    let ty = self.annotation_type(&assign.annotation);
                    Self::field(&assign.target, info, |field| field.annotated(ty.clone()));
                }
                other => {
                    for block in blocks(other) {
                        self.read_body(block, info, methods);
                    }
                }
            }
        }
    }

    /// A name a class body assigns is a member and a field, which `note` tells what the statement gives.
    fn field(target: &Expr, info: &mut ClassInfo, note: impl Fn(&mut Field)) {
        let mut names = Vec::new();
        target_names(target, &mut names);
        for name in names {
            note(info.fields.entry(name.to_owned()).or_default());
            info.members.insert(name.to_owned());
        }
    }

    /// The `self.name = ...` and `self.name: T` statements of a method make members, as the first parameter names the instance.
    fn read_method(&self, method: &StmtFunctionDef, info: &mut ClassInfo) {
        if is_static(method) {
            return;
        }
        let Some(first) = method
            .parameters
            .posonlyargs
            .iter()
            .chain(&method.parameters.args)
            .next()
        else {
            return;
        };
        let receiver = first.parameter.name.as_str();
        let typed = self.parameter_types(method);
        let mut scan = SelfScan {
            table: self,
            receiver,
            typed: &typed,
            info,
        };
        scan.visit_body(&method.body);
    }

    /// The types the annotated parameters of `function` have.
    pub(super) fn parameter_types(&self, function: &StmtFunctionDef) -> HashMap<String, Ty> {
        let parameters = &function.parameters;
        parameters
            .posonlyargs
            .iter()
            .chain(&parameters.args)
            .chain(&parameters.kwonlyargs)
            .filter_map(|with_default| {
                let parameter = &with_default.parameter;
                let ty = self.annotation_type(parameter.annotation.as_deref()?)?;
                Some((parameter.name.as_str().to_owned(), ty))
            })
            .collect()
    }

    /// The chain `expr` is, when it starts at a name the file binds.
    pub(super) fn type_chain(&self, expr: &Expr) -> Option<String> {
        self.known_chain(dotted(expr)?)
    }

    fn known_chain(&self, chain: String) -> Option<String> {
        let root = chain.split('.').next()?;
        (self.knows(root) && !chain.contains('?')).then_some(chain)
    }

    /// Whether `chain` is one of `names` as the standard library gives it, and not as a class or an import of the file does: a builtin the file does not rebind, or a name an import of `typing` or `collections` brings in.
    pub(super) fn is_library_name(&self, chain: &str, names: &[&str]) -> bool {
        match chain.rsplit_once('.') {
            Some((module, name)) => {
                names.contains(&name)
                    && module
                        .split('.')
                        .next()
                        .is_some_and(|root| self.library.contains(root))
            }
            None => {
                names.contains(&chain)
                    && (self.library.contains(chain) || (is_ambient(chain) && !self.knows(chain)))
            }
        }
    }

    /// What an annotation names: a builtin such as `list[int]`, or a chain the file binds, through `Optional`, `Union`, `Annotated`, `ClassVar`, `Final`, `Type` and `| None`. Of an `Annotated` only the first argument is a type, and a `Literal` holds values. Another generic is no class of the file.
    pub(super) fn annotation_type(&self, annotation: &Expr) -> Option<Ty> {
        match annotation {
            Expr::Name(_) | Expr::Attribute(_) => self.chain_type(dotted(annotation)?),
            Expr::StringLiteral(text) => {
                self.chain_type(text_chain(text.value.to_str())?.to_owned())
            }
            Expr::BinOp(binary) if binary.op == Operator::BitOr => {
                self.either_type([&*binary.left, &*binary.right])
            }
            Expr::Subscript(subscript) => self.subscript_type(subscript),
            _ => None,
        }
    }

    fn chain_type(&self, chain: String) -> Option<Ty> {
        if self.is_library_name(&chain, BUILTIN_TYPES) {
            return Some(Ty::Builtin);
        }
        self.class_chain(chain).map(Ty::class)
    }

    /// The chain when it names a class: one the file declares, or one an import brings in that reads as a class. A name bound by an assignment, such as a `TypeVar` or an alias of a union, and a name of the standard library's typing modules, such as `Any`, hold no class.
    fn class_chain(&self, chain: String) -> Option<String> {
        let root = chain.split('.').next()?;
        let class = self.classes.contains_key(&chain)
            || (self.imported.contains(root)
                && !self.library.contains(root)
                && is_class_name(&chain));
        (class && !chain.contains('?')).then_some(chain)
    }

    /// The type of a value that is one of `members` of a union, `None` leaving no mark.
    fn either_type<'e>(&self, members: impl IntoIterator<Item = &'e Expr>) -> Option<Ty> {
        either(
            members
                .into_iter()
                .filter(|member| !matches!(member, Expr::NoneLiteral(_)))
                .map(|member| self.annotation_type(member)),
        )
    }

    fn subscript_type(&self, subscript: &ExprSubscript) -> Option<Ty> {
        let wrapper = dotted(&subscript.value)?;
        let name = wrapper.rsplit('.').next().unwrap_or(&wrapper);
        let first = match &*subscript.slice {
            Expr::Tuple(tuple) => tuple.elts.first()?,
            other => other,
        };
        match name {
            "Union" => match &*subscript.slice {
                Expr::Tuple(tuple) => self.either_type(&tuple.elts),
                other => self.annotation_type(other),
            },
            "Optional" | "Annotated" | "ClassVar" | "Final" | "Type" | "type" => {
                self.annotation_type(first)
            }
            _ => self
                .is_library_name(&wrapper, BUILTIN_TYPES)
                .then_some(Ty::Builtin),
        }
    }

    /// The type of a value that is one of `values`, `None` leaving no mark.
    fn either_value<'e>(
        &self,
        values: impl IntoIterator<Item = &'e Expr>,
        parameters: &HashMap<String, Ty>,
    ) -> Option<Ty> {
        either(
            values
                .into_iter()
                .filter(|value| !matches!(value, Expr::NoneLiteral(_)))
                .map(|value| self.value_type(value, parameters)),
        )
    }

    /// What a value is when the file can tell: an instance of a class it binds, made by calling one, `Foo(...)`, or a parameter whose type is known, and a builtin made by a literal, a comprehension or a call of a builtin type. A choice between values, written with `or` or `if`, has the type they all have, `None` aside.
    pub(super) fn value_type(&self, value: &Expr, parameters: &HashMap<String, Ty>) -> Option<Ty> {
        match value {
            Expr::Call(call) => {
                let chain = dotted(&call.func)?;
                if self.is_library_name(&chain, BUILTIN_CALLS) {
                    return Some(Ty::Builtin);
                }
                let chain = self.known_chain(chain)?;
                is_class_name(&chain).then(|| Ty::class(chain))
            }
            Expr::Name(name) => parameters.get(name.id.as_str()).cloned(),
            Expr::BoolOp(operation) => self.either_value(&operation.values, parameters),
            Expr::If(branch) => self.either_value([&*branch.body, &*branch.orelse], parameters),
            Expr::List(_)
            | Expr::Dict(_)
            | Expr::Set(_)
            | Expr::Tuple(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::Generator(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::FString(_)
            | Expr::NumberLiteral(_)
            | Expr::BooleanLiteral(_) => Some(Ty::Builtin),
            _ => None,
        }
    }
}

/// The text of a string annotation, when it is one chain of names.
fn text_chain(text: &str) -> Option<&str> {
    let text = text.trim();
    text.split('.').all(is_identifier).then_some(text)
}

/// Whether a chain reads as a class: its last segment starts with a capital letter.
pub(super) fn is_class_name(chain: &str) -> bool {
    chain
        .rsplit('.')
        .next()
        .and_then(|last| last.chars().next())
        .is_some_and(char::is_uppercase)
}

/// Whether `text` is one identifier.
pub(super) fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_alphabetic())
        && chars.all(|c| c == '_' || c.is_alphanumeric())
}

/// A base written `Generic[T]` or `Base[int]` names `Generic` and `Base`.
pub(super) fn base_of(base: &Expr) -> &Expr {
    match base {
        Expr::Subscript(subscript) => &subscript.value,
        other => other,
    }
}

/// Whether the function is a `@staticmethod`, which has no instance to read members through.
pub(super) fn is_static(function: &StmtFunctionDef) -> bool {
    function
        .decorator_list
        .iter()
        .any(|decorator| dotted(&decorator.expression).is_some_and(|name| name == "staticmethod"))
}

/// Collects the classes of `suite` with the name each goes by, `Outer.Inner` for one nested in a class: those of the top level, of its compound statements and of class bodies, but never of a function.
fn class_defs<'a>(
    suite: &'a [Stmt],
    path: &mut Vec<String>,
    out: &mut Vec<(String, &'a StmtClassDef)>,
) {
    for stmt in suite {
        if let Stmt::ClassDef(class) = stmt {
            path.push(class.name.as_str().to_owned());
            out.push((path.join("."), class));
            class_defs(&class.body, path, out);
            path.pop();
        } else {
            for block in blocks(stmt) {
                class_defs(block, path, out);
            }
        }
    }
}

/// The statements nested directly in a compound statement. A function or a class body is a scope of its own and is not among them.
pub(super) fn blocks(stmt: &Stmt) -> Vec<&[Stmt]> {
    match stmt {
        Stmt::If(branch) => {
            let mut blocks = vec![branch.body.as_slice()];
            blocks.extend(branch.elif_else_clauses.iter().map(|c| c.body.as_slice()));
            blocks
        }
        Stmt::For(looped) => vec![looped.body.as_slice(), looped.orelse.as_slice()],
        Stmt::While(looped) => vec![looped.body.as_slice(), looped.orelse.as_slice()],
        Stmt::With(with) => vec![with.body.as_slice()],
        Stmt::Try(attempt) => {
            let mut blocks = vec![attempt.body.as_slice()];
            for ruff_python_ast::ExceptHandler::ExceptHandler(handler) in &attempt.handlers {
                blocks.push(handler.body.as_slice());
            }
            blocks.push(attempt.orelse.as_slice());
            blocks.push(attempt.finalbody.as_slice());
            blocks
        }
        Stmt::Match(matched) => matched
            .cases
            .iter()
            .map(|case| case.body.as_slice())
            .collect(),
        _ => Vec::new(),
    }
}

/// The names an assignment target binds: a name, or the names inside a tuple, a list or a starred target. An attribute or a subscript binds none.
pub(super) fn target_names<'a>(target: &'a Expr, out: &mut Vec<&'a str>) {
    match target {
        Expr::Name(name) => out.push(name.id.as_str()),
        Expr::Tuple(tuple) => tuple.elts.iter().for_each(|item| target_names(item, out)),
        Expr::List(list) => list.elts.iter().for_each(|item| target_names(item, out)),
        Expr::Starred(starred) => target_names(&starred.value, out),
        _ => {}
    }
}

/// The names an importer of the file can take by name: every name the file binds at its top level, the names it imports among them, because `from a import x` takes an `x` that `a` imports as it takes one that `a` defines. The names a star import brings in are not listed, because the walk follows the star lines to the files they lead to; the strings of `__all__` are, because they name what the file hands out, whatever binds it.
pub(super) fn exports(suite: &[Stmt]) -> Vec<String> {
    let mut names = Binder::of(suite, true).names;
    hand_out(suite, &mut names);
    names.into_iter().collect()
}

/// Adds the names a statement of `suite`, or of a block inside it, hands out beyond those it binds: the strings `__all__` is given, by an assignment, a `+=`, an `extend` or an `append`.
fn hand_out(suite: &[Stmt], names: &mut BTreeSet<String>) {
    let is_all =
        |target: &Expr| matches!(target, Expr::Name(name) if name.id.as_str() == "__all__");
    for stmt in suite {
        match stmt {
            Stmt::Assign(assign) if assign.targets.iter().any(is_all) => {
                listed(&assign.value, names);
            }
            Stmt::AnnAssign(assign) if is_all(&assign.target) => {
                if let Some(value) = &assign.value {
                    listed(value, names);
                }
            }
            Stmt::AugAssign(assign) if is_all(&assign.target) => listed(&assign.value, names),
            Stmt::Expr(statement) => {
                if let Expr::Call(call) = &*statement.value
                    && let Expr::Attribute(method) = &*call.func
                    && is_all(&method.value)
                    && let [argument] = &*call.arguments.args
                {
                    match method.attr.as_str() {
                        "extend" => listed(argument, names),
                        "append" => listed_name(argument, names),
                        _ => {}
                    }
                }
            }
            other => {
                for block in blocks(other) {
                    hand_out(block, names);
                }
            }
        }
    }
}

/// The strings of a list, tuple or set, which is what `__all__` holds, and of the lists and tuples added together to make it.
fn listed(value: &Expr, names: &mut BTreeSet<String>) {
    let elements = match value {
        Expr::List(list) => &list.elts,
        Expr::Tuple(tuple) => &tuple.elts,
        Expr::Set(set) => &set.elts,
        Expr::BinOp(sum) if sum.op == Operator::Add => {
            listed(&sum.left, names);
            listed(&sum.right, names);
            return;
        }
        _ => return,
    };
    for element in elements {
        listed_name(element, names);
    }
}

/// The name a string is, which is what an element of `__all__` is.
fn listed_name(value: &Expr, names: &mut BTreeSet<String>) {
    if let Expr::StringLiteral(text) = value {
        names.insert(text.value.to_str().to_owned());
    }
}

/// The name an `import` statement binds: `import a.b` binds `a`, and `import a.b as c` binds `c`.
fn import_names(stmt: &StmtImport, out: &mut impl FnMut(&str)) {
    for alias in &stmt.names {
        match &alias.asname {
            Some(local) => out(local.as_str()),
            None => out(alias.name.as_str().split('.').next().unwrap_or_default()),
        }
    }
}

/// The names a `from` import binds.
fn import_from_names(stmt: &StmtImportFrom, out: &mut impl FnMut(&str)) {
    for alias in stmt
        .names
        .iter()
        .filter(|alias| alias.name.as_str() != STAR)
    {
        out(alias
            .asname
            .as_ref()
            .map_or(alias.name.as_str(), |local| local.as_str()));
    }
}

/// The names a scope binds, without those of the scopes inside it.
#[derive(Default)]
pub(super) struct Binder {
    pub(super) names: BTreeSet<String>,
    /// What `global` declares in the scope, which is bound in the module and not here.
    pub(super) globals: BTreeSet<String>,
    /// What `nonlocal` declares, which is bound by the scope around.
    nonlocals: BTreeSet<String>,
    imports: bool,
    /// How many comprehensions deep the walk is: the names they loop over are theirs, but a walrus binds in the scope around.
    comprehensions: usize,
}

impl Binder {
    /// The names bound by `body`. `imports` says whether an import binds a name: it does in the table of the module and in what a package's `__init__` hands out, and does not in a function or a class body, because the file reads an import there as it reads one at the top.
    pub(super) fn of(body: &[Stmt], imports: bool) -> Self {
        let mut binder = Self {
            imports,
            ..Self::default()
        };
        binder.visit_body(body);
        binder
    }

    /// The names bound in the scope itself: not those declared `global` or `nonlocal`.
    pub(super) fn locals(&self) -> BTreeSet<String> {
        self.names
            .iter()
            .filter(|name| !self.globals.contains(*name) && !self.nonlocals.contains(*name))
            .cloned()
            .collect()
    }

    /// The names declared `global` that the scope also assigns.
    pub(super) fn written_globals(&self) -> BTreeSet<String> {
        self.names
            .iter()
            .filter(|name| self.globals.contains(*name))
            .cloned()
            .collect()
    }

    fn bind(&mut self, name: &str) {
        self.names.insert(name.to_owned());
    }
}

impl<'a> Visitor<'a> for Binder {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(function) => self.bind(function.name.as_str()),
            Stmt::ClassDef(class) => self.bind(class.name.as_str()),
            Stmt::Import(import) if self.imports => {
                import_names(import, &mut |name| self.bind(name));
            }
            Stmt::ImportFrom(import) if self.imports => {
                import_from_names(import, &mut |name| self.bind(name));
            }
            Stmt::Global(global) => self
                .globals
                .extend(global.names.iter().map(|name| name.as_str().to_owned())),
            Stmt::Nonlocal(nonlocal) => self
                .nonlocals
                .extend(nonlocal.names.iter().map(|name| name.as_str().to_owned())),
            other => visitor::walk_stmt(self, other),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) if name.ctx == ExprContext::Store && self.comprehensions == 0 => {
                self.bind(name.id.as_str());
            }
            Expr::Named(named) => {
                self.visit_expr(&named.value);
                if let Expr::Name(target) = &*named.target {
                    self.bind(target.id.as_str());
                }
            }
            // A lambda is a scope of its own, and so is a comprehension, except for the walrus in it.
            Expr::Lambda(_) => {}
            Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::Generator(_) => {
                self.comprehensions += 1;
                visitor::walk_expr(self, expr);
                self.comprehensions -= 1;
            }
            other => visitor::walk_expr(self, other),
        }
    }

    fn visit_except_handler(&mut self, handler: &'a ruff_python_ast::ExceptHandler) {
        let ruff_python_ast::ExceptHandler::ExceptHandler(inner) = handler;
        if let Some(name) = &inner.name {
            self.bind(name.as_str());
        }
        visitor::walk_except_handler(self, handler);
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        if let Some(name) = captured(pattern) {
            self.bind(name);
        }
        visitor::walk_pattern(self, pattern);
    }
}

/// The name a pattern binds by itself, not counting the patterns inside it: `case x`, `case [*rest]`, `case {**rest}`.
pub(super) fn captured(pattern: &Pattern) -> Option<&str> {
    let name = match pattern {
        Pattern::MatchAs(inner) => inner.name.as_ref(),
        Pattern::MatchStar(inner) => inner.name.as_ref(),
        Pattern::MatchMapping(inner) => inner.rest.as_ref(),
        _ => None,
    };
    name.map(ruff_python_ast::Identifier::as_str)
}

/// Collects the names every import of the file binds, wherever it stands, and which of them come from [`LIBRARY_MODULES`].
struct ImportScan<'t> {
    imported: &'t mut HashSet<String>,
    library: &'t mut HashSet<String>,
}

impl<'a> Visitor<'a> for ImportScan<'_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Import(import) => {
                import_names(import, &mut |name| {
                    self.imported.insert(name.to_owned());
                });
                for alias in &import.names {
                    if LIBRARY_MODULES.contains(&alias.name.as_str()) {
                        let local = alias.asname.as_ref().map_or_else(
                            || alias.name.as_str().split('.').next().unwrap_or_default(),
                            |local| local.as_str(),
                        );
                        self.library.insert(local.to_owned());
                    }
                }
            }
            Stmt::ImportFrom(import) => {
                import_from_names(import, &mut |name| {
                    self.imported.insert(name.to_owned());
                });
                let from_library = import.level == 0
                    && import
                        .module
                        .as_ref()
                        .is_some_and(|module| LIBRARY_MODULES.contains(&module.as_str()));
                if from_library {
                    import_from_names(import, &mut |name| {
                        self.library.insert(name.to_owned());
                    });
                }
            }
            other => visitor::walk_stmt(self, other),
        }
    }
}

/// Finds `receiver.name = ...` and `receiver.name: T` in a method.
struct SelfScan<'t> {
    table: &'t Table,
    receiver: &'t str,
    typed: &'t HashMap<String, Ty>,
    info: &'t mut ClassInfo,
}

impl SelfScan<'_> {
    /// The member `target` is, when it is an attribute of the receiver.
    fn member<'e>(&self, target: &'e Expr) -> Option<&'e str> {
        let Expr::Attribute(attribute) = target else {
            return None;
        };
        let Expr::Name(root) = &*attribute.value else {
            return None;
        };
        (root.id.as_str() == self.receiver).then_some(attribute.attr.as_str())
    }

    /// Notes the member, and what the statement that writes it gives its field.
    fn note(&mut self, member: &str, note: impl FnOnce(&mut Field)) {
        self.info.members.insert(member.to_owned());
        note(self.info.fields.entry(member.to_owned()).or_default());
    }
}

impl<'a> Visitor<'a> for SelfScan<'_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            // A function or a class inside the method has an instance of its own.
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            Stmt::Assign(assign) => {
                let ty = self.table.value_type(&assign.value, self.typed);
                for target in &assign.targets {
                    if let Some(member) = self.member(target) {
                        self.note(member, |field| field.assigned(&assign.value, ty.clone()));
                    }
                }
            }
            Stmt::AnnAssign(assign) => {
                if let Some(member) = self.member(&assign.target) {
                    let ty = self.table.annotation_type(&assign.annotation);
                    self.note(member, |field| field.annotated(ty));
                }
            }
            other => visitor::walk_stmt(self, other),
        }
    }

    fn visit_expr(&mut self, _expr: &'a Expr) {}
}
