//! One syntax tree read for imports, exports, contracts, embedded documents and what the link phase needs.

mod makers;
mod maps;
mod roots;
mod symbols;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::fact::{
    ContractFact, Embedded, Import, ImportFlags, ImportKind, Line, Symbols, UnresolvedReason,
    UnresolvedSite,
};
use gsttt_core::rules::{Outcome, Pack, Prefix, Site};
use gsttt_core::text::LineIndex;
use oxc_ast::ast::{
    AccessorProperty, Argument, ArrowFunctionExpression, AssignmentExpression, AssignmentTarget,
    AssignmentTargetPropertyIdentifier, BindingIdentifier, BindingPattern, CallExpression, Class,
    ClassBody, Declaration, Decorator, ExportAllDeclaration, ExportDeclaration,
    ExportDefaultDeclaration, ExportDefaultDeclarationKind, ExportFromDeclaration,
    ExportNamedDeclaration, Expression, FormalParameter, Function, IdentifierReference,
    ImportDeclaration, ImportDeclarationSpecifier, ImportExpression, ImportOrExportKind,
    MemberExpression, MethodDefinition, ModuleExportName, NewExpression, ObjectProperty,
    ObjectPropertyKind, PrivateInExpression, Program, PropertyDefinition, SimpleAssignmentTarget,
    TSExportAssignment, TSImportEqualsDeclaration, TSImportType, TSImportTypeQualifier,
    TSModuleBlock, TSModuleReference, TaggedTemplateExpression, UnaryExpression,
    VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::{GetSpan, Span};
use oxc_syntax::operator::UnaryOperator;
use oxc_syntax::scope::ScopeFlags;

use self::makers::Maker;
use self::roots::Roots;
use self::symbols::Recorder;
use crate::extra::{Binding, Export, ExportTarget, Mount, MountTarget, Route, TsExtra};
use crate::shape::{
    Shape, chain_of, dotted, dotted_name, field_name, literal_text, member_chain, property_name,
    property_str, static_chain, template_text, text_of,
};

/// Everything one file says on its own.
#[derive(Debug, Default)]
pub(crate) struct Analysis {
    pub(crate) imports: Vec<Import>,
    pub(crate) contracts: Vec<ContractFact>,
    pub(crate) embedded: Vec<Embedded>,
    pub(crate) unresolved: Vec<UnresolvedSite>,
    /// The names the file exports itself, sorted, once each, and without what an `export *` brings. `None` when its syntax does not list them: it assigns `module.exports` or `exports`, or has `export =`.
    pub(crate) exports: Option<Vec<String>>,
    /// The declarations of the file, what each uses and what the file runs as it loads.
    pub(crate) symbols: Option<Symbols>,
    pub(crate) extra: TsExtra,
}

/// Reads `program`, written as `text`, with the rules of `packs`.
pub(crate) fn analyze(
    program: &Program<'_>,
    text: &str,
    index: &LineIndex,
    packs: &[Pack],
) -> Analysis {
    let mut analyzer = Analyzer {
        text,
        index,
        packs,
        makers: makers::collect(program, packs),
        roots: roots::collect(program, packs),
        symbols: Recorder::collect(program, index),
        done: BTreeSet::new(),
        names: BTreeMap::new(),
        classes: Vec::new(),
        has_graphql: packs.iter().any(|pack| pack.name == "graphql"),
        has_trpc: packs.iter().any(|pack| pack.name == "trpc"),
        exports_unknown: false,
        module_depth: 0,
        out: Analysis::default(),
    };
    analyzer.visit_program(program);
    analyzer.finish()
}

/// What a load names of the module it loads.
#[derive(Clone, Copy)]
enum Taking<'p, 'a> {
    /// Nothing: a mock, `import('x').then(start)`, a `require` used as it stands.
    Nothing,
    /// What a declaration takes: `const { a } = await import('x')`, `const x = require('x')`.
    Declared(&'p BindingPattern<'a>),
    /// One member read from the loaded value: `(await import('x')).a`. It binds no local.
    Member(&'p str),
}

/// A class being walked: the decorators it carries, and the class it extends.
struct ClassFrame {
    decorators: Vec<(String, Shape)>,
    heritage: Option<String>,
}

struct Analyzer<'p, 'a> {
    text: &'p str,
    index: &'p LineIndex,
    packs: &'p [Pack],
    /// Variables made by a call in this file, found before the walk so that a route may come before its router.
    makers: BTreeMap<String, Maker>,
    /// The aliases of `this` and the names declared with a root type, found before the walk because a class may use `self` above the method that binds it.
    roots: Roots<'p>,
    /// The table of the top-level bindings, found before the walk, and what the walk finds against it: the declarations, what each uses and what the file runs as it loads.
    symbols: Recorder<'a>,
    /// Starts of the calls whose meaning another visit already took, so they are read once.
    done: BTreeSet<u32>,
    /// The name a call is stored under, `appRouter` for `const appRouter = router({…})`, by the start of the call.
    names: BTreeMap<u32, String>,
    classes: Vec<ClassFrame>,
    has_graphql: bool,
    has_trpc: bool,
    /// Whether the file assigns `module.exports` or `exports`, or has `export =`, so that its syntax does not list what it exports.
    exports_unknown: bool,
    /// How many `namespace` and `declare module` bodies the walk is in, whose `export`s are theirs and not the file's.
    module_depth: u32,
    out: Analysis,
}

impl<'a> Analyzer<'_, 'a> {
    fn line(&self, offset: u32) -> Line {
        self.index.line(offset)
    }

    /// The first line of the source at `span`, cut short, to name a site that cannot be read.
    fn snippet(&self, span: Span) -> String {
        let start = usize::try_from(span.start).unwrap_or(0);
        let end = usize::try_from(span.end).unwrap_or(0);
        let text = self.text.get(start..end).unwrap_or_default();
        text.lines()
            .next()
            .unwrap_or_default()
            .trim()
            .chars()
            .take(80)
            .collect()
    }

    fn bind(&mut self, local: &str, spec: &str, name: &str) {
        self.out.extra.bindings.push(Binding {
            local: local.to_owned(),
            spec: spec.to_owned(),
            name: name.to_owned(),
        });
    }

    fn export(&mut self, name: &str, target: ExportTarget) {
        if self.module_depth > 0 {
            return;
        }
        self.out.extra.exports.push(Export {
            name: name.to_owned(),
            target,
        });
    }

    fn record(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Fact(fact) => self.out.contracts.push(fact),
            Outcome::Blind(site) => self.out.unresolved.push(site),
            Outcome::Skip => {}
        }
    }

    /// A literal `require`, `import()` or mock call becomes an import, with the names `taking` says it takes from the module; one that is computed is a blind spot. Each call is read once.
    fn load(
        &mut self,
        span: Span,
        argument: Option<&Expression<'_>>,
        kind: ImportKind,
        taking: Taking<'_, '_>,
    ) {
        if !self.done.insert(span.start) {
            return;
        }
        let line = self.line(span.start);
        let Some(spec) = argument.and_then(literal_text) else {
            let detail = self.snippet(span);
            self.out.unresolved.push(UnresolvedSite {
                line,
                reason: UnresolvedReason::NonLiteral,
                detail,
            });
            return;
        };
        let namespace = if matches!(kind, ImportKind::Require) {
            "default"
        } else {
            "*"
        };
        let mut import = Import::new(spec.clone(), kind, line);
        match taking {
            Taking::Member(name) => import.names.push(name.to_owned()),
            Taking::Declared(BindingPattern::BindingIdentifier(local)) => {
                self.bind(local.name.as_str(), &spec, namespace);
                import.names.push(namespace.to_owned());
            }
            Taking::Declared(BindingPattern::ObjectPattern(object)) => {
                // A rest element, or a key that is computed, may take any name, so the load takes the module itself as a namespace does.
                let mut open = object.rest.is_some();
                for property in &object.properties {
                    let name = if property.computed {
                        None
                    } else {
                        property_name(&property.key)
                    };
                    let Some(name) = name else {
                        open = true;
                        continue;
                    };
                    if let BindingPattern::BindingIdentifier(local) = &property.value {
                        self.bind(local.name.as_str(), &spec, &name);
                    }
                    import.names.push(name);
                }
                if open {
                    import.names.push(namespace.to_owned());
                }
            }
            // A load used as it stands names nothing, and neither does a declared pattern such as `[a]`.
            Taking::Nothing | Taking::Declared(_) => {}
        }
        self.out.imports.push(import);
    }

    /// `(await import('y')).a`, `(await vi.importActual('y')).a`, `require('y').a` and `jest.requireActual('y').a` take the name `a` from `y`. A load that hands back a promise, such as `import('y')` without `await`, names nothing when a member is read from it. The inner visit of the load reaches `load` with the same span and is skipped.
    fn member_of_load(&mut self, object: &Expression<'_>, member: &str) {
        let (awaited, inner) = match object.get_inner_expression() {
            Expression::AwaitExpression(awaited) => (true, &awaited.argument),
            other => (false, other),
        };
        if let Some(load) = load_of(inner).filter(|load| load.promise == awaited) {
            self.load(load.span, load.argument, load.kind, Taking::Member(member));
        }
    }

    /// `const x = require('y')` and `const { a } = await import('y')`.
    fn load_declared(&mut self, init: &Expression<'_>, pattern: &BindingPattern<'_>) {
        if let Some(load) = load_of(init) {
            self.load(
                load.span,
                load.argument,
                load.kind,
                Taking::Declared(pattern),
            );
        }
    }

    /// `const { A, B } = process.env` reads the chains `process.env.A` and `process.env.B`.
    fn destructured_chain(
        &mut self,
        pattern: &BindingPattern<'_>,
        init: &Expression<'_>,
        offset: u32,
    ) {
        let BindingPattern::ObjectPattern(object) = pattern else {
            return;
        };
        let Some(base) = chain_of(init) else {
            return;
        };
        for property in &object.properties {
            let mut names = base.clone();
            names.push(if property.computed {
                None
            } else {
                property_name(&property.key)
            });
            self.chain_rules(&names, offset);
        }
    }

    /// Applies the chain rules to every root the chain starts with: a root a rule lists, a name declared with one of its root types, or the alias of `this` that the chain starts with, read as `this`. A rule that lists terminals needs one of them after the first segment, so `trpc.useUtils()` and `prisma.$transaction()` name nothing.
    fn chain_rules(&mut self, names: &[Option<String>], offset: u32) {
        let line = self.line(offset);
        let packs = self.packs;
        let names = self.roots.through_this(names);
        let names = names.as_ref();
        for pack in packs {
            for rule in &pack.chains {
                for count in 1..names.len() {
                    let Some((head, tail)) = names.split_at_checked(count) else {
                        break;
                    };
                    let Some(root) = dotted(head) else {
                        break;
                    };
                    if !(rule.root.matches(&root) || self.roots.is_typed_root(rule, &root)) {
                        continue;
                    }
                    let segments: Vec<Option<&str>> = tail.iter().map(Option::as_deref).collect();
                    let ended =
                        segments.iter().skip(1).flatten().any(|segment| {
                            rule.terminals.iter().any(|terminal| terminal == segment)
                        });
                    if !rule.terminals.is_empty() && !ended {
                        continue;
                    }
                    let outcome = rule.apply(&pack.name, &segments, line, &root);
                    self.record(outcome);
                }
            }
        }
    }

    /// Applies the import rules to a name the file takes from the package `spec`; `line` is the line of the import.
    fn import_rules(&mut self, spec: &str, name: &str, line: Line) {
        let packs = self.packs;
        for pack in packs {
            for rule in pack.imports.iter().filter(|rule| rule.reads(spec)) {
                let outcome = rule.apply(&pack.name, name, line);
                self.record(outcome);
            }
        }
    }

    /// The prefix the call that made the receiver of `callee` gives, and the receiver's name, when the receiver was made in this file by a call that `rule` names.
    fn receiver<'c>(&self, callee: &'c str, rule: &Prefix) -> Option<(String, &'c str)> {
        let (host, _) = callee.rsplit_once('.')?;
        let maker = self.makers.get(host)?;
        rule.from
            .matches(&maker.callee)
            .then(|| (rule.read(&maker.shape), host))
    }

    fn call(&mut self, it: &CallExpression<'_>, callee: &str) {
        let line = self.line(it.span.start);
        let packs = self.packs;
        let mut lazy: Option<Shape> = None;
        for pack in packs {
            for rule in &pack.calls {
                if !rule.callee.matches(callee) {
                    continue;
                }
                let site: &Shape =
                    lazy.get_or_insert_with(|| Shape::of_call(callee, it.arguments.iter()));
                // `app.get('env')` reads a setting: a route always has a handler after its path.
                if rule.receiver.is_some()
                    && rule.kind == ContractKind::Http
                    && rule.role == Role::Serves
                    && it.arguments.len() < 2
                {
                    continue;
                }
                let (prefix, host) = match &rule.receiver {
                    None => (None, None),
                    Some(receiver) => match self.receiver(callee, receiver) {
                        Some((prefix, host)) => (Some(prefix), Some(host)),
                        None => continue,
                    },
                };
                let outcome = rule.apply(&pack.name, site, prefix.as_deref(), line, callee);
                match (outcome, host) {
                    (Outcome::Fact(fact), Some(host))
                        if rule.kind == ContractKind::Http && rule.role == Role::Serves =>
                    {
                        self.out.extra.routes.push(Route {
                            host: host.to_owned(),
                            fact,
                        });
                    }
                    (outcome, _) => self.record(outcome),
                }
            }
            for rule in &pack.embeds {
                if !rule.callee.matches(callee) {
                    continue;
                }
                let site: &Shape =
                    lazy.get_or_insert_with(|| Shape::of_call(callee, it.arguments.iter()));
                if rule.text.is_none() && site.arg(NonZeroU32::MIN).is_none() {
                    continue;
                }
                match rule.apply(&pack.name, site, line, callee) {
                    Ok(embedded) => self.out.embedded.push(embedded),
                    Err(blind) => self.out.unresolved.push(blind),
                }
            }
        }
        if let Some(host) = callee.strip_suffix(".use")
            && self.makers.get(host).is_some_and(|maker| maker.serves)
        {
            self.mount(host, it, line);
        }
        self.trpc_router(it, callee);
    }

    /// `host.use([prefix], router…)`. A prefix may be an array of paths: the routers are mounted once under each literal element, and an element that is not a literal is a site the file could not read.
    fn mount(&mut self, host: &str, call: &CallExpression<'_>, line: Line) {
        let mut prefixes = vec![String::new()];
        let mut targets = Vec::new();
        let mut computed = false;
        for (position, argument) in call
            .arguments
            .iter()
            .filter_map(Argument::as_expression)
            .enumerate()
        {
            if position == 0
                && let Expression::ArrayExpression(array) = argument.get_inner_expression()
            {
                let paths: Vec<Option<String>> = array
                    .elements
                    .iter()
                    .map(|element| element.as_expression().and_then(text_of))
                    .collect();
                if paths.iter().any(Option::is_some) {
                    computed = paths.iter().any(Option::is_none);
                    prefixes = paths.into_iter().flatten().collect();
                } else {
                    // `app.use([a, b])` lists handlers, not paths.
                    targets.extend(
                        array
                            .elements
                            .iter()
                            .filter_map(|element| element.as_expression().and_then(mount_target)),
                    );
                }
            } else if position == 0
                && let Some(text) = text_of(argument)
            {
                prefixes = vec![text];
            } else if let Some(target) = mount_target(argument) {
                targets.push(target);
            }
        }
        if targets.is_empty() {
            return;
        }
        if computed || prefixes.iter().any(|prefix| prefix.contains("${}")) {
            let detail = self.snippet(call.span);
            self.out.unresolved.push(UnresolvedSite {
                line,
                reason: UnresolvedReason::NonLiteral,
                detail,
            });
        }
        for prefix in prefixes {
            self.out.extra.mounts.push(Mount {
                host: host.to_owned(),
                prefix,
                targets: targets.clone(),
                line,
            });
        }
    }

    fn tagged(&mut self, it: &TaggedTemplateExpression<'_>, tag: &str) {
        let line = self.line(it.span.start);
        let packs = self.packs;
        let mut lazy: Option<Shape> = None;
        for pack in packs {
            for rule in &pack.embeds {
                if !rule.callee.matches(tag) {
                    continue;
                }
                let site: &Shape =
                    lazy.get_or_insert_with(|| Shape::of_tag(tag, template_text(&it.quasi, "")));
                match rule.apply(&pack.name, site, line, tag) {
                    Ok(embedded) => self.out.embedded.push(embedded),
                    Err(blind) => self.out.unresolved.push(blind),
                }
            }
        }
    }

    /// Applies the decorator rules to `decorators`, which sit on `member` (a method, a field or the class itself).
    fn decorators(&mut self, decorators: &[Decorator<'_>], member: Option<&str>) {
        let packs = self.packs;
        for decorator in decorators {
            let Some((name, mut site)) = decorator_shape(decorator) else {
                continue;
            };
            site.member = member.map(str::to_owned);
            let line = self.line(decorator.span.start);
            for pack in packs {
                for rule in &pack.decorators {
                    if !rule.decorator.matches(&name) || rule.receiver.is_some() {
                        continue;
                    }
                    let prefix = match &rule.class {
                        None => None,
                        Some(class) => match self.class_prefix(class) {
                            Some(prefix) => Some(prefix),
                            None => continue,
                        },
                    };
                    let outcome = rule.apply(&pack.name, &site, prefix.as_deref(), line, &name);
                    self.record(outcome);
                }
            }
        }
    }

    /// The prefix the innermost class gives through the decorator `rule` names; `None` when it carries none.
    fn class_prefix(&self, rule: &Prefix) -> Option<String> {
        let frame = self.classes.last()?;
        let (_, shape) = frame
            .decorators
            .iter()
            .find(|(name, _)| rule.from.matches(name))?;
        Some(rule.read(shape))
    }

    /// The assign rules for `target = value`; a class field counts when the class extends one the rule names.
    fn assignment(&mut self, target: &str, value: &Expression<'_>, offset: u32, in_class: bool) {
        let heritage = if in_class {
            self.classes.last().and_then(|frame| frame.heritage.clone())
        } else {
            None
        };
        let line = self.line(offset);
        let site = Shape::of_value(text_of(value));
        let packs = self.packs;
        for pack in packs {
            for rule in &pack.assigns {
                if !rule.target.matches(target) {
                    continue;
                }
                let allowed = match &rule.class {
                    None => true,
                    Some(bases) => heritage.as_deref().is_some_and(|base| bases.matches(base)),
                };
                if allowed {
                    let outcome = rule.apply(&pack.name, &site, line, target);
                    self.record(outcome);
                }
            }
        }
    }

    /// `module.exports = x` and `exports.name = x`. A file that does either lists no exports in its syntax, so they are unknown.
    fn commonjs_export(&mut self, target: &str, value: &Expression<'_>) {
        if target == "module.exports" {
            self.exports_unknown = true;
            if let Expression::ObjectExpression(object) = value.get_inner_expression() {
                for property in &object.properties {
                    if let ObjectPropertyKind::ObjectProperty(property) = property
                        && !property.computed
                        && let Some(name) = property_name(&property.key)
                    {
                        self.export_value(&name, &property.value);
                    }
                }
            } else {
                self.export_value("default", value);
            }
        } else if let Some(name) = target
            .strip_prefix("exports.")
            .or_else(|| target.strip_prefix("module.exports."))
        {
            self.exports_unknown = true;
            self.export_value(name, value);
        }
    }

    fn export_value(&mut self, name: &str, value: &Expression<'_>) {
        if let Expression::Identifier(local) = value.get_inner_expression() {
            self.export(name, ExportTarget::Local(local.name.as_str().to_owned()));
        }
    }

    /// Walks what is computed inside a chain of member accesses, `a[b()].c[d]`, and the types written on the way down, `(a as T).c`, and nothing else of it. The chain itself goes through the wrappers, as `chain_of` reads it.
    fn walk_spine(&mut self, expr: &Expression<'a>) {
        match expr {
            Expression::StaticMemberExpression(member) => self.walk_spine(&member.object),
            Expression::ComputedMemberExpression(member) => {
                self.visit_expression(&member.expression);
                self.walk_spine(&member.object);
            }
            Expression::ParenthesizedExpression(inner) => self.walk_spine(&inner.expression),
            Expression::TSNonNullExpression(inner) => self.walk_spine(&inner.expression),
            Expression::TSAsExpression(inner) => {
                self.visit_ts_type(&inner.type_annotation);
                self.walk_spine(&inner.expression);
            }
            Expression::TSSatisfiesExpression(inner) => {
                self.visit_ts_type(&inner.type_annotation);
                self.walk_spine(&inner.expression);
            }
            Expression::TSTypeAssertion(inner) => {
                self.visit_ts_type(&inner.type_annotation);
                self.walk_spine(&inner.expression);
            }
            Expression::TSInstantiationExpression(inner) => {
                self.visit_ts_type_parameter_instantiation(&inner.type_arguments);
                self.walk_spine(&inner.expression);
            }
            _ => {}
        }
    }

    fn finish(self) -> Analysis {
        let Self {
            makers,
            symbols,
            mut out,
            exports_unknown,
            ..
        } = self;
        out.extra.hosts = makers
            .into_iter()
            .filter(|(_, maker)| maker.serves)
            .map(|(name, _)| name)
            .collect();
        out.contracts.sort_by(|a, b| order(a).cmp(&order(b)));
        out.contracts.dedup_by(|a, b| order(a) == order(b));
        out.extra
            .routes
            .sort_by(|a, b| (&a.host, order(&a.fact)).cmp(&(&b.host, order(&b.fact))));
        out.extra
            .routes
            .dedup_by(|a, b| a.host == b.host && order(&a.fact) == order(&b.fact));
        out.unresolved
            .sort_by(|a, b| (a.line, &a.detail).cmp(&(b.line, &b.detail)));
        out.unresolved
            .dedup_by(|a, b| a.line == b.line && a.detail == b.detail);
        out.exports = (!exports_unknown).then(|| exported_names(&out.extra.exports));
        out.symbols = Some(symbols.finish(&out.extra.exports));
        out
    }
}

