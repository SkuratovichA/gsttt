//! The names of a file that a rule reads without their being written in it: `self` after `const self = this`, and `reader` or `this.db` when their declared type, or the class they are made of with `new`, is one the rules list as the root type of a chain or as the receiver type of a call. A name counts only when every binding of it in the file is of that kind, so `self` bound to `this` in one method and to a mock in another function is neither.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use gsttt_core::rules::{ChainRule, Pack};
use oxc_ast::ast::{
    BindingIdentifier, BindingPattern, Class, Expression, FormalParameter, Function, Program,
    PropertyDefinition, TSEnumDeclaration, TSInterfaceDeclaration, TSType, TSTypeAliasDeclaration,
    TSTypeAnnotation, TSTypeName, TSTypeParameter, TSTypeParameterInstantiation,
    VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_syntax::scope::ScopeFlags;

use crate::shape::{dotted_name, field_name, is_parameter_property, this_key};

/// The generic types that keep the members of their first type argument: `Pick<PrismaClient, 'tenant'>` is a client with fewer models.
const WRAPPERS: [&str; 6] = [
    "Pick",
    "Omit",
    "Partial",
    "Required",
    "Readonly",
    "NonNullable",
];

/// The names of one file that a rule reads on top of the roots and callees the rules list. Both sets hold per file, not per scope, and only for a name that nothing else binds anywhere in the file.
#[derive(Debug, Default)]
pub(super) struct Roots<'p> {
    /// Names bound by `const self = this`.
    this_aliases: BTreeSet<String>,
    /// Names declared with a type some rule lists, by name (`reader`, `this.db`), with the listed types they were declared with.
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

    /// Whether `name` is declared with one of `types`, the root types of a chain rule or the receiver types of a call rule.
    pub(super) fn is_typed(&self, types: &[String], name: &str) -> bool {
        self.typed.get(name).is_some_and(|declared| {
            declared
                .iter()
                .any(|ty| types.iter().any(|listed| listed == ty))
        })
    }
}

/// The chain rules of `packs`.
fn chain_rules(packs: &[Pack]) -> impl Iterator<Item = &ChainRule> {
    packs.iter().flat_map(|pack| &pack.chains)
}

/// Every type some rule of `packs` lists: the root types of the chain rules and the receiver types of the call rules.
fn listed_types(packs: &[Pack]) -> impl Iterator<Item = &str> {
    let roots = chain_rules(packs).flat_map(|rule| &rule.root_types);
    let receivers = packs
        .iter()
        .flat_map(|pack| &pack.calls)
        .flat_map(|rule| &rule.receiver_types);
    roots.chain(receivers).map(String::as_str)
}

/// The aliases of `this` and the typed names of `program`. A project whose rules start at no `this` and list no type skips the walk, and so does a file that binds no name which could start a chain or receive a call.
pub(super) fn collect<'p>(program: &Program<'_>, packs: &'p [Pack]) -> Roots<'p> {
    let reads_types = listed_types(packs).next().is_some();
    let starts_at_this = chain_rules(packs).any(|rule| {
        rule.root
            .iter()
            .any(|root| root == "this" || root.starts_with("this."))
    });
    if !reads_types && !starts_at_this {
        return Roots::default();
    }
    // Whether another binding takes a name back is known only at the end of the file, because that binding may come before the one that makes the name a root. So the first walk finds the names that could start a chain, and a second one, made only when there is such a name, finds the other bindings of those names: the bindings of every other name are never kept.
    let mut first = Collector::new(packs, reads_types, BTreeSet::new());
    first.visit_program(program);
    let watched = first.candidates();
    if watched.is_empty() {
        return Roots::default();
    }
    let mut second = Collector::new(packs, reads_types, watched);
    second.visit_program(program);
    second.finish()
}

