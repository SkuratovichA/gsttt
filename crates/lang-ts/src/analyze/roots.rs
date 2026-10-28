//! The names of a file that start a chain without being written as the root of a rule: `self` after `const self = this`, and `reader` or `this.db` when their declared type is one the rules list as a root type. A name counts only when every binding of it in the file is of that kind, so `self` bound to `this` in one method and to a mock in another function is neither.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use gsttt_core::rules::{ChainRule, Pack};
use oxc_ast::ast::{
    BindingIdentifier, BindingPattern, Class, Expression, FormalParameter, Function, Program,
    PropertyDefinition, TSEnumDeclaration, TSInterfaceDeclaration, TSType, TSTypeAliasDeclaration,
    TSTypeAnnotation, TSTypeName, TSTypeParameter, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_syntax::scope::ScopeFlags;

use crate::shape::{field_name, this_key};

/// The generic types that keep the members of their first type argument: `Pick<PrismaClient, 'tenant'>` is a client with fewer models.
const WRAPPERS: [&str; 6] = [
    "Pick",
    "Omit",
    "Partial",
    "Required",
    "Readonly",
    "NonNullable",
];

/// The names of one file that start a chain on top of the roots the rules list. Both sets hold per file, not per scope, and only for a name that nothing else binds anywhere in the file.
#[derive(Debug, Default)]
pub(super) struct Roots<'p> {
    /// Names bound by `const self = this`.
    this_aliases: BTreeSet<String>,
    /// Names declared with a root type of some rule, by name (`reader`, `this.db`), with the root types they were declared with.
    typed: BTreeMap<String, Vec<&'p str>>,
}

impl Roots<'_> {
    /// `names` read from `this` when the chain starts at a name bound by `const self = this`.
    pub(super) fn through_this<'n>(
        &self,
        names: &'n [Option<String>],
    ) -> Cow<'n, [Option<String>]> {
        match names.split_first() {
            Some((Some(head), rest)) if self.this_aliases.contains(head.as_str()) => Cow::Owned(
                std::iter::once(Some("this".to_owned()))
                    .chain(rest.iter().cloned())
                    .collect(),
            ),
            _ => Cow::Borrowed(names),
        }
    }

    /// Whether `root` is a name declared with one of the root types of `rule`.
    pub(super) fn is_typed_root(&self, rule: &ChainRule, root: &str) -> bool {
        self.typed.get(root).is_some_and(|declared| {
            declared
                .iter()
                .any(|ty| rule.root_types.iter().any(|root_type| root_type == ty))
        })
    }
}

/// The aliases of `this` and the typed roots of `program`. A project whose rules start at no `this` and list no root types skips the walk.
pub(super) fn collect<'p>(program: &Program<'_>, packs: &'p [Pack]) -> Roots<'p> {
    let rules = || packs.iter().flat_map(|pack| &pack.chains);
    let wanted: Vec<&str> = rules()
        .flat_map(|rule| &rule.root_types)
        .map(String::as_str)
        .collect();
    let starts_at_this = rules().any(|rule| {
        rule.root
            .iter()
            .any(|root| root == "this" || root.starts_with("this."))
    });
    if wanted.is_empty() && !starts_at_this {
        return Roots::default();
    }
    let mut collector = Collector {
        wanted,
        heads: Vec::new(),
        declared: Vec::new(),
        types: BTreeMap::new(),
        this_bound: BTreeSet::new(),
        others: Vec::new(),
    };
    collector.visit_program(program);
    collector.finish()
}

/// What a binding binds: a name in scope, or the property `this.<name>` of a class.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Name<'a> {
    Local(&'a str),
    Property(&'a str),
}

impl Name<'_> {
    /// The name as a chain of the rules writes it.
    fn key(self) -> String {
        match self {
            Self::Local(name) => name.to_owned(),
            Self::Property(name) => this_key(name),
        }
    }
}

