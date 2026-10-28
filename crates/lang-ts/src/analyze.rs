//! One syntax tree read for imports, exports, contracts, embedded documents and what the link phase needs.

mod makers;
mod maps;

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::fact::{
    ContractFact, Embedded, Import, ImportFlags, ImportKind, Line, UnresolvedReason, UnresolvedSite,
};
use gsttt_core::rules::{Outcome, Pack, Prefix, Site};
use gsttt_core::text::LineIndex;
use oxc_ast::ast::{
    AccessorProperty, Argument, AssignmentExpression, AssignmentTarget, BindingPattern,
    CallExpression, Class, Declaration, Decorator, ExportAllDeclaration, ExportDeclaration,
    ExportDefaultDeclaration, ExportDefaultDeclarationKind, ExportFromDeclaration,
    ExportNamedDeclaration, Expression, ImportDeclaration, ImportDeclarationSpecifier,
    ImportExpression, ImportOrExportKind, MemberExpression, MethodDefinition, ModuleExportName,
    ObjectProperty, ObjectPropertyKind, Program, PropertyDefinition, TSExportAssignment,
    TSImportEqualsDeclaration, TSImportType, TSImportTypeQualifier, TSModuleReference,
    TaggedTemplateExpression, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::{GetSpan, Span};

use self::makers::Maker;
use crate::extra::{Binding, Export, ExportTarget, Mount, MountTarget, Route, TsExtra};
use crate::shape::{
    Shape, chain_of, dotted, literal_text, member_chain, property_name, static_chain,
    template_text, text_of,
};

/// Everything one file says on its own.
#[derive(Debug, Default)]
pub(crate) struct Analysis {
    pub(crate) imports: Vec<Import>,
    pub(crate) contracts: Vec<ContractFact>,
    pub(crate) embedded: Vec<Embedded>,
    pub(crate) unresolved: Vec<UnresolvedSite>,
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
        done: BTreeSet::new(),
        names: BTreeMap::new(),
        classes: Vec::new(),
        has_graphql: packs.iter().any(|pack| pack.name == "graphql"),
        has_trpc: packs.iter().any(|pack| pack.name == "trpc"),
        out: Analysis::default(),
    };
    analyzer.visit_program(program);
    analyzer.finish()
}

/// A class being walked: the decorators it carries, and the class it extends.
struct ClassFrame {
    decorators: Vec<(String, Shape)>,
    heritage: Option<String>,
}

struct Analyzer<'p> {
    text: &'p str,
    index: &'p LineIndex,
    packs: &'p [Pack],
    /// Variables made by a call in this file, found before the walk so that a route may come before its router.
    makers: BTreeMap<String, Maker>,
    /// Starts of the calls whose meaning another visit already took, so they are read once.
    done: BTreeSet<u32>,
    /// The name a call is stored under, `appRouter` for `const appRouter = router({…})`, by the start of the call.
    names: BTreeMap<u32, String>,
    classes: Vec<ClassFrame>,
    has_graphql: bool,
    has_trpc: bool,
    out: Analysis,
}