/// The names of `names` that the file binds more than once. An import binds its name once, so a second binding of any kind, a parameter, a variable, a function or a class, hides the import wherever it is in scope, and the walk keeps no scope: the name is no import for the whole file.
pub(super) fn rebound<'a>(
    program: &Program<'a>,
    names: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    let mut walk = Collector::new(&[], false, names.into_iter().map(Name::Local).collect());
    walk.visit_program(program);
    walk.others
        .iter()
        .filter(|&(&name, &count)| {
            // `const name = this` is a binding that the count leaves out.
            count > 1 || matches!(name, Name::Local(local) if walk.this_bound.contains(local))
        })
        .map(|(name, _)| name.key())
        .collect()
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
    /// The packs, which list the types.
    packs: &'p [Pack],
    /// Whether some pack lists a type: when none does, the types of the file are not read.
    reads_types: bool,
    /// The names that the types of the bindings stand for at their head, the rows of one binding after another.
    heads: Vec<Cow<'a, str>>,
    /// A binding that has a type, with the rows of `heads` that hold the type.
    declared: Vec<(Name<'a>, Range<usize>)>,
    /// Every name the file declares as a `type` alias, an `interface`, a `class` or an `enum`, with the names that its definition stands for at its head, which only a `type` alias has: such a name is the type the file says it is, never the type of a pack that has the same name.
    types: BTreeMap<&'a str, Vec<Cow<'a, str>>>,
    /// Every name that `const name = this` binds.
    this_bound: BTreeSet<&'a str>,
    /// The names that could start a chain, which the first walk found, and the only ones whose other bindings are kept. Empty during the first walk.
    watched: BTreeSet<Name<'a>>,
    /// How many bindings each watched name has that are neither `this` nor given a type: a name bound to anything else is no alias and no root for the whole file.
    others: BTreeMap<Name<'a>, u32>,
}

impl<'p, 'a> Collector<'p, 'a> {
    fn new(packs: &'p [Pack], reads_types: bool, watched: BTreeSet<Name<'a>>) -> Self {
        Self {
            packs,
            reads_types,
            heads: Vec::new(),
            declared: Vec::new(),
            types: BTreeMap::new(),
            this_bound: BTreeSet::new(),
            watched,
            others: BTreeMap::new(),
        }
    }

    /// Binds each of `names` to its type when that stands for some name at its head, and to something else when it does not. The type is the annotation or, when there is none, the class that the initializer constructs with `new`.
    fn bind(
        &mut self,
        names: impl IntoIterator<Item = Name<'a>>,
        annotation: Option<&TSTypeAnnotation<'a>>,
        init: Option<&Expression<'a>>,
    ) {
        let start = self.heads.len();
        if self.reads_types {
            match annotation {
                Some(annotation) => heads(&annotation.type_annotation, &mut |head| {
                    self.heads.push(head);
                }),
                None => self.heads.extend(init.and_then(constructed)),
            }
        }
        let rows = start..self.heads.len();
        if rows.is_empty() {
            for name in names {
                self.other(name);
            }
        } else {
            self.declared
                .extend(names.into_iter().map(|name| (name, rows.clone())));
        }
    }

    /// The file binds `name` to something that is neither `this` nor given a type. Only a watched name keeps it.
    fn other(&mut self, name: Name<'a>) {
        if self.watched.contains(&name) {
            let count = self.others.entry(name).or_default();
            *count = count.saturating_add(1);
        }
    }

    /// The file declares a type called `name`. The names that the type stands for at its head go into the rows returned, which are absent when no rule lists a root type.
    fn declare_type(&mut self, name: &'a str) -> Option<&mut Vec<Cow<'a, str>>> {
        self.reads_types
            .then(|| self.types.entry(name).or_default())
    }

    /// The type that some pack lists under `name`.
    fn listed_type(&self, name: &str) -> Option<&'p str> {
        listed_types(self.packs).find(|listed| *listed == name)
    }