/// What the walk saw, before the aliases of types are followed: a type alias may come after the name that uses it.
struct Collector<'p, 'a> {
    /// Every root type of every rule.
    wanted: Vec<&'p str>,
    /// The names that the types of the bindings stand for at their head, the rows of one binding after another.
    heads: Vec<Cow<'a, str>>,
    /// A binding that has a type, with the rows of `heads` that hold the type.
    declared: Vec<(Name<'a>, Range<usize>)>,
    /// Every name the file declares as a `type` alias, an `interface`, a `class` or an `enum`, with the names that its definition stands for at its head, which only a `type` alias has: such a name is the type the file says it is, never the root type of a pack that has the same name.
    types: BTreeMap<&'a str, Vec<Cow<'a, str>>>,
    /// Every name that `const name = this` binds.
    this_bound: BTreeSet<&'a str>,
    /// Every binding that is neither `this` nor given a type: a name bound to anything else is no alias and no root for the whole file.
    others: Vec<Name<'a>>,
}

impl<'a> Collector<'_, 'a> {
    /// Binds each of `names` to the type `annotation` when it stands for some name at its head, and to something else when it does not.
    fn bind(
        &mut self,
        names: impl IntoIterator<Item = Name<'a>>,
        annotation: Option<&TSTypeAnnotation<'a>>,
    ) {
        let start = self.heads.len();
        if let Some(annotation) = annotation.filter(|_| !self.wanted.is_empty()) {
            heads(&annotation.type_annotation, &mut |head| {
                self.heads.push(head);
            });
        }
        let rows = start..self.heads.len();
        if rows.is_empty() {
            self.others.extend(names);
        } else {
            self.declared
                .extend(names.into_iter().map(|name| (name, rows.clone())));
        }
    }

    /// The file declares a type called `name`.
    fn declare_type(&mut self, name: &'a str) {
        if !self.wanted.is_empty() {
            self.types.entry(name).or_default();
        }
    }
}

impl<'p> Collector<'p, '_> {
    fn finish(&self) -> Roots<'p> {
        let mut typed: BTreeMap<Name<'_>, Vec<&'p str>> = BTreeMap::new();
        let mut untyped = Vec::new();
        let mut work = Vec::new();
        let mut seen = BTreeSet::new();
        for (name, rows) in &self.declared {
            seen.clear();
            work.extend(
                self.heads
                    .get(rows.clone())
                    .unwrap_or_default()
                    .iter()
                    .map(Cow::as_ref),
            );
            let mut found = Vec::new();
            self.root_types(&mut work, &mut seen, &mut found);
            if found.is_empty() {
                untyped.push(*name);
            } else {
                typed.entry(*name).or_default().extend(found);
            }
        }
        let bound_otherwise =
            |name: Name<'_>| self.others.contains(&name) || untyped.contains(&name);
        Roots {
            this_aliases: self
                .this_bound
                .iter()
                .filter(|&&name| !bound_otherwise(Name::Local(name)))
                .map(|&name| name.to_owned())
                .collect(),
            typed: typed
                .into_iter()
                .filter(|&(name, _)| !bound_otherwise(name))
                .map(|(name, found)| (name.key(), found))
                .collect(),
        }
    }

    /// Empties `work`, a list of names, and adds to `found` the root types that they are, or stand for through the aliases of the file. A name the file declares is the type it says it is, which only a `type` alias can make a root type; a name the file does not declare is a root type when a pack lists it. `seen` holds the names already followed for this question and is never emptied during it, so that a chain of any length is followed, a cycle ends, and a union that names its own alias many times is read once. The names wait on a list and not on the stack, because a chain of a hundred thousand aliases must not need a hundred thousand frames.
    fn root_types<'s>(
        &'s self,
        work: &mut Vec<&'s str>,
        seen: &mut BTreeSet<&'s str>,
        found: &mut Vec<&'p str>,
    ) {
        while let Some(head) = work.pop() {
            if !seen.insert(head) {
                continue;
            }
            match self.types.get(head) {
                Some(rows) => work.extend(rows.iter().map(Cow::as_ref)),
                None => found.extend(self.wanted.iter().copied().find(|wanted| *wanted == head)),
            }
        }
    }
}