/// The names `exports` give, in byte order and once each. `*` is what an `export *` brings, which is no name of the file.
fn exported_names(exports: &[Export]) -> Vec<String> {
    let mut names: Vec<String> = exports
        .iter()
        .filter(|export| export.name != "*")
        .map(|export| export.name.clone())
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// The order facts are emitted in.
fn order(fact: &ContractFact) -> (Line, &str, &str, &str) {
    (
        fact.line,
        fact.key.kind().as_str(),
        fact.key.key(),
        fact.role.as_str(),
    )
}

impl<'a> Visit<'a> for Analyzer<'_, 'a> {
    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        let Some(spec) = it.source.value.as_str() else {
            return;
        };
        let line = self.line(it.span.start);
        let kind = if it.specifiers.is_none() {
            ImportKind::SideEffect
        } else {
            ImportKind::Static
        };
        let mut import = Import::new(spec, kind, line);
        let mut all_type = true;
        for specifier in it.specifiers.iter().flatten() {
            let (local, name, is_type) = match specifier {
                ImportDeclarationSpecifier::ImportSpecifier(item) => (
                    item.local.name.as_str(),
                    export_name(&item.imported),
                    matches!(item.import_kind, ImportOrExportKind::Type),
                ),
                ImportDeclarationSpecifier::ImportDefaultSpecifier(item) => {
                    (item.local.name.as_str(), "default".to_owned(), false)
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(item) => {
                    (item.local.name.as_str(), "*".to_owned(), false)
                }
            };
            all_type &= is_type;
            self.bind(local, spec, &name);
            if is_member(&name) {
                self.import_rules(spec, &name, line);
            }
            import.names.push(name);
            import.exported.push(local.to_owned());
        }
        if matches!(it.import_kind, ImportOrExportKind::Type)
            || (all_type && !import.names.is_empty())
        {
            import.flags = import.flags | ImportFlags::TYPE_ONLY;
        }
        self.out.imports.push(import);
    }

    fn visit_export_declaration(&mut self, it: &ExportDeclaration<'a>) {
        for name in declared_names(&it.declaration) {
            self.export(&name, ExportTarget::Local(name.clone()));
        }
        walk::walk_export_declaration(self, it);
    }

    fn visit_export_named_declaration(&mut self, it: &ExportNamedDeclaration<'a>) {
        for specifier in &it.specifiers {
            let target = ExportTarget::Local(export_name(&specifier.local));
            self.export(&export_name(&specifier.exported), target);
        }
    }

    fn visit_export_from_declaration(&mut self, it: &ExportFromDeclaration<'a>) {
        let Some(spec) = it.source.value.as_str() else {
            return;
        };
        let line = self.line(it.span.start);
        let mut import = Import::new(spec, ImportKind::ReExport, line);
        for specifier in &it.specifiers {
            let name = export_name(&specifier.local);
            let exported = export_name(&specifier.exported);
            let target = ExportTarget::From {
                spec: spec.to_owned(),
                name: name.clone(),
            };
            self.export(&exported, target);
            if is_member(&name) {
                self.import_rules(spec, &name, line);
            }
            import.names.push(name);
            import.exported.push(exported);
        }
        if matches!(it.export_kind, ImportOrExportKind::Type) {
            import.flags = import.flags | ImportFlags::TYPE_ONLY;
        }
        self.out.imports.push(import);
    }

    fn visit_export_all_declaration(&mut self, it: &ExportAllDeclaration<'a>) {
        let Some(spec) = it.source.value.as_str() else {
            return;
        };
        let line = self.line(it.span.start);
        let namespace = it.exported.as_ref().map(export_name);
        // `export * as ns from 'x'` re-exports the namespace under one name, which is not `export *`.
        let mut import = if let Some(namespace) = &namespace {
            let mut import = Import::new(spec, ImportKind::ReExport, line);
            import.names.push("*".to_owned());
            import.exported.push(namespace.clone());
            import
        } else {
            Import::new(spec, ImportKind::ReExportAll, line)
        };
        self.export(
            namespace.as_deref().unwrap_or("*"),
            ExportTarget::From {
                spec: spec.to_owned(),
                name: "*".to_owned(),
            },
        );
        if matches!(it.export_kind, ImportOrExportKind::Type) {
            import.flags = import.flags | ImportFlags::TYPE_ONLY;
        }
        self.out.imports.push(import);
    }

    fn visit_export_default_declaration(&mut self, it: &ExportDefaultDeclaration<'a>) {
        let target = match &it.declaration {
            ExportDefaultDeclarationKind::Identifier(local) => local.name.as_str().to_owned(),
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => function
                .id
                .as_ref()
                .map_or_else(|| "default".to_owned(), |id| id.name.as_str().to_owned()),
            ExportDefaultDeclarationKind::ClassDeclaration(class) => class
                .id
                .as_ref()
                .map_or_else(|| "default".to_owned(), |id| id.name.as_str().to_owned()),
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                interface.id.name.as_str().to_owned()
            }
            other => {
                if let Some(Expression::CallExpression(call)) =
                    other.as_expression().map(Expression::get_inner_expression)
                {
                    self.names.insert(call.span.start, "default".to_owned());
                }
                "default".to_owned()
            }
        };
        self.export("default", ExportTarget::Local(target));
        walk::walk_export_default_declaration(self, it);
    }

    fn visit_ts_export_assignment(&mut self, it: &TSExportAssignment<'a>) {
        self.exports_unknown = true;
        self.export_value("default", &it.expression);
        walk::walk_ts_export_assignment(self, it);
    }

    fn visit_ts_module_block(&mut self, it: &TSModuleBlock<'a>) {
        self.module_depth = self.module_depth.saturating_add(1);
        walk::walk_ts_module_block(self, it);
        self.module_depth = self.module_depth.saturating_sub(1);
    }

    fn visit_ts_import_equals_declaration(&mut self, it: &TSImportEqualsDeclaration<'a>) {
        // `import A = B.C` names something else of the file, which `A` then uses.
        if !matches!(
            it.module_reference,
            TSModuleReference::ExternalModuleReference(_)
        ) {
            walk::walk_ts_import_equals_declaration(self, it);
        }
        if let TSModuleReference::ExternalModuleReference(reference) = &it.module_reference
            && let Some(spec) = reference.expression.value.as_str()
        {
            let mut import = Import::new(spec, ImportKind::Require, self.line(it.span.start));
            if matches!(it.import_kind, ImportOrExportKind::Type) {
                import.flags = import.flags | ImportFlags::TYPE_ONLY;
            }
            self.bind(it.id.name.as_str(), spec, "default");
            import.names.push("default".to_owned());
            self.out.imports.push(import);
        }
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        self.load(
            it.span,
            Some(&it.source),
            ImportKind::Dynamic,
            Taking::Nothing,
        );
        walk::walk_import_expression(self, it);
    }

    /// `typeof import('x')` and `import('x').T` in a type bind nothing and run nothing, but the file depends on `x`. Without a qualifier the type is the module itself, which is the name `*` of a namespace import. The name it takes from `x` reaches the import rules as the name of `import { T } from 'x'` does.
    fn visit_ts_import_type(&mut self, it: &TSImportType<'a>) {
        if let Some(spec) = it.source.value.as_str() {
            let line = self.line(it.source.span.start);
            let mut import = Import::new(spec, ImportKind::Static, line);
            import.flags = import.flags | ImportFlags::TYPE_ONLY;
            let name = it.qualifier.as_ref().map_or("*", first_name);
            if is_member(name) {
                self.import_rules(spec, name, line);
            }
            import.names.push(name.to_owned());
            self.out.imports.push(import);
        }
        walk::walk_ts_import_type(self, it);
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let Some(init) = &it.init {
            let init = peel(init);
            if let (BindingPattern::BindingIdentifier(id), Expression::CallExpression(call)) =
                (&it.id, init)
            {
                self.names
                    .insert(call.span.start, id.name.as_str().to_owned());
            }
            self.load_declared(init, &it.id);
            if self.has_trpc
                && let (BindingPattern::BindingIdentifier(id), Expression::CallExpression(call)) =
                    (&it.id, init)
                && maps::is_procedure(call)
            {
                self.out.extra.procedures.push(id.name.as_str().to_owned());
            }
            self.destructured_chain(&it.id, init, it.span.start);
            if self.has_graphql
                && let BindingPattern::BindingIdentifier(id) = &it.id
                && is_resolvers_name(id.name.as_str())
                && let Expression::ObjectExpression(object) = init
            {
                self.resolver_map(object);
            }
        }
        let outer = self.symbols.declarator(it.span.start);
        walk::walk_variable_declarator(self, it);
        self.symbols.restore(outer);
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        if self.has_graphql
            && !it.computed
            && property_str(&it.key) == Some("resolvers")
            && let Expression::ObjectExpression(object) = it.value.get_inner_expression()
        {
            self.resolver_map(object);
        }
        walk::walk_object_property(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if let AssignmentTarget::StaticMemberExpression(member) = &it.left
            && let Some(chain) = static_chain(&member.object, member.property.name.as_str())
            && let Some(target) = dotted(&chain)
        {
            self.commonjs_export(&target, &it.right);
            self.assignment(&target, &it.right, it.span.start, false);
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        let argument = it.arguments.first().and_then(Argument::as_expression);
        if is_require(it) {
            self.load(it.span, argument, ImportKind::Require, Taking::Nothing);
        } else if let Some(names) = chain_of(&it.callee) {
            self.symbols.call_write(&names);
            if let Some(callee) = dotted(&names) {
                // A module named at run time is neither an import nor a blind spot, so only a literal one goes to `load`.
                if let Some(call) = module_call(&callee)
                    && argument.and_then(literal_text).is_some()
                {
                    self.load(it.span, argument, call.kind(), Taking::Nothing);
                }
                self.call(it, &callee);
            }
        }
        let outer = self.symbols.call(&it.callee);
        walk::walk_call_expression(self, it);
        self.symbols.restore(outer);
    }

    fn visit_tagged_template_expression(&mut self, it: &TaggedTemplateExpression<'a>) {
        if let Some(tag) = chain_of(&it.tag).and_then(|names| dotted(&names)) {
            self.tagged(it, &tag);
        }
        let outer = self.symbols.run();
        walk::walk_tagged_template_expression(self, it);
        self.symbols.restore(outer);
    }

    fn visit_member_expression(&mut self, it: &MemberExpression<'a>) {
        let Some(names) = member_chain(it) else {
            match it {
                MemberExpression::PrivateFieldExpression(member) => {
                    self.symbols.private(member.field.name.as_str(), None);
                }
                MemberExpression::StaticMemberExpression(member) => {
                    let property = member.property.name.as_str();
                    self.member_of_load(&member.object, property);
                    // `this.#field.member` reads a member of the type of the field, so the field is not read as a value.
                    if let Expression::PrivateFieldExpression(field) =
                        member.object.get_inner_expression()
                    {
                        self.symbols
                            .private(field.field.name.as_str(), Some(property));
                        self.visit_expression(&field.object);
                        return;
                    }
                }
                MemberExpression::ComputedMemberExpression(_) => {}
            }
            walk::walk_member_expression(self, it);
            return;
        };
        self.chain_rules(&names, it.span().start);
        self.symbols.chain(&names);
        // The links below this one are parts of the same chain: only what is computed inside it is still to walk.
        match it {
            MemberExpression::StaticMemberExpression(member) => self.walk_spine(&member.object),
            MemberExpression::ComputedMemberExpression(member) => {
                self.visit_expression(&member.expression);
                self.walk_spine(&member.object);
            }
            MemberExpression::PrivateFieldExpression(_) => {}
        }
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        let frame = ClassFrame {
            decorators: it.decorators.iter().filter_map(decorator_shape).collect(),
            heritage: it
                .heritage
                .as_ref()
                .and_then(|heritage| dotted_name(&heritage.expression))
                .map(Cow::into_owned),
        };
        let name = it.id.as_ref().map(|id| id.name.as_str().to_owned());
        self.classes.push(frame);
        self.decorators(&it.decorators, name.as_deref());
        let outer = self.symbols.class_at(it.span.start);
        walk::walk_class(self, it);
        self.symbols.restore(outer);
        self.classes.pop();
    }

    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        let member = if it.computed {
            None
        } else {
            property_name(&it.key)
        };
        self.decorators(&it.decorators, member.as_deref());
        walk::walk_method_definition(self, it);
    }

    fn visit_property_definition(&mut self, it: &PropertyDefinition<'a>) {
        let member = field_name(it);
        self.decorators(&it.decorators, member);
        if let (Some(name), Some(value)) = (member, &it.value) {
            self.assignment(name, value, it.span.start, true);
        }
        if it.r#static {
            walk::walk_property_definition(self, it);
            return;
        }
        // The value of an instance field runs when the class is constructed, the rest when it is defined.
        self.visit_decorators(&it.decorators);
        self.visit_property_key(&it.key);
        if let Some(type_annotation) = &it.type_annotation {
            self.visit_ts_type_annotation(type_annotation);
        }
        if let Some(value) = &it.value {
            let outer = self.symbols.later();
            self.visit_expression(value);
            self.symbols.restore(outer);
        }
    }

    fn visit_accessor_property(&mut self, it: &AccessorProperty<'a>) {
        let member = if it.computed {
            None
        } else {
            property_name(&it.key)
        };
        self.decorators(&it.decorators, member.as_deref());
        if it.r#static {
            walk::walk_accessor_property(self, it);
            return;
        }
        self.visit_decorators(&it.decorators);
        self.visit_property_key(&it.key);
        if let Some(type_annotation) = &it.type_annotation {
            self.visit_ts_type_annotation(type_annotation);
        }
        if let Some(value) = &it.value {
            let outer = self.symbols.later();
            self.visit_expression(value);
            self.symbols.restore(outer);
        }
    }

    fn visit_program(&mut self, it: &Program<'a>) {
        for (position, statement) in it.body.iter().enumerate() {
            self.symbols.statement(position);
            self.visit_statement(statement);
        }
    }

    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.symbols.reference(it.name.as_str());
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        let outer = self.symbols.function();
        walk::walk_function(self, it, flags);
        self.symbols.restore(outer);
    }

    fn visit_formal_parameter(&mut self, it: &FormalParameter<'a>) {
        let outer = self.symbols.parameter();
        self.visit_decorators(&it.decorators);
        self.symbols.restore(outer);
        self.visit_binding_pattern(&it.pattern);
        if let Some(type_annotation) = &it.type_annotation {
            self.visit_ts_type_annotation(type_annotation);
        }
        if let Some(initializer) = &it.initializer {
            self.visit_expression(initializer);
        }
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        let outer = self.symbols.function();
        walk::walk_arrow_function_expression(self, it);
        self.symbols.restore(outer);
    }

    fn visit_decorator(&mut self, it: &Decorator<'a>) {
        let outer = self.symbols.run();
        walk::walk_decorator(self, it);
        self.symbols.restore(outer);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        let outer = self.symbols.run();
        walk::walk_new_expression(self, it);
        self.symbols.restore(outer);
    }

    fn visit_class_body(&mut self, it: &ClassBody<'a>) {
        for (position, element) in it.body.iter().enumerate() {
            let outer = self.symbols.element(position);
            self.visit_class_element(element);
            self.symbols.restore(outer);
        }
    }

    fn visit_simple_assignment_target(&mut self, it: &SimpleAssignmentTarget<'a>) {
        if let Some(base) = symbols::target_base(it) {
            self.symbols.write(base);
        }
        walk::walk_simple_assignment_target(self, it);
    }

    fn visit_assignment_target_property_identifier(
        &mut self,
        it: &AssignmentTargetPropertyIdentifier<'a>,
    ) {
        self.symbols.write(it.binding.name.as_str());
        walk::walk_assignment_target_property_identifier(self, it);
    }

    fn visit_unary_expression(&mut self, it: &UnaryExpression<'a>) {
        if it.operator == UnaryOperator::Delete
            && let Some(base) = symbols::base_name(&it.argument)
        {
            self.symbols.write(base);
        }
        walk::walk_unary_expression(self, it);
    }

    fn visit_private_in_expression(&mut self, it: &PrivateInExpression<'a>) {
        self.symbols.private(it.left.name.as_str(), None);
        walk::walk_private_in_expression(self, it);
    }
}