    /// The declared names whose types stand for listed types, with those types, and the declared names whose types do not.
    fn resolve(&self) -> (BTreeMap<Name<'a>, Vec<&'p str>>, BTreeSet<Name<'a>>) {
        let mut typed: BTreeMap<Name<'a>, Vec<&'p str>> = BTreeMap::new();
        let mut untyped = BTreeSet::new();
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
            self.find_listed(&mut work, &mut seen, &mut found);
            if found.is_empty() {
                untyped.insert(*name);
            } else {
                typed.entry(*name).or_default().extend(found);
            }
        }
        (typed, untyped)
    }

    /// The names that could start a chain or receive a call: the declared names with a listed type, and the names bound to `this`.
    fn candidates(&self) -> BTreeSet<Name<'a>> {
        let (typed, _) = self.resolve();
        typed
            .into_keys()
            .chain(self.this_bound.iter().copied().map(Name::Local))
            .collect()
    }

    fn finish(&self) -> Roots<'p> {
        let (typed, untyped) = self.resolve();
        let bound_otherwise =
            |name: Name<'a>| self.others.contains_key(&name) || untyped.contains(&name);
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

    /// Empties `work`, a list of names, and adds to `found` the listed types that they are, or stand for through the aliases of the file. A name the file declares is the type it says it is, which only a `type` alias can make a listed type; a name the file does not declare is a listed type when a pack lists it. `seen` holds the names already followed for this question and is never emptied during it, so that a chain of any length is followed, a cycle ends, and a union that names its own alias many times is read once. The names wait on a list and not on the stack, because a chain of a hundred thousand aliases must not need a hundred thousand frames.
    fn find_listed<'s>(
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
                None => found.extend(self.listed_type(head)),
            }
        }
    }
}

impl<'a> Visit<'a> for Collector<'_, 'a> {
    /// Every name that no visitor below classifies is bound to something else: a destructured parameter, a function, a class, an import, a catch clause.
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.other(Name::Local(it.name.as_str()));
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let BindingPattern::BindingIdentifier(id) = &it.id {
            let name = id.name.as_str();
            if it.init.as_ref().is_some_and(|init| {
                matches!(init.get_inner_expression(), Expression::ThisExpression(_))
            }) {
                self.this_bound.insert(name);
            } else {
                self.bind(
                    [Name::Local(name)],
                    it.type_annotation.as_deref(),
                    it.init.as_ref(),
                );
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
            let property = is_parameter_property(it).then_some(Name::Property(name));
            self.bind(
                std::iter::once(Name::Local(name)).chain(property),
                it.type_annotation.as_deref(),
                it.initializer.as_deref(),
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
            self.bind(
                [Name::Property(name)],
                it.type_annotation.as_deref(),
                it.value.as_ref(),
            );
        }
        walk::walk_property_definition(self, it);
    }

    /// An overload, a `declare function` and an abstract method have no body, and the parameters of a signature without a body bind nothing.
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        if it.body.is_some() {
            walk::walk_function(self, it, flags);
        }
    }

    /// The members of an interface are signatures without a body, and the types it extends are what it stands for.
    fn visit_ts_interface_declaration(&mut self, it: &TSInterfaceDeclaration<'a>) {
        if let Some(rows) = self.declare_type(it.id.name.as_str()) {
            for base in &it.extends {
                reference_heads(
                    &base.type_name,
                    base.type_arguments.as_deref(),
                    &mut |head| {
                        rows.push(head);
                    },
                );
            }
        }
    }

    fn visit_ts_type_alias_declaration(&mut self, it: &TSTypeAliasDeclaration<'a>) {
        if let Some(rows) = self.declare_type(it.id.name.as_str()) {
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

    /// A class is the class it extends, and a class that extends none is no root type.
    fn visit_class(&mut self, it: &Class<'a>) {
        if let Some(id) = &it.id
            && let Some(rows) = self.declare_type(id.name.as_str())
        {
            rows.extend(
                it.heritage
                    .as_ref()
                    .and_then(|heritage| dotted_name(&heritage.expression)),
            );
        }
        walk::walk_class(self, it);
    }
}

/// The class that `init` constructs: `PrismaClient` for `new PrismaClient({ log: ['query'] })`, and nothing for a call or any other expression.
pub(super) fn constructed<'a>(init: &Expression<'a>) -> Option<Cow<'a, str>> {
    match init.get_inner_expression() {
        Expression::NewExpression(new) => dotted_name(&new.callee),
        _ => None,
    }
}

