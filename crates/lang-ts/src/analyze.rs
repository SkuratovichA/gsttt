//! One syntax tree read for imports, exports, contracts, embedded documents and what the link phase needs.

mod constants;
mod makers;
mod maps;
mod roots;
mod symbols;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use gsttt_core::fact::{
    ContractFact, Embedded, Import, ImportFlags, ImportKind, Line, Symbols, UnresolvedReason,
    UnresolvedSite,
};
use gsttt_core::rules::{Access, CallRule, Outcome, Pack, Prefix, Reading, Site, serves_http};
use gsttt_core::text::LineIndex;
use oxc_ast::ast::{
    AccessorProperty, Argument, ArrowFunctionExpression, AssignmentExpression, AssignmentTarget,
    AssignmentTargetPropertyIdentifier, BindingIdentifier, BindingPattern, CallExpression, Class,
    ClassBody, Declaration, Decorator, ExportAllDeclaration, ExportDeclaration,
    ExportDefaultDeclaration, ExportDefaultDeclarationKind, ExportFromDeclaration,
    ExportNamedDeclaration, Expression, ExpressionStatement, FormalParameter, Function,
    IdentifierReference, ImportDeclaration, ImportDeclarationSpecifier, ImportExpression,
    ImportOrExportKind, MemberExpression, MethodDefinition, ModuleExportName, NewExpression,
    ObjectProperty, ObjectPropertyKind, PrivateInExpression, Program, PropertyDefinition,
    SimpleAssignmentTarget, TSExportAssignment, TSImportEqualsDeclaration, TSImportType,
    TSImportTypeQualifier, TSModuleBlock, TSModuleReference, TSType, TSTypeName,
    TaggedTemplateExpression, UnaryExpression, UpdateExpression, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::{GetSpan, Span};
use oxc_syntax::operator::{AssignmentOperator, UnaryOperator};
use oxc_syntax::scope::ScopeFlags;

pub(crate) use self::constants::Constants;
use self::makers::{Maker, NEST_FACTORY_CREATE};
use self::roots::Roots;
use self::symbols::Recorder;
use crate::builtins::is_builtin;
use crate::extra::{
    Binding, ClassRoute, Deferred, DeferredTarget, Export, ExportTarget, Inherit, Mount,
    MountTarget, Route, RuleRef, TsExtra,
};
use crate::pathing::{is_relative, split_package};
use crate::shape::{
    COMPUTED, Shape, chain_of, dotted, dotted_name, field_name, literal_text, member_chain,
    property_name, property_str, static_chain, template_text, text_with,
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
    let gates = Gates::of(packs);
    let constants = Constants::collect(program);
    let mut analyzer = Analyzer {
        text,
        index,
        packs,
        makers: makers::collect(program, text, packs, gates.nestjs, &constants),
        constants,
        roots: roots::collect(program, packs),
        symbols: Recorder::collect(program, index),
        done: BTreeSet::new(),
        names: BTreeMap::new(),
        classes: Vec::new(),
        gates,
        hosts: BTreeSet::new(),
        exports_unknown: false,
        module_depth: 0,
        imported: Vec::new(),
        access: None,
        discarded: None,
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

/// A class being walked: its name, the decorators it carries, and the class it extends.
struct ClassFrame {
    name: Option<String>,
    decorators: Vec<ClassDecorator>,
    heritage: Option<String>,
}

/// A decorator of a class: the name it is applied by, the arguments it is given, and the source it covers.
struct ClassDecorator {
    name: String,
    shape: Shape,
    span: Span,
}

/// What a class decorator gives as the prefix of the routes of its class.
enum ClassPrefix {
    /// The text of the prefix, empty when the decorator carries none.
    Text(String),
    /// The decorator holds a prefix that is not text here, so the path of no route of the class is known.
    Unread,
}

impl ClassPrefix {
    /// What `decorator` gives under the class prefix `rule` that names it.
    fn of(rule: &Prefix, decorator: &ClassDecorator) -> Self {
        match rule.reading(&decorator.shape).as_prefix() {
            Some(prefix) => Self::Text(prefix.to_owned()),
            None => Self::Unread,
        }
    }
}

/// The packs that switch on reading that no rule describes, by name, found once per file.
#[derive(Clone, Copy)]
struct Gates {
    graphql: bool,
    trpc: bool,
    nestjs: bool,
}

impl Gates {
    fn of(packs: &[Pack]) -> Self {
        let has = |name: &str| packs.iter().any(|pack| pack.name == name);
        Self {
            graphql: has("graphql"),
            trpc: has("trpc"),
            nestjs: has("nestjs"),
        }
    }
}

struct Analyzer<'p, 'a> {
    text: &'p str,
    index: &'p LineIndex,
    packs: &'p [Pack],
    /// Variables made by a call in this file, found before the walk so that a route may come before its router.
    makers: BTreeMap<String, Maker>,
    /// The constants of this file and the names it imports, found before the walk because a rule may read a constant that is declared below the site.
    constants: Constants,
    /// The aliases of `this` and the names declared with a root type, found before the walk because a class may use `self` above the method that binds it.
    roots: Roots<'p>,
    /// The table of the top-level bindings, found before the walk, and what the walk finds against it: the declarations, what each uses and what the file runs as it loads.
    symbols: Recorder<'a>,
    /// Starts of the calls whose meaning another visit already took, so they are read once.
    done: BTreeSet<u32>,
    /// The name a call is stored under, `appRouter` for `const appRouter = router({…})`, by the start of the call.
    names: BTreeMap<u32, String>,
    classes: Vec<ClassFrame>,
    gates: Gates,
    /// The classes that mounts can attach: `@Module` classes and the classes that serve routes. The variables that serve are added when the walk is over.
    hosts: BTreeSet<String>,
    /// Whether the file assigns `module.exports` or `exports`, or has `export =`, so that its syntax does not list what it exports.
    exports_unknown: bool,
    /// How many `namespace` and `declare module` bodies the walk is in, whose `export`s are theirs and not the file's.
    module_depth: u32,
    /// The names the file imports from a package an import rule reads, which name their contract once the file is known to use the local name as a value.
    imported: Vec<Imported>,
    /// How the next member expression the walk reaches is touched, set by what writes it just before the walk goes in and taken by that member expression first of all, so that a computed key or an argument inside it is read.
    access: Option<Access>,
    /// The expression that the last expression statement is made of, parentheses peeled: its value is dropped, so a compound assignment or an update that is exactly this expression only writes.
    discarded: Option<Span>,
    out: Analysis,
}

/// A name an import takes from a package that an import rule reads.
struct Imported {
    /// The place of the name in `extra.bindings`, which holds the package, the name it exports and the local name.
    binding: usize,
    /// The line of the import.
    line: Line,
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

    /// Records that `local` is `name` of the package `spec`, and gives the place of the binding in `extra.bindings`.
    fn bind(&mut self, local: &str, spec: &str, name: &str) -> usize {
        self.constants.import(local, spec);
        let place = self.out.extra.bindings.len();
        self.out.extra.bindings.push(Binding {
            local: local.to_owned(),
            spec: spec.to_owned(),
            name: name.to_owned(),
        });
        place
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
            self.unread(span, line, UnresolvedReason::NonLiteral);
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
            self.chain_rules(&names, offset, Access::Read);
        }
    }

    /// Applies the chain rules to every root the chain starts with, touched as `access` says: a root a rule lists, a name declared with one of its root types, or the alias of `this` that the chain starts with, read as `this`. A rule that lists terminals needs one of them after the first segment, so `trpc.useUtils()` and `prisma.$transaction()` name nothing.
    fn chain_rules(&mut self, names: &[Option<String>], offset: u32, access: Access) {
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
                    for outcome in rule.apply(&pack.name, access, &segments, line, &root) {
                        self.record(outcome);
                    }
                }
            }
        }
    }

    /// Applies the import rules to the names the file imports and uses as a value; a type, a re-export and a name the file never uses name nothing.
    fn import_rules(&mut self) {
        let outcomes: Vec<Outcome> = std::mem::take(&mut self.imported)
            .into_iter()
            .filter_map(|imported| {
                let binding = self.out.extra.bindings.get(imported.binding)?;
                self.symbols
                    .used_as_value(&binding.local)
                    .then_some((binding, imported.line))
            })
            .flat_map(|(binding, line)| {
                self.packs.iter().flat_map(move |pack| {
                    pack.imports
                        .iter()
                        .filter(|rule| rule.reads(&binding.spec))
                        .map(move |rule| rule.apply(&pack.name, &binding.name, line))
                })
            })
            .collect();
        for outcome in outcomes {
            self.record(outcome);
        }
    }

    /// Whether an import rule reads the names imported from the package `spec`.
    fn packs_read(&self, spec: &str) -> bool {
        self.packs
            .iter()
            .flat_map(|pack| &pack.imports)
            .any(|rule| rule.reads(spec))
    }

    /// What the call that made the receiver of `callee` gives as a prefix, and the receiver's name, when the receiver was made in this file by a call that `rule` names.
    fn receiver<'c>(&self, callee: &'c str, rule: &Prefix) -> Option<(Reading, &'c str)> {
        let (host, _) = callee.rsplit_once('.')?;
        let maker = self.makers.get(host)?;
        rule.from
            .matches(&maker.callee)
            .then(|| (rule.reading(&maker.shape), host))
    }

    /// The receiver of `callee` when it is written on a name this file imports from a file of its project: `api` for `api.get`, `store.cache` for `store.cache.get`. A name imported from Node, or from a package that some active pack is for, is a library, and a rule reads a call on it as it is written.
    fn imported_receiver<'c>(&self, callee: &'c str) -> Option<&'c str> {
        let (host, _) = callee.rsplit_once('.')?;
        let root = host.split('.').next().unwrap_or(host);
        let spec = self.constants.spec_of(root)?;
        (!is_library(self.packs, spec)).then_some(host)
    }

    /// The rules that read a call on `callee` as `rule`, which takes no receiver, does, but on a receiver made by a call: the same kind and role, a callee that matches, and enough arguments to be read. When the name `callee` is written on turns out to be such a receiver, they read the call and `rule` does not.
    fn rivals(&self, callee: &str, rule: &CallRule, arguments: usize) -> Vec<RuleRef> {
        let mut rivals = Vec::new();
        for (pack_index, pack) in self.packs.iter().enumerate() {
            for (rule_index, rival) in pack.calls.iter().enumerate() {
                if rival.receiver.is_some()
                    && rival.kind == rule.kind
                    && rival.role == rule.role
                    && rival.callee.matches(callee)
                    && !reads_setting(rival, arguments)
                {
                    rivals.push(RuleRef::Call {
                        pack: pack_index,
                        rule: rule_index,
                    });
                }
            }
        }
        rivals
    }

    fn call(&mut self, it: &CallExpression<'_>, callee: &str) {
        let line = self.line(it.span.start);
        let packs = self.packs;
        let mut lazy: Option<Shape> = None;
        let mut imported: Option<Option<&str>> = None;
        for (pack_index, pack) in packs.iter().enumerate() {
            for (rule_index, rule) in pack.calls.iter().enumerate() {
                if !rule.callee.matches(callee) {
                    continue;
                }
                let site: &Shape = lazy.get_or_insert_with(|| {
                    Shape::of_call(callee, it.arguments.iter(), &self.constants)
                });
                // A route always has a handler after its path.
                if reads_setting(rule, it.arguments.len()) {
                    continue;
                }
                let at = RuleRef::Call {
                    pack: pack_index,
                    rule: rule_index,
                };
                // The receiver may be a client or a router that another file makes, which only the link can see.
                let from_file = *imported.get_or_insert_with(|| self.imported_receiver(callee));
                let (prefix, host) = match (&rule.receiver, from_file) {
                    (None, Some(from_file)) => {
                        let rivals = self.rivals(callee, rule, it.arguments.len());
                        if !rivals.is_empty() {
                            self.defer(Deferred {
                                rule: at,
                                shape: site.clone(),
                                prefix: None,
                                line,
                                what: callee.to_owned(),
                                target: DeferredTarget::Fallback {
                                    host: from_file.to_owned(),
                                    rivals,
                                },
                            });
                            continue;
                        }
                        (None, None)
                    }
                    (None, None) => (None, None),
                    (Some(receiver), from_file) => {
                        let Some((reading, host)) = self.receiver(callee, receiver) else {
                            if let Some(from_file) = from_file {
                                self.defer(Deferred {
                                    rule: at,
                                    shape: site.clone(),
                                    prefix: None,
                                    line,
                                    what: callee.to_owned(),
                                    target: DeferredTarget::Receiver {
                                        host: from_file.to_owned(),
                                    },
                                });
                            }
                            continue;
                        };
                        let Some(prefix) = rule.behind(&reading).map(str::to_owned) else {
                            self.record(Outcome::blind(line, callee));
                            continue;
                        };
                        (Some(prefix), Some(host))
                    }
                };
                let outcome = rule.apply(&pack.name, site, prefix.as_deref(), line, callee);
                if waits_for_link(&outcome, site) {
                    let target = match host {
                        Some(host) if serves_http(&rule.kind, rule.role) => {
                            DeferredTarget::Host(host.to_owned())
                        }
                        _ => DeferredTarget::Own,
                    };
                    self.defer(Deferred {
                        rule: at,
                        shape: site.clone(),
                        prefix,
                        line,
                        what: callee.to_owned(),
                        target,
                    });
                    continue;
                }
                match (outcome, host) {
                    (Outcome::Fact(fact), Some(host)) if serves_http(&rule.kind, rule.role) => {
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
                let site: &Shape = lazy.get_or_insert_with(|| {
                    Shape::of_call(callee, it.arguments.iter(), &self.constants)
                });
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
        if self.gates.nestjs
            && let Some(host) = callee.strip_suffix(".setGlobalPrefix")
        {
            self.global_prefix(host, it, line);
        }
        self.trpc_router(it, callee);
    }

    /// `app.setGlobalPrefix('api')` on an app that `NestFactory.create(AppModule)` made in this file puts `AppModule` and every controller it reaches behind the prefix. Every other `setGlobalPrefix` call is a site the file could not read: one on an app made elsewhere or from a root module that is not a plain name, and one whose prefix is not a literal. A call that passes options is applied and also a site, because the routes the options exclude are not behind the prefix.
    fn global_prefix(&mut self, app: &str, call: &CallExpression<'_>, line: Line) {
        let made = self
            .makers
            .get(app)
            .filter(|maker| maker.callee == NEST_FACTORY_CREATE)
            .map(|maker| maker.root.clone());
        let Some(root) = made else {
            self.unread(
                call.span,
                line,
                UnresolvedReason::Unsupported(
                    "NestJS global prefix on an app that NestFactory.create does not make in this file"
                        .to_owned(),
                ),
            );
            return;
        };
        let Some(root) = root else {
            self.unread(
                call.span,
                line,
                UnresolvedReason::Unsupported(
                    "NestJS global prefix on an app whose root module is not a plain name"
                        .to_owned(),
                ),
            );
            return;
        };
        let Some(prefix) = call
            .arguments
            .first()
            .and_then(Argument::as_expression)
            .and_then(|prefix| text_with(prefix, &self.constants))
            .filter(|prefix| !prefix.contains(COMPUTED))
        else {
            self.unread(call.span, line, UnresolvedReason::NonLiteral);
            return;
        };
        self.out.extra.mounts.push(Mount {
            host: app.to_owned(),
            prefix,
            targets: vec![MountTarget::Local(root)],
            line,
        });
        if call.arguments.len() > 1 {
            self.unread(
                call.span,
                line,
                UnresolvedReason::Unsupported("NestJS global prefix options".to_owned()),
            );
        }
    }

    /// A site this file could not read, at `line` and for `reason`, shown by the source that `span` covers.
    fn unread(&mut self, span: Span, line: Line, reason: UnresolvedReason) {
        let detail = self.snippet(span);
        self.out.unresolved.push(UnresolvedSite {
            line,
            reason,
            detail,
        });
    }

    /// Keeps a site for the link phase to read, with every trace of its target that a site read here leaves.
    fn defer(&mut self, deferred: Deferred) {
        if let DeferredTarget::Class { class, .. } = &deferred.target {
            self.hosts.insert(class.clone());
        }
        self.out.extra.deferred.push(deferred);
    }

    /// `@Module({ imports, controllers })` on the class `module` mounts the modules and controllers it lists as plain names or `forwardRef(() => Name)` on that class, with no path of their own.
    fn module_mounts(&mut self, module: &str, decorators: &[Decorator<'_>]) {
        for decorator in decorators {
            let Expression::CallExpression(call) = decorator.expression.get_inner_expression()
            else {
                continue;
            };
            let is_module = dotted_name(&call.callee)
                .is_some_and(|name| name == "Module" || name.ends_with(".Module"));
            if !is_module {
                continue;
            }
            self.hosts.insert(module.to_owned());
            let targets = module_targets(call);
            if !targets.is_empty() {
                self.out.extra.mounts.push(Mount {
                    host: module.to_owned(),
                    prefix: String::new(),
                    targets,
                    line: self.line(decorator.span.start),
                });
            }
        }
    }

    /// A class that extends a plain name and carries the class decorator of an http serves rule serves the routes of that base class under the prefix of the decorator. The base class may be in another file, so the link phase resolves the name.
    fn inherit(&mut self, name: &str) {
        let packs = self.packs;
        let Some((rule, decorator)) = packs
            .iter()
            .flat_map(|pack| &pack.decorators)
            .filter(|rule| serves_http(&rule.kind, rule.role))
            .filter_map(|rule| rule.class.as_ref())
            .find_map(|prefix| Some((prefix, self.class_decorator(prefix)?)))
        else {
            return;
        };
        // A prefix that is not read gives no route to the class, so it gives none to inherit either: `unread_prefixes` has reported it.
        let ClassPrefix::Text(prefix) = ClassPrefix::of(rule, decorator) else {
            return;
        };
        let line = self.line(decorator.span.start);
        let Some(base) = self
            .classes
            .last()
            .and_then(|frame| frame.heritage.as_deref())
            .filter(|base| !base.contains('.'))
            .map(str::to_owned)
        else {
            return;
        };
        self.hosts.insert(name.to_owned());
        self.out.extra.inherits.push(Inherit {
            class: name.to_owned(),
            base,
            prefix,
            line,
        });
    }

    /// One site this file could not read for each decorator of the innermost class whose route prefix is not text, although many route rules may share the prefix: the class serves no route.
    fn unread_prefixes(&mut self) {
        let packs = self.packs;
        let mut spans: Vec<Span> = packs
            .iter()
            .flat_map(|pack| &pack.decorators)
            .filter_map(|rule| rule.class.as_ref())
            .filter_map(|prefix| {
                let decorator = self.class_decorator(prefix)?;
                matches!(ClassPrefix::of(prefix, decorator), ClassPrefix::Unread)
                    .then_some(decorator.span)
            })
            .collect();
        spans.sort_by_key(|span| span.start);
        spans.dedup();
        for span in spans {
            let line = self.line(span.start);
            self.unread(span, line, UnresolvedReason::NonLiteral);
        }
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
                    .map(|element| {
                        element
                            .as_expression()
                            .and_then(|element| text_with(element, &self.constants))
                    })
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
                && let Some(text) = text_with(argument, &self.constants)
            {
                prefixes = vec![text];
            } else if let Some(target) = mount_target(argument) {
                targets.push(target);
            }
        }
        if targets.is_empty() {
            return;
        }
        if computed || prefixes.iter().any(|prefix| prefix.contains(COMPUTED)) {
            self.unread(call.span, line, UnresolvedReason::NonLiteral);
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
            let Some((name, arguments)) = decorator_call(decorator) else {
                continue;
            };
            // Most decorators of a program (`@Injectable`, `@Column`) are named by no rule, and their arguments are not read.
            let named = packs
                .iter()
                .flat_map(|pack| &pack.decorators)
                .any(|rule| rule.receiver.is_none() && rule.decorator.matches(&name));
            if !named {
                continue;
            }
            let mut site = Shape::of_call(&name, arguments.iter(), &self.constants);
            site.member = member.map(str::to_owned);
            let line = self.line(decorator.span.start);
            for (pack_index, pack) in packs.iter().enumerate() {
                for (rule_index, rule) in pack.decorators.iter().enumerate() {
                    if !rule.decorator.matches(&name) || rule.receiver.is_some() {
                        continue;
                    }
                    let serves = serves_http(&rule.kind, rule.role);
                    let (prefix, inherited) = match &rule.class {
                        None => (None, false),
                        Some(class) => match self.class_prefix(class) {
                            Some(ClassPrefix::Text(prefix)) => (Some(prefix), false),
                            // A class without the decorator may be the base of a controller in another file, which serves its routes.
                            None if serves => (Some(String::new()), true),
                            // A prefix this file cannot read serves no route, and `unread_prefixes` has reported it; a class without the decorator serves nothing for a rule that does not serve.
                            Some(ClassPrefix::Unread) | None => continue,
                        },
                    };
                    let outcome = rule.apply(&pack.name, &site, prefix.as_deref(), line, &name);
                    if waits_for_link(&outcome, &site) {
                        let class = self.classes.last().and_then(|frame| frame.name.clone());
                        let target = match class {
                            Some(class) if rule.class.is_some() && serves => {
                                DeferredTarget::Class { class, inherited }
                            }
                            _ => DeferredTarget::Own,
                        };
                        self.defer(Deferred {
                            rule: RuleRef::Decorator {
                                pack: pack_index,
                                rule: rule_index,
                            },
                            shape: site.clone(),
                            prefix,
                            line,
                            what: name.clone(),
                            target,
                        });
                        continue;
                    }
                    if rule.class.is_some() && serves {
                        self.class_route(&outcome, inherited);
                    }
                    // The route of an undecorated class is served by the controllers that extend it, so it is not a fact here; a path it could not read is still a site this file could not read.
                    if !(inherited && matches!(outcome, Outcome::Fact(_))) {
                        self.record(outcome);
                    }
                }
            }
        }
    }

    /// Keeps the route `outcome` names, when it names one, for the link phase, with the class it is declared in.
    fn class_route(&mut self, outcome: &Outcome, inherited: bool) {
        let (Outcome::Fact(fact), Some(class)) = (
            outcome,
            self.classes.last().and_then(|frame| frame.name.clone()),
        ) else {
            return;
        };
        self.hosts.insert(class.clone());
        self.out.extra.class_routes.push(ClassRoute {
            class,
            fact: fact.clone(),
            inherited,
        });
    }

    /// The decorator of the innermost class that the class prefix `rule` names; `None` when the class carries none.
    fn class_decorator(&self, rule: &Prefix) -> Option<&ClassDecorator> {
        self.classes
            .last()?
            .decorators
            .iter()
            .find(|decorator| rule.from.matches(&decorator.name))
    }

    /// The prefix the innermost class gives through the decorator `rule` names; `None` when it carries none.
    fn class_prefix(&self, rule: &Prefix) -> Option<ClassPrefix> {
        let decorator = self.class_decorator(rule)?;
        Some(ClassPrefix::of(rule, decorator))
    }

    /// The assign rules for `target = value`; a class field counts when the class extends one the rule names.
    fn assignment(&mut self, target: &str, value: &Expression<'_>, offset: u32, in_class: bool) {
        let heritage = if in_class {
            self.classes.last().and_then(|frame| frame.heritage.clone())
        } else {
            None
        };
        let line = self.line(offset);
        let site = Shape::of_value(text_with(value, &self.constants));
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

    /// How a compound assignment or an update at `span` touches its target. As an expression statement of its own it drops the value, so it only writes; anywhere else the value can be used and the old value is read too.
    fn modification(&self, span: Span) -> Access {
        if self.discarded == Some(span) {
            Access::Write
        } else {
            Access::ReadWrite
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

    fn finish(mut self) -> Analysis {
        self.import_rules();
        let Self {
            makers,
            symbols,
            constants,
            mut out,
            mut hosts,
            exports_unknown,
            ..
        } = self;
        out.extra.makers = makers::exported(&makers, &out.extra.exports);
        hosts.extend(
            makers
                .into_iter()
                .filter(|(_, maker)| maker.serves)
                .map(|(name, _)| name),
        );
        out.extra.hosts = hosts.into_iter().collect();
        out.contracts.sort_by(|a, b| order(a).cmp(&order(b)));
        out.contracts.dedup_by(|a, b| order(a) == order(b));
        out.extra
            .routes
            .sort_by(|a, b| (&a.host, order(&a.fact)).cmp(&(&b.host, order(&b.fact))));
        out.extra
            .routes
            .dedup_by(|a, b| a.host == b.host && order(&a.fact) == order(&b.fact));
        out.extra.class_routes.sort_by(|a, b| {
            (&a.class, order(&a.fact), a.inherited).cmp(&(&b.class, order(&b.fact), b.inherited))
        });
        out.extra.class_routes.dedup_by(|a, b| {
            a.class == b.class && order(&a.fact) == order(&b.fact) && a.inherited == b.inherited
        });
        out.extra.deferred.sort_by_key(|deferred| deferred.line);
        out.extra.deferred.dedup();
        out.extra.constants = constants.exported(&out.extra.exports);
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
        let type_declaration = matches!(it.import_kind, ImportOrExportKind::Type);
        let read_by_rules = !type_declaration && self.packs_read(spec);
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
            let binding = self.bind(local, spec, &name);
            if read_by_rules && !is_type && is_member(&name) {
                self.imported.push(Imported { binding, line });
            }
            import.names.push(name);
            import.exported.push(local.to_owned());
        }
        if type_declaration || (all_type && !import.names.is_empty()) {
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
        // `import A = B.C` names something else of the file, which `A` then uses as a value, so the walk does not take `B.C` for a type name.
        match &it.module_reference {
            TSModuleReference::ExternalModuleReference(reference) => {
                if let Some(spec) = reference.expression.value.as_str() {
                    let mut import =
                        Import::new(spec, ImportKind::Require, self.line(it.span.start));
                    if matches!(it.import_kind, ImportOrExportKind::Type) {
                        import.flags = import.flags | ImportFlags::TYPE_ONLY;
                    }
                    self.bind(it.id.name.as_str(), spec, "default");
                    import.names.push("default".to_owned());
                    self.out.imports.push(import);
                }
            }
            TSModuleReference::IdentifierReference(name) => self.visit_identifier_reference(name),
            TSModuleReference::QualifiedName(name) => {
                if let Some(root) = name.left.get_identifier_reference() {
                    self.visit_identifier_reference(root);
                }
            }
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

    /// `typeof import('x')` and `import('x').T` in a type bind nothing and run nothing, but the file depends on `x`. Without a qualifier the type is the module itself, which is the name `*` of a namespace import. A type is no use of a value, so it reaches no import rule.
    fn visit_ts_import_type(&mut self, it: &TSImportType<'a>) {
        if let Some(spec) = it.source.value.as_str() {
            let line = self.line(it.source.span.start);
            let mut import = Import::new(spec, ImportKind::Static, line);
            import.flags = import.flags | ImportFlags::TYPE_ONLY;
            let name = it.qualifier.as_ref().map_or("*", first_name);
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
            if self.gates.trpc
                && let (BindingPattern::BindingIdentifier(id), Expression::CallExpression(call)) =
                    (&it.id, init)
                && maps::is_procedure(call)
            {
                self.out.extra.procedures.push(id.name.as_str().to_owned());
            }
            self.destructured_chain(&it.id, init, it.span.start);
            if self.gates.graphql
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
        if self.gates.graphql
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
        if it.operator != AssignmentOperator::Assign {
            self.access = Some(self.modification(it.span));
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_update_expression(&mut self, it: &UpdateExpression<'a>) {
        self.access = Some(self.modification(it.span));
        walk::walk_update_expression(self, it);
    }

    fn visit_expression_statement(&mut self, it: &ExpressionStatement<'a>) {
        self.discarded = Some(it.expression.without_parentheses().span());
        walk::walk_expression_statement(self, it);
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
        let access = self.access.take().unwrap_or(Access::Read);
        let Some(names) = member_chain(it) else {
            match it {
                MemberExpression::PrivateFieldExpression(member) => {
                    self.symbols.private(member.field.name.as_str(), None);
                }
                MemberExpression::StaticMemberExpression(member) => {
                    let property = member.property.name.as_str();
                    self.member_of_load(&member.object, property);
                    // `this.#field.member` reads a member of the type of the field, so the field is not read as a value. The types written around the field are still walked, as for `this.field.member`.
                    if let Expression::PrivateFieldExpression(field) =
                        member.object.get_inner_expression()
                    {
                        self.walk_spine(&member.object);
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
        self.chain_rules(&names, it.span().start, access);
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
        let name = it.id.as_ref().map(|id| id.name.as_str().to_owned());
        let packs = self.packs;
        let frame = ClassFrame {
            name: name.clone(),
            // Only the decorators that a class prefix names are kept: the others give the class nothing to read.
            decorators: it
                .decorators
                .iter()
                .filter_map(|decorator| {
                    let (name, arguments) = decorator_call(decorator)?;
                    packs
                        .iter()
                        .flat_map(|pack| &pack.decorators)
                        .filter_map(|rule| rule.class.as_ref())
                        .any(|prefix| prefix.from.matches(&name))
                        .then(|| ClassDecorator {
                            shape: Shape::of_call(&name, arguments.iter(), &self.constants),
                            name,
                            span: decorator.span,
                        })
                })
                .collect(),
            heritage: it
                .heritage
                .as_ref()
                .and_then(|heritage| dotted_name(&heritage.expression))
                .map(Cow::into_owned),
        };
        self.classes.push(frame);
        if !it.decorators.is_empty() {
            self.unread_prefixes();
        }
        self.decorators(&it.decorators, name.as_deref());
        if let Some(name) = &name {
            if self.gates.nestjs {
                self.module_mounts(name, &it.decorators);
            }
            if !it.decorators.is_empty() {
                self.inherit(name);
            }
        }
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

    fn visit_ts_type(&mut self, it: &TSType<'a>) {
        let outer = self.symbols.enter_type();
        walk::walk_ts_type(self, it);
        self.symbols.leave_type(outer);
    }

    /// The name of a type reference, and of an `implements` or an interface `extends`, which are types and not values even where the walk meets them outside a `TSType`.
    fn visit_ts_type_name(&mut self, it: &TSTypeName<'a>) {
        let outer = self.symbols.enter_type();
        walk::walk_ts_type_name(self, it);
        self.symbols.leave_type(outer);
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
        // A compound assignment or an update has said how it touches its target; every other target, in a destructuring or in the head of a loop, is stored into.
        self.access.get_or_insert(Access::Write);
        walk::walk_simple_assignment_target(self, it);
        self.access = None;
    }

    fn visit_assignment_target_property_identifier(
        &mut self,
        it: &AssignmentTargetPropertyIdentifier<'a>,
    ) {
        self.symbols.write(it.binding.name.as_str());
        walk::walk_assignment_target_property_identifier(self, it);
    }

    fn visit_unary_expression(&mut self, it: &UnaryExpression<'a>) {
        if it.operator == UnaryOperator::Delete {
            if let Some(base) = symbols::base_name(&it.argument) {
                self.symbols.write(base);
            }
            if is_member_access(&it.argument) {
                self.access = Some(Access::Write);
            }
        }
        walk::walk_unary_expression(self, it);
        self.access = None;
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

/// Whether `expr`, with parentheses and type assertions taken off, is a member access, so that the walk reaches a member expression for it first.
fn is_member_access(expr: &Expression<'_>) -> bool {
    match expr.get_inner_expression() {
        Expression::ChainExpression(chain) => chain.expression.is_member_expression(),
        other => other.is_member_expression(),
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

/// Whether `rule` serves routes on a receiver and a call with `arguments` arguments has no handler after its path, so it reads a setting: `app.get('env')` is no route.
fn reads_setting(rule: &CallRule, arguments: usize) -> bool {
    rule.receiver.is_some() && serves_http(&rule.kind, rule.role) && arguments < 2
}

/// Whether `spec` names Node or a package that some pack of `packs` is for.
fn is_library(packs: &[Pack], spec: &str) -> bool {
    is_builtin(spec)
        || (!is_relative(spec)
            && split_package(spec).is_some_and(|(package, _)| {
                packs
                    .iter()
                    .any(|pack| pack.detect.iter().any(|name| name == package))
            }))
}

/// The name a decorator is applied by and the arguments of the call it is written as, none for `@Name`. `None` when it is applied by anything but a name or a chain of names.
fn decorator_call<'d, 'a>(decorator: &'d Decorator<'a>) -> Option<(String, &'d [Argument<'a>])> {
    match decorator.expression.get_inner_expression() {
        Expression::CallExpression(call) => {
            Some((dotted(&chain_of(&call.callee)?)?, call.arguments.as_slice()))
        }
        other => Some((dotted(&chain_of(other)?)?, &[])),
    }
}

/// Whether the rule that found `outcome` could not read `site` and some of the values it could not read are names that the link may read from the file that declares them.
fn waits_for_link(outcome: &Outcome, site: &Shape) -> bool {
    matches!(outcome, Outcome::Blind(_)) && site.has_names()
}

/// The modules and controllers that the `controllers` and `imports` arrays of the `@Module` call list, as names: a plain name, or `forwardRef(() => Name)`. A module made by a call (`ConfigModule.forRoot()`), a spread and a member are left out.
fn module_targets(module: &CallExpression<'_>) -> Vec<MountTarget> {
    let Some(Expression::ObjectExpression(object)) = module
        .arguments
        .first()
        .and_then(Argument::as_expression)
        .map(Expression::get_inner_expression)
    else {
        return Vec::new();
    };
    object
        .properties
        .iter()
        .filter_map(|property| match property {
            ObjectPropertyKind::ObjectProperty(property) if !property.computed => Some(property),
            _ => None,
        })
        .filter(|property| matches!(property_str(&property.key), Some("controllers" | "imports")))
        .filter_map(|property| match property.value.get_inner_expression() {
            Expression::ArrayExpression(array) => Some(array),
            _ => None,
        })
        .flat_map(|array| &array.elements)
        .filter_map(|element| module_target(element.as_expression()?))
        .collect()
}

/// A name, or the name `forwardRef(() => Name)` returns. A member such as `ns.Module` is not followed.
fn module_target(expr: &Expression<'_>) -> Option<MountTarget> {
    let named = match expr.get_inner_expression() {
        Expression::CallExpression(call) => {
            let callee = dotted_name(&call.callee)?;
            if callee != "forwardRef" && !callee.ends_with(".forwardRef") {
                return None;
            }
            let Expression::ArrowFunctionExpression(arrow) = call
                .arguments
                .first()?
                .as_expression()?
                .get_inner_expression()
            else {
                return None;
            };
            arrow.get_expression()?.get_inner_expression()
        }
        other => other,
    };
    if matches!(named, Expression::Identifier(_)) {
        mount_target(named)
    } else {
        None
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

#[cfg(test)]
mod tests {
    use gsttt_core::contract::Role;
    use gsttt_scenario::scenario;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;

    use super::*;

    fn packs() -> Vec<Pack> {
        gsttt_config::builtin_packs(crate::PACKS).unwrap()
    }

    /// What the parse of `code` finds, with the built-in packs.
    fn analysis_of(code: &str) -> Analysis {
        analysis_in(code, oxc_span::SourceType::ts())
    }

    fn analysis_in(code: &str, source_type: oxc_span::SourceType) -> Analysis {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, code, source_type).parse();
        analyze(&parsed.program, code, &LineIndex::new(code), &packs())
    }

    #[test]
    #[scenario(
        "typescript",
        "A client whose base is not text is called at the path after it"
    )]
    fn a_client_whose_base_is_not_text_is_called_at_the_path_after_it() {
        let analysis = analysis_of(
            "import axios from 'axios';\nconst api = axios.create({ baseURL: process.env.API });\napi.get('/users');\n",
        );
        let calls: Vec<String> = analysis
            .contracts
            .iter()
            .filter(|fact| fact.role == Role::Calls && fact.via == "axios")
            .map(|fact| fact.key.to_string())
            .collect();
        assert_eq!(calls, ["http GET /users"]);
        assert_eq!(analysis.unresolved, []);
    }

    #[test]
    #[scenario("typescript", "A spread may carry a property")]
    fn a_spread_may_carry_a_property() {
        let pack = r#"name = "requests"
language = "ts"

[[calls]]
callee = "client.request"
kind = "http"
role = "calls"
key = "$arg1"
method = "$arg2.method|GET"
"#;
        let packs = gsttt_config::builtin_packs(&[("requests", pack)]).unwrap();
        let code = "client.request('/plain');\nclient.request('/post', { method: 'POST' });\nclient.request('/options', { ...options });\nclient.request('/rest', ...rest);\nclient.request(...args);\nclient.request('/own', { ...options, method: 'PUT' });\n";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, code, oxc_span::SourceType::ts()).parse();
        let analysis = analyze(&parsed.program, code, &LineIndex::new(code), &packs);
        let calls: Vec<String> = analysis
            .contracts
            .iter()
            .map(|fact| format!("{}@{}", fact.key, fact.line))
            .collect();
        // What the call writes is read wherever the spread stands, and a method that a spread may give is any method, where the same call without the spread is `GET`.
        assert_eq!(
            calls,
            [
                "http GET /plain@1",
                "http POST /post@2",
                "http * /options@3",
                "http * /rest@4",
                "http PUT /own@6",
            ]
        );
        // A call whose only argument is a spread may carry the url, so it is an unread site.
        let unread: Vec<(u32, &str)> = analysis
            .unresolved
            .iter()
            .map(|site| (site.line, site.detail.as_str()))
            .collect();
        assert_eq!(unread, [(5, "client.request")]);
    }

    #[test]
    fn node_and_the_packages_of_the_packs_are_libraries() {
        let packs = packs();
        for spec in [
            "fs",
            "node:fs",
            "fs/promises",
            "node:test",
            "axios",
            "axios/lib/x",
            "express",
            "@nestjs/common",
            "@nestjs/common/decorators",
        ] {
            assert!(is_library(&packs, spec), "{spec}");
        }
    }

    #[test]
    fn a_package_that_no_pack_is_for_and_a_relative_path_are_no_libraries() {
        let packs = packs();
        for spec in [
            "left-pad",
            "axios-extra",
            "@acme/ui",
            "@acme/ui/button",
            "@nestjs/testing",
            ".",
            "..",
            "./http",
            "../lib/http",
        ] {
            assert!(!is_library(&packs, spec), "{spec}");
        }
    }

    #[test]
    fn a_call_on_a_package_or_node_defers_nothing() {
        let analysis = analysis_of(
            "import axios from 'axios';\nimport http from 'node:http';\nimport https from 'https';\naxios.get('/x');\nhttp.get('/y');\nhttps.get('/z');\n",
        );
        assert_eq!(analysis.contracts.len(), 1);
        assert_eq!(analysis.extra.deferred, []);
    }

    #[test]
    fn a_call_on_a_name_from_a_file_of_the_project_is_deferred_on_that_name() {
        let analysis = analysis_of("import { api } from './http';\napi.get('/x');\n");
        assert_eq!(analysis.contracts, []);
        assert!(matches!(
            analysis.extra.deferred.as_slice(),
            [Deferred { target: DeferredTarget::Receiver { host }, .. }] if host == "api"
        ));
    }

    /// A file that imports the model `Tenant` by name from the client and goes on with `$code`.
    macro_rules! with_import {
        ($code:expr) => {
            concat!("import { Tenant } from '@prisma/client';\n", $code, "\n")
        };
    }

    /// How many tables `code`, read as a TSX file with the built-in packs, calls.
    fn tables_called_by(code: &str) -> usize {
        analysis_in(code, oxc_span::SourceType::tsx())
            .contracts
            .len()
    }

    #[test]
    #[scenario("typescript", "A name in a value position is a use")]
    fn a_name_in_a_value_position_is_a_use_of_the_value() {
        for code in [
            with_import!("export const t = Tenant();"),
            with_import!("export const t = [Tenant];"),
            with_import!("export const status = Tenant.ACTIVE;"),
            with_import!("export const t = typeof Tenant;"),
            with_import!("export default Tenant;"),
            with_import!("export class Row extends Tenant {}"),
            with_import!("export enum Kind { A = Tenant.A }"),
            with_import!("export const t = Tenant as unknown;"),
            with_import!("export const t = Tenant satisfies object;"),
            with_import!("export const t = Tenant!;"),
            with_import!("export function f(t: Map<Box<string>, Tenant>) { return Tenant.A; }"),
            with_import!("export const o = { Tenant };"),
            with_import!("export const o = { ...Tenant };"),
            with_import!("export const e = <Tenant />;"),
            with_import!("export const f = (t = Tenant) => t;"),
            with_import!("export class Row { x = Tenant; }"),
            with_import!("import Alias = Tenant.Inner;\nexport { Alias };"),
            with_import!(
                "export class Holder { constructor(private readonly tenant: Tenant) {} get() { return Tenant.fields; } }"
            ),
        ] {
            assert_eq!(tables_called_by(code), 1, "{code}");
        }
    }

    #[test]
    #[scenario("typescript", "A name outside a value position is no use")]
    fn a_name_outside_a_value_position_is_no_use_of_the_value() {
        for code in [
            with_import!("export function f(tenant: Tenant): Tenant { return tenant; }"),
            with_import!("export class Row implements Tenant {}"),
            with_import!("interface Row extends Tenant {}"),
            with_import!("declare function g<T>(): T;\nexport const t = g<Tenant>();"),
            with_import!("export type Shape = typeof Tenant;"),
            with_import!("export type Alias = Tenant[];"),
            with_import!("declare const x: unknown;\nexport const t = x satisfies Tenant;"),
            with_import!("declare const x: unknown;\nexport const t = x as Tenant;"),
            with_import!("export declare function h(tenant: Tenant): void;"),
            with_import!(
                "export class Holder { constructor(private readonly tenant: Tenant) {} get() { return this.tenant; } }"
            ),
            with_import!("export { Tenant };"),
            with_import!("export { Tenant as T };"),
            "import type { Tenant } from '@prisma/client';\nexport type Row = Tenant;\n",
            "import { type Tenant } from '@prisma/client';\nexport type Row = Tenant;\n",
            "export type Row = import('@prisma/client').Tenant;\n",
            "export { Tenant as T } from '@prisma/client';\n",
            "export type { Tenant } from '@prisma/client';\n",
        ] {
            assert_eq!(tables_called_by(code), 0, "{code}");
        }
    }

    #[test]
    fn a_name_the_import_rule_excepts_names_nothing_when_used_as_a_value() {
        let code = "import { Prisma, PrismaClient } from '@prisma/client';\nexport const sql = Prisma.sql;\nexport const client = new PrismaClient();\n";
        assert_eq!(tables_called_by(code), 0);
    }
}