/// The expression with parentheses, type assertions and `await` taken off.
fn peel<'e, 'a>(expr: &'e Expression<'a>) -> &'e Expression<'a> {
    match expr.get_inner_expression() {
        Expression::AwaitExpression(awaited) => peel(&awaited.argument),
        inner => inner,
    }
}

fn is_require(call: &CallExpression<'_>) -> bool {
    matches!(&call.callee, Expression::Identifier(name) if name.name.as_str() == "require")
}

/// How a call of Vitest or Jest takes the module in its first argument.
enum ModuleCall {
    /// It mocks the module, stops mocking it, or sets or makes a mock for it.
    Mock,
    /// It loads the module as it is: `jest.requireActual` hands the module back and `vi.importActual` a promise of it.
    Actual { promise: bool },
}

impl ModuleCall {
    fn kind(&self) -> ImportKind {
        match self {
            Self::Mock => ImportKind::Mock,
            Self::Actual { .. } => ImportKind::Dynamic,
        }
    }
}

/// How the call named `callee` takes the module in its first argument, or `None` for any other call.
fn module_call(callee: &str) -> Option<ModuleCall> {
    match callee {
        "jest.requireActual" => Some(ModuleCall::Actual { promise: false }),
        "vi.importActual" => Some(ModuleCall::Actual { promise: true }),
        "vi.mock"
        | "vi.doMock"
        | "vi.unmock"
        | "vi.doUnmock"
        | "vi.importMock"
        | "jest.mock"
        | "jest.doMock"
        | "jest.unmock"
        | "jest.dontMock"
        | "jest.requireMock"
        | "jest.setMock"
        | "jest.createMockFromModule"
        | "jest.genMockFromModule"
        | "jest.unstable_mockModule"
        | "jest.deepUnmock" => Some(ModuleCall::Mock),
        _ => None,
    }
}