/// Calls `found` with the name of every type that `ty` stands for at its head: a reference is its own name, a union or an intersection is each of its members, and a wrapper such as `Pick<T, …>` is what it wraps.
pub(super) fn heads<'a>(ty: &TSType<'a>, found: &mut impl FnMut(Cow<'a, str>)) {
    match ty {
        TSType::TSParenthesizedType(inner) => heads(&inner.type_annotation, found),
        TSType::TSUnionType(union) => {
            for member in &union.types {
                heads(member, found);
            }
        }
        TSType::TSIntersectionType(intersection) => {
            for member in &intersection.types {
                heads(member, found);
            }
        }
        TSType::TSTypeReference(reference) => {
            reference_heads(
                &reference.type_name,
                reference.type_arguments.as_deref(),
                found,
            );
        }
        _ => {}
    }
}

/// Like [`heads`] for the type `name<arguments>`, which an `extends` clause of an interface writes as well as a type reference does.
fn reference_heads<'a>(
    name: &TSTypeName<'a>,
    arguments: Option<&TSTypeParameterInstantiation<'a>>,
    found: &mut impl FnMut(Cow<'a, str>),
) {
    match name {
        TSTypeName::IdentifierReference(name) if WRAPPERS.contains(&name.name.as_str()) => {
            if let Some(wrapped) = arguments.and_then(|arguments| arguments.params.first()) {
                heads(wrapped, found);
            }
        }
        TSTypeName::IdentifierReference(name) => found(Cow::Borrowed(name.name.as_str())),
        qualified => found(Cow::Owned(qualified.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    use super::rebound;

    /// The names of `watched` that `code` binds more than once, in name order.
    fn rebound_in(code: &str, watched: &[&'static str]) -> Vec<String> {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, code, SourceType::ts()).parse();
        rebound(&parsed.program, watched.iter().copied())
            .into_iter()
            .collect()
    }

    #[test]
    fn a_name_that_is_bound_again_anywhere_in_the_file_is_rebound() {
        for code in [
            "import { A } from 'a'; function f(A: string) {}",
            "import { A } from 'a'; const f = (A) => A;",
            "import { A } from 'a'; function f({ A }) {}",
            "import { A } from 'a'; function f() { const A = 1; }",
            "import { A } from 'a'; function f() { let [A] = []; }",
            "import { A } from 'a'; function A() {}",
            "import { A } from 'a'; class B { m() { class A {} } }",
            "import { A } from 'a'; try {} catch (A) {}",
            "import { A } from 'a'; class B { constructor(private A: string) {} }",
            "import { A } from 'a'; function f() { const A = this; }",
            "import { A } from 'a'; function f() { for (const A of []) {} }",
            "import { A } from 'a'; enum A { X }",
        ] {
            assert_eq!(rebound_in(code, &["A"]), ["A"], "{code}");
        }
    }

    #[test]
    fn a_name_that_nothing_binds_again_is_not_rebound() {
        for code in [
            "import { A } from 'a'; fetch(A);",
            "import { A } from 'a'; type A2 = A; interface B { A: string }",
            "import { A } from 'a'; const o = { A: 1, B: A }; o.A;",
            "import { A } from 'a'; class B { A = 1; m() { return this.A; } }",
            "import { A } from 'a'; type F = (A: string) => void;",
            "import { A } from 'a'; declare function f(A: string): void;",
            "import { A } from 'a'; function f<A>(x: A) {}",
            "import { A } from 'a'; function f(B: string) {}",
        ] {
            assert_eq!(rebound_in(code, &["A"]), Vec::<String>::new(), "{code}");
        }
    }

    #[test]
    fn only_the_names_asked_for_are_followed() {
        let code = "import { A, B } from 'x'; function f(A, B) {} function g(C) {}";
        assert_eq!(rebound_in(code, &["A", "B"]), ["A", "B"]);
        assert_eq!(rebound_in(code, &["A", "C"]), ["A"]);
    }
}