impl Analyzer<'_> {
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

    /// A literal `require` or `import()` becomes an import, with the names the declaration takes from it; one that is computed is a blind spot. Each call is read once.
    fn load(
        &mut self,
        span: Span,
        argument: Option<&Expression<'_>>,
        kind: ImportKind,
        pattern: Option<&BindingPattern<'_>>,
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
        match pattern {
            Some(BindingPattern::BindingIdentifier(local)) => {
                self.bind(local.name.as_str(), &spec, namespace);
                import.names.push(namespace.to_owned());
            }
            Some(BindingPattern::ObjectPattern(object)) => {
                for property in &object.properties {
                    let name = if property.computed {
                        None
                    } else {
                        property_name(&property.key)
                    };
                    let Some(name) = name else { continue };
                    if let BindingPattern::BindingIdentifier(local) = &property.value {
                        self.bind(local.name.as_str(), &spec, &name);
                    }
                    import.names.push(name);
                }
            }
            _ => {}
        }
        self.out.imports.push(import);
    }

    /// `const x = require('y')` and `const { a } = await import('y')`.
    fn load_declared(&mut self, init: &Expression<'_>, pattern: &BindingPattern<'_>) {
        match init {
            Expression::CallExpression(call) if is_require(call) => {
                let argument = call.arguments.first().and_then(Argument::as_expression);
                self.load(call.span, argument, ImportKind::Require, Some(pattern));
            }
            Expression::ImportExpression(import) => {
                self.load(
                    import.span,
                    Some(&import.source),
                    ImportKind::Dynamic,
                    Some(pattern),
                );
            }
            _ => {}
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

    /// Applies the chain rules to every root the chain starts with. A rule that lists terminals needs one of them after the first segment, so `trpc.useUtils()` and `prisma.$transaction()` name nothing.
    fn chain_rules(&mut self, names: &[Option<String>], offset: u32) {
        let line = self.line(offset);
        let packs = self.packs;
        for pack in packs {
            for rule in &pack.chains {
                for count in 1..names.len() {
                    let Some((head, tail)) = names.split_at_checked(count) else {
                        break;
                    };
                    let Some(root) = dotted(head) else {
                        break;
                    };
                    if !rule.root.matches(&root) {
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

    /// `module.exports = x` and `exports.name = x`.
    fn commonjs_export(&mut self, target: &str, value: &Expression<'_>) {
        if target == "module.exports" {
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
            self.export_value(name, value);
        }
    }

    fn export_value(&mut self, name: &str, value: &Expression<'_>) {
        if let Expression::Identifier(local) = value.get_inner_expression() {
            self.export(name, ExportTarget::Local(local.name.as_str().to_owned()));
        }
    }

    /// Walks what is computed inside a chain of member accesses, `a[b()].c[d]`, and nothing else of it.
    fn walk_spine(&mut self, expr: &Expression<'_>) {
        match expr.get_inner_expression() {
            Expression::StaticMemberExpression(member) => self.walk_spine(&member.object),
            Expression::ComputedMemberExpression(member) => {
                self.visit_expression(&member.expression);
                self.walk_spine(&member.object);
            }
            _ => {}
        }
    }

    fn finish(self) -> Analysis {
        let Self {
            makers, mut out, ..
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
        out
    }
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

impl<'a> Visit<'a> for Analyzer<'_> {
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
            import.names.push(name);
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
        let mut import = Import::new(spec, ImportKind::ReExport, self.line(it.span.start));
        for specifier in &it.specifiers {
            let name = export_name(&specifier.local);
            let target = ExportTarget::From {
                spec: spec.to_owned(),
                name: name.clone(),
            };
            self.export(&export_name(&specifier.exported), target);
            import.names.push(name);
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
        // `export * as ns from 'x'` re-exports the namespace under one name, which is not `export *`.
        let mut import = if it.exported.is_some() {
            let mut import = Import::new(spec, ImportKind::ReExport, line);
            import.names.push("*".to_owned());
            import
        } else {
            Import::new(spec, ImportKind::ReExportAll, line)
        };
        let exported = it
            .exported
            .as_ref()
            .map_or_else(|| "*".to_owned(), export_name);
        self.export(
            &exported,
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
        self.export_value("default", &it.expression);
        walk::walk_ts_export_assignment(self, it);
    }

    fn visit_ts_import_equals_declaration(&mut self, it: &TSImportEqualsDeclaration<'a>) {
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
        self.load(it.span, Some(&it.source), ImportKind::Dynamic, None);
        walk::walk_import_expression(self, it);
    }

    /// `typeof import('x')` and `import('x').T` in a type bind nothing and run nothing, but the file depends on `x`.
    fn visit_ts_import_type(&mut self, it: &TSImportType<'a>) {
        if let Some(spec) = it.source.value.as_str() {
            let mut import = Import::new(spec, ImportKind::Static, self.line(it.source.span.start));
            import.flags = import.flags | ImportFlags::TYPE_ONLY;
            if let Some(qualifier) = &it.qualifier {
                import.names.push(first_name(qualifier).to_owned());
            }
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
        walk::walk_variable_declarator(self, it);
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        if self.has_graphql
            && !it.computed
            && property_name(&it.key).as_deref() == Some("resolvers")
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
            self.load(it.span, argument, ImportKind::Require, None);
        } else if let Some(callee) = chain_of(&it.callee).and_then(|names| dotted(&names)) {
            // A module named at run time is neither an import nor a blind spot, so only a literal one goes to `load`.
            if is_module_call(&callee) && argument.and_then(literal_text).is_some() {
                self.load(it.span, argument, ImportKind::Dynamic, None);
            }
            self.call(it, &callee);
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_tagged_template_expression(&mut self, it: &TaggedTemplateExpression<'a>) {
        if let Some(tag) = chain_of(&it.tag).and_then(|names| dotted(&names)) {
            self.tagged(it, &tag);
        }
        walk::walk_tagged_template_expression(self, it);
    }

    fn visit_member_expression(&mut self, it: &MemberExpression<'a>) {
        let Some(names) = member_chain(it) else {
            walk::walk_member_expression(self, it);
            return;
        };
        self.chain_rules(&names, it.span().start);
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
                .and_then(|heritage| chain_of(&heritage.expression))
                .and_then(|names| dotted(&names)),
        };
        let name = it.id.as_ref().map(|id| id.name.as_str().to_owned());
        self.classes.push(frame);
        self.decorators(&it.decorators, name.as_deref());
        walk::walk_class(self, it);
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
        let member = if it.computed {
            None
        } else {
            property_name(&it.key)
        };
        self.decorators(&it.decorators, member.as_deref());
        if let (Some(name), Some(value)) = (&member, &it.value) {
            self.assignment(name, value, it.span.start, true);
        }
        walk::walk_property_definition(self, it);
    }

    fn visit_accessor_property(&mut self, it: &AccessorProperty<'a>) {
        let member = if it.computed {
            None
        } else {
            property_name(&it.key)
        };
        self.decorators(&it.decorators, member.as_deref());
        walk::walk_accessor_property(self, it);
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

/// The calls of Vitest and Jest whose first argument is a module: the one to mock and the one to load as it is.
fn is_module_call(callee: &str) -> bool {
    matches!(
        callee,
        "vi.mock"
            | "vi.doMock"
            | "vi.unmock"
            | "vi.doUnmock"
            | "vi.importActual"
            | "vi.importMock"
            | "jest.mock"
            | "jest.doMock"
            | "jest.unmock"
            | "jest.dontMock"
            | "jest.requireActual"
            | "jest.requireMock"
    )
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

fn export_name(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(name) => name.name.as_str().to_owned(),
        ModuleExportName::IdentifierReference(name) => name.name.as_str().to_owned(),
        ModuleExportName::StringLiteral(text) => {
            text.value.as_str().map(str::to_owned).unwrap_or_default()
        }
    }
}

/// The names `export const a = 1, b = 2`, `export function f() {}` and `export class C {}` declare.
fn declared_names(declaration: &Declaration<'_>) -> Vec<String> {
    match declaration {
        Declaration::VariableDeclaration(variables) => variables
            .declarations
            .iter()
            .filter_map(|declarator| match &declarator.id {
                BindingPattern::BindingIdentifier(id) => Some(id.name.as_str().to_owned()),
                _ => None,
            })
            .collect(),
        Declaration::FunctionDeclaration(function) => function
            .id
            .iter()
            .map(|id| id.name.as_str().to_owned())
            .collect(),
        Declaration::ClassDeclaration(class) => class
            .id
            .iter()
            .map(|id| id.name.as_str().to_owned())
            .collect(),
        _ => Vec::new(),
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