/// An expression that loads a module as it is: `require('x')` and `jest.requireActual('x')` hand the module back, `import('x')` and `vi.importActual('x')` a promise of it.
struct Load<'e, 'a> {
    span: Span,
    argument: Option<&'e Expression<'a>>,
    kind: ImportKind,
    promise: bool,
}

/// The load that `expr` is, with parentheses and type assertions taken off. A module that `jest.requireActual` or `vi.importActual` names at run time is neither an import nor a blind spot, so such a call is no load, while `require(name)` and `import(name)` are loads that `Analyzer::load` records as unread.
fn load_of<'e, 'a>(expr: &'e Expression<'a>) -> Option<Load<'e, 'a>> {
    match expr.get_inner_expression() {
        Expression::ImportExpression(import) => Some(Load {
            span: import.span,
            argument: Some(&import.source),
            kind: ImportKind::Dynamic,
            promise: true,
        }),
        Expression::CallExpression(call) => {
            let argument = call.arguments.first().and_then(Argument::as_expression);
            let (kind, promise) = if is_require(call) {
                (ImportKind::Require, false)
            } else {
                let callee = dotted(&chain_of(&call.callee)?)?;
                let ModuleCall::Actual { promise } = module_call(&callee)? else {
                    return None;
                };
                argument.and_then(literal_text)?;
                (ImportKind::Dynamic, promise)
            };
            Some(Load {
                span: call.span,
                argument,
                kind,
                promise,
            })
        }
        _ => None,
    }
}

