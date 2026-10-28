//! The names of a file that start a chain without being written as the root of a rule: `self` after `const self = this`, and `reader` or `this.db` when their declared type is one the rules list as a root type.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::rules::{ChainRule, Pack};
use oxc_ast::ast::{
    BindingPattern, Expression, FormalParameter, Program, PropertyDefinition, TSType,
    TSTypeAliasDeclaration, TSTypeName, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};

use crate::shape::property_name;

/// How many aliases deep a type is followed, so that `type A = B; type B = A` ends.
const ALIAS_DEPTH: usize = 8;

/// The generic types that keep the members of their first type argument: `Pick<PrismaClient, 'tenant'>` is a client with fewer models.
const WRAPPERS: [&str; 6] = [
    "Pick",
    "Omit",
    "Partial",
    "Required",
    "Readonly",
    "NonNullable",
];

/// The names of one file that start a chain on top of the roots the rules list. Both sets hold per file, not per scope: a name declared in one function also counts in another function of the same file.
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
        declared: Vec::new(),
        aliases: Vec::new(),
        this_aliases: BTreeSet::new(),
    };
    collector.visit_program(program);
    collector.finish()
}

/// What the walk saw, before the aliases of types are followed: a type alias may come after the name that uses it.
struct Collector<'p, 'a> {
    /// Every root type of every rule.
    wanted: Vec<&'p str>,
    /// A declared name and a name its type stands for at its head, one row for each.
    declared: Vec<(Cow<'a, str>, Cow<'a, str>)>,
    /// A local `type` alias and a name its definition stands for at its head, one row for each.
    aliases: Vec<(&'a str, Cow<'a, str>)>,
    this_aliases: BTreeSet<String>,
}

impl<'a> Collector<'_, 'a> {
    /// The name `name()` is declared with `ty`. The name is built once for each row, so a type that stands for nothing costs no allocation.
    fn declare(&mut self, name: impl Fn() -> Cow<'a, str>, ty: &TSType<'a>) {
        if self.wanted.is_empty() {
            return;
        }
        heads(ty, &mut |head| self.declared.push((name(), head)));
    }
}

impl<'p> Collector<'p, '_> {
    fn finish(self) -> Roots<'p> {
        let mut typed: BTreeMap<String, Vec<&'p str>> = BTreeMap::new();
        for (name, head) in &self.declared {
            let mut found = Vec::new();
            self.root_types(head, ALIAS_DEPTH, &mut found);
            if !found.is_empty() {
                typed
                    .entry(name.clone().into_owned())
                    .or_default()
                    .extend(found);
            }
        }
        Roots {
            this_aliases: self.this_aliases,
            typed,
        }
    }

    /// Adds to `found` the root types that `head` is, or stands for through the aliases of the file.
    fn root_types(&self, head: &str, depth: usize, found: &mut Vec<&'p str>) {
        if let Some(root_type) = self.wanted.iter().copied().find(|wanted| *wanted == head) {
            found.push(root_type);
        } else if let Some(depth) = depth.checked_sub(1) {
            for (_, next) in self.aliases.iter().filter(|(alias, _)| *alias == head) {
                self.root_types(next, depth, found);
            }
        }
    }
}

impl<'a> Visit<'a> for Collector<'_, 'a> {
    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let BindingPattern::BindingIdentifier(id) = &it.id {
            let name = id.name.as_str();
            if it.init.as_ref().is_some_and(|init| {
                matches!(init.get_inner_expression(), Expression::ThisExpression(_))
            }) {
                self.this_aliases.insert(name.to_owned());
            }
            if let Some(annotation) = &it.type_annotation {
                self.declare(|| Cow::Borrowed(name), &annotation.type_annotation);
            }
        }
        walk::walk_variable_declarator(self, it);
    }

    fn visit_formal_parameter(&mut self, it: &FormalParameter<'a>) {
        if let (BindingPattern::BindingIdentifier(id), Some(annotation)) =
            (&it.pattern, &it.type_annotation)
        {
            let name = id.name.as_str();
            self.declare(|| Cow::Borrowed(name), &annotation.type_annotation);
            // `constructor(private readonly db: PrismaService)` declares the property `this.db` as well.
            if it.accessibility.is_some() || it.readonly {
                self.declare(
                    || Cow::Owned(format!("this.{name}")),
                    &annotation.type_annotation,
                );
            }
        }
        walk::walk_formal_parameter(self, it);
    }

    fn visit_property_definition(&mut self, it: &PropertyDefinition<'a>) {
        if !it.computed
            && let Some(annotation) = &it.type_annotation
            && let Some(name) = property_name(&it.key)
        {
            self.declare(
                || Cow::Owned(format!("this.{name}")),
                &annotation.type_annotation,
            );
        }
        walk::walk_property_definition(self, it);
    }

    fn visit_ts_type_alias_declaration(&mut self, it: &TSTypeAliasDeclaration<'a>) {
        if !self.wanted.is_empty() {
            let alias = it.id.name.as_str();
            heads(&it.type_annotation, &mut |head| {
                self.aliases.push((alias, head));
            });
        }
        walk::walk_ts_type_alias_declaration(self, it);
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