impl<'a> Visit<'a> for Collector<'_, 'a> {
    /// Every name that no visitor below classifies is bound to something else: a destructured parameter, a function, a class, an import, a catch clause.
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.others.push(Name::Local(it.name.as_str()));
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let BindingPattern::BindingIdentifier(id) = &it.id {
            let name = id.name.as_str();
            if it.init.as_ref().is_some_and(|init| {
                matches!(init.get_inner_expression(), Expression::ThisExpression(_))
            }) {
                self.this_bound.insert(name);
            } else {
                self.bind([Name::Local(name)], it.type_annotation.as_deref());
            }
        } else {
            self.visit_binding_pattern(&it.id);
        }
        if let Some(init) = &it.init {
            self.visit_expression(init);
        }
    }

    fn visit_formal_parameter(&mut self, it: &FormalParameter<'a>) {
        if let BindingPattern::BindingIdentifier(id) = &it.pattern {
            let name = id.name.as_str();
            // `constructor(private readonly db: PrismaService)` declares the property `this.db` as well.
            let property =
                (it.accessibility.is_some() || it.readonly).then_some(Name::Property(name));
            self.bind(
                std::iter::once(Name::Local(name)).chain(property),
                it.type_annotation.as_deref(),
            );
        } else {
            self.visit_binding_pattern(&it.pattern);
        }
        for decorator in &it.decorators {
            self.visit_decorator(decorator);
        }
        if let Some(initializer) = &it.initializer {
            self.visit_expression(initializer);
        }
    }

    fn visit_property_definition(&mut self, it: &PropertyDefinition<'a>) {
        if let Some(name) = field_name(it) {
            self.bind([Name::Property(name)], it.type_annotation.as_deref());
        }
        walk::walk_property_definition(self, it);
    }

    /// An overload, a `declare function` and an abstract method have no body, and the parameters of a signature without a body bind nothing.
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        if it.body.is_some() {
            walk::walk_function(self, it, flags);
        }
    }

    /// The members of an interface are signatures without a body.
    fn visit_ts_interface_declaration(&mut self, it: &TSInterfaceDeclaration<'a>) {
        self.declare_type(it.id.name.as_str());
    }

    fn visit_ts_type_alias_declaration(&mut self, it: &TSTypeAliasDeclaration<'a>) {
        if !self.wanted.is_empty() {
            let rows = self.types.entry(it.id.name.as_str()).or_default();
            heads(&it.type_annotation, &mut |head| rows.push(head));
        }
    }

    /// A type binds no name of the file: the parameters of its function types and the keys of its mapped types only describe.
    fn visit_ts_type(&mut self, _it: &TSType<'a>) {}

    /// The name of a type parameter belongs to the types.
    fn visit_ts_type_parameter(&mut self, _it: &TSTypeParameter<'a>) {}

    fn visit_ts_enum_declaration(&mut self, it: &TSEnumDeclaration<'a>) {
        self.declare_type(it.id.name.as_str());
        walk::walk_ts_enum_declaration(self, it);
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        if let Some(id) = &it.id {
            self.declare_type(id.name.as_str());
        }
        walk::walk_class(self, it);
    }
}

/// Calls `found` with the name of every type that `ty` stands for at its head: a reference is its own name, a union is each of its members, and a wrapper such as `Pick<T, …>` is what it wraps.
fn heads<'a>(ty: &TSType<'a>, found: &mut impl FnMut(Cow<'a, str>)) {
    match ty {
        TSType::TSParenthesizedType(inner) => heads(&inner.type_annotation, found),
        TSType::TSUnionType(union) => {
            for member in &union.types {
                heads(member, found);
            }
        }
        TSType::TSTypeReference(reference) => match &reference.type_name {
            TSTypeName::IdentifierReference(name) if WRAPPERS.contains(&name.name.as_str()) => {
                let wrapped = reference
                    .type_arguments
                    .as_ref()
                    .and_then(|arguments| arguments.params.first());
                if let Some(wrapped) = wrapped {
                    heads(wrapped, found);
                }
            }
            TSTypeName::IdentifierReference(name) => found(Cow::Borrowed(name.name.as_str())),
            qualified => found(Cow::Owned(qualified.to_string())),
        },
        _ => {}
    }
}