fn is_resolvers_name(name: &str) -> bool {
    name == "resolvers" || name.ends_with("Resolvers")
}

/// The first name of the qualifier of an import type: `A` for `import('x').A.B`.
fn first_name<'q>(qualifier: &'q TSImportTypeQualifier<'_>) -> &'q str {
    match qualifier {
        TSImportTypeQualifier::Identifier(name) => name.name.as_str(),
        TSImportTypeQualifier::QualifiedName(qualified) => first_name(&qualified.left),
    }
}

/// Whether an imported or re-exported `name` is a member of the module, not its default export or its namespace.
fn is_member(name: &str) -> bool {
    name != "default" && name != "*"
}

fn export_name(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(name) => name.name.as_str().to_owned(),
        ModuleExportName::IdentifierReference(name) => name.name.as_str().to_owned(),
        ModuleExportName::StringLiteral(text) => {
            text.value.as_str().map(str::to_owned).unwrap_or_default()
        }
    }
}

/// The names an `export` declaration declares: `export const a = 1, { b, c: d } = e`, `export function f() {}`, `export class C {}`, `export enum E {}`, `export type T = string`, `export interface I {}`, `export namespace N {}` and `export import A = B.C`.
fn declared_names(declaration: &Declaration<'_>) -> Vec<String> {
    let name = |id: &BindingIdentifier<'_>| id.name.as_str().to_owned();
    match declaration {
        Declaration::VariableDeclaration(variables) => variables
            .declarations
            .iter()
            .flat_map(|declarator| declarator.id.get_binding_identifiers())
            .map(name)
            .collect(),
        Declaration::FunctionDeclaration(function) => function.id.iter().map(name).collect(),
        Declaration::ClassDeclaration(class) => class.id.iter().map(name).collect(),
        Declaration::TSTypeAliasDeclaration(alias) => vec![name(&alias.id)],
        Declaration::TSInterfaceDeclaration(interface) => vec![name(&interface.id)],
        Declaration::TSEnumDeclaration(declared) => vec![name(&declared.id)],
        Declaration::TSNamespaceDeclaration(namespace) => vec![name(&namespace.id)],
        Declaration::TSImportEqualsDeclaration(import) => vec![name(&import.id)],
        // `declare module 'x' {}` and `declare global {}` declare no name of the file.
        Declaration::TSExternalModuleDeclaration(_) | Declaration::TSGlobalDeclaration(_) => {
            Vec::new()
        }
    }
}

/// The name a decorator is applied by and the arguments it is given.
fn decorator_shape(decorator: &Decorator<'_>) -> Option<(String, Shape)> {
    match decorator.expression.get_inner_expression() {
        Expression::CallExpression(call) => {
            let name = dotted(&chain_of(&call.callee)?)?;
            let shape = Shape::of_call(&name, call.arguments.iter());
            Some((name, shape))
        }
        other => {
            let name = dotted(&chain_of(other)?)?;
            let shape = Shape::of_call(&name, std::iter::empty::<&Argument<'_>>());
            Some((name, shape))
        }
    }
}

/// What a router is mounted from: a name, or a `require` written in place.
fn mount_target(expr: &Expression<'_>) -> Option<MountTarget> {
    match expr.get_inner_expression() {
        Expression::CallExpression(call) if is_require(call) => call
            .arguments
            .first()
            .and_then(Argument::as_expression)
            .and_then(literal_text)
            .map(MountTarget::Module),
        other => chain_of(other)
            .and_then(|names| dotted(&names))
            .map(MountTarget::Local),
    }
}
