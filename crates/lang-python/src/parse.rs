//! Reads one Python file into facts with the ruff parser: its imports, the contract sites the packs in force
//! name, and what `link` needs to compose route prefixes across files.

use std::borrow::Cow;
use std::collections::HashMap;

use gsttt_core::connector::{Parsed, SourceFile};
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::fact::{
    FileFacts, Import, ImportFlags, ImportKind, Line, ParseStatus, UnresolvedReason, UnresolvedSite,
};
use gsttt_core::rules::{Outcome, Pack, Prefix, Site};
use gsttt_core::text::LineIndex;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    Arguments, Decorator, Expr, ExprCall, ExprContext, ExprSubscript, PySourceType, Stmt,
    StmtAnnAssign, StmtAssign, StmtClassDef, StmtFunctionDef, StmtIf, StmtImport, StmtImportFrom,
    StmtTry, WithItem,
};
use ruff_python_parser::{ParseError, parse_unchecked_source};
use ruff_text_size::{Ranged, TextSize};

use crate::extra::{Bind, Mount, PyExtra, Route, Target};
use crate::files::{is_python_source, tags_of};
use crate::project::PythonProject;
use crate::site::{AssignSite, CallSite, dict_prop, dotted, join_prefixes, literal_of, text_of};

/// Reads `file`. A file that is not Python source, such as a manifest, comes back with its tags only.
pub(crate) fn parse(project: &PythonProject, file: &SourceFile<'_>) -> Parsed<PyExtra> {
    let tags = tags_of(file.path);
    if !is_python_source(file.path) {
        return Parsed::new(FileFacts {
            tags,
            ..FileFacts::default()
        });
    }
    let source_type = if file.path.extension() == Some("pyi") {
        PySourceType::Stub
    } else {
        PySourceType::Python
    };
    // The parser recovers from syntax errors and keeps the statements it could read; the imports among them stay.
    let parsed = parse_unchecked_source(file.text, source_type);
    let lines = LineIndex::new(file.text);
    let mut collector = Collector::new(&project.packs, &lines);
    collector.visit_body(parsed.suite());
    let mut read = collector.finish();
    read.facts.tags = tags;
    read.facts.status = status_of(parsed.errors(), &lines);
    read
}

fn status_of(errors: &[ParseError], lines: &LineIndex) -> ParseStatus {
    let Some(first) = errors
        .iter()
        .map(|error| u32::from(error.location.start()))
        .min()
    else {
        return ParseStatus::Ok;
    };
    ParseStatus::Recovered {
        errors: u32::try_from(errors.len()).unwrap_or(u32::MAX),
        first_error_line: lines.line(first),
    }
}

/// A name assigned a call, `router = APIRouter(prefix="/bookings")`. Rules written for a receiver read it to tell
/// what the receiver was made by.
struct Binding<'a> {
    callee: String,
    arguments: &'a Arguments,
}

/// What encloses the statement being read.
enum Frame<'a> {
    Function,
    Class(ClassFrame<'a>),
}

struct ClassFrame<'a> {
    /// The decorators of the class by dotted name, with their arguments when they are called.
    decorators: Vec<(String, Option<&'a Arguments>)>,
    bases: Vec<String>,
    /// What a settings class puts before the name of each field.
    env_prefix: String,
}

/// Walks the syntax tree once, in source order, and collects everything the file holds.
struct Collector<'a> {
    packs: &'a [Pack],
    lines: &'a LineIndex,
    facts: FileFacts,
    extra: PyExtra,
    binds: Vec<Bind>,
    flags: ImportFlags,
    frames: Vec<Frame<'a>>,
    /// Names assigned a call anywhere in the file. A name assigned twice keeps its last assignment: the file is
    /// read as one scope, which is how a module-level router or client is used.
    bindings: HashMap<String, Binding<'a>>,
}

impl<'a> Collector<'a> {
    fn new(packs: &'a [Pack], lines: &'a LineIndex) -> Self {
        Self {
            packs,
            lines,
            facts: FileFacts::default(),
            extra: PyExtra::default(),
            binds: Vec::new(),
            flags: ImportFlags::NONE,
            frames: Vec::new(),
            bindings: HashMap::new(),
        }
    }

    fn finish(mut self) -> Parsed<PyExtra> {
        // The binds of a file only matter to a file that mounts something.
        if !self.extra.mounts.is_empty() {
            self.extra.binds = self.binds;
        }
        // Visit order is deterministic; a stable sort by line puts the facts of a decorator and of its body in order.
        self.facts.contracts.sort_by_key(|fact| fact.line);
        self.facts.embedded.sort_by_key(|document| document.line);
        self.facts.unresolved.sort_by_key(|site| site.line);
        Parsed {
            facts: self.facts,
            extra: self.extra,
        }
    }

    fn line_of(&self, offset: TextSize) -> Line {
        self.lines.line(u32::from(offset))
    }

    fn has_pack(&self, name: &str) -> bool {
        self.packs.iter().any(|pack| pack.name == name)
    }

    fn unresolved(&mut self, line: Line, what: &str) {
        self.facts.unresolved.push(UnresolvedSite {
            line,
            reason: UnresolvedReason::NonLiteral,
            detail: what.to_owned(),
        });
    }

    fn push_import(&mut self, mut import: Import) {
        import.flags = self.flags;
        self.facts.imports.push(import);
    }

    fn top_class(&self) -> Option<&ClassFrame<'a>> {
        match self.frames.last() {
            Some(Frame::Class(class)) => Some(class),
            _ => None,
        }
    }

    /// Runs `body` with `flag` set on every import it holds.
    fn guarded(&mut self, flag: ImportFlags, body: &'a [Stmt]) {
        let outer = self.flags;
        self.flags = outer | flag;
        self.visit_body(body);
        self.flags = outer;
    }

    fn import(&mut self, stmt: &StmtImport) {
        let line = self.line_of(stmt.start());
        for alias in &stmt.names {
            let spec = alias.name.as_str();
            self.push_import(Import::new(spec, ImportKind::Static, line));
            // `import a.b.c` binds `a` only, which is not the module the import resolves to; the forms that name
            // their module are the ones a router can be reached through.
            let local = match &alias.asname {
                Some(asname) => Some(asname.as_str()),
                None if !spec.contains('.') => Some(spec),
                None => None,
            };
            if let Some(local) = local {
                self.binds.push(Bind {
                    local: local.to_owned(),
                    spec: spec.to_owned(),
                    line,
                    name: None,
                });
            }
        }
    }

    fn import_from(&mut self, stmt: &StmtImportFrom) {
        let line = self.line_of(stmt.start());
        let dots = ".".repeat(usize::try_from(stmt.level).unwrap_or_default());
        let module = stmt.module.as_ref().map_or("", |module| module.as_str());
        let spec = format!("{dots}{module}");
        let mut import = Import::new(spec.as_str(), ImportKind::Static, line);
        import.names = stmt
            .names
            .iter()
            .map(|alias| alias.name.as_str().to_owned())
            .collect();
        if import.names.iter().any(|name| name.as_str() == "*") {
            import.kind = ImportKind::ReExportAll;
        }
        self.push_import(import);
        for alias in stmt.names.iter().filter(|alias| alias.name.as_str() != "*") {
            let local = alias.asname.as_ref().unwrap_or(&alias.name);
            self.binds.push(Bind {
                local: local.as_str().to_owned(),
                spec: spec.clone(),
                line,
                name: Some(alias.name.as_str().to_owned()),
            });
        }
    }

    fn function(&mut self, function: &'a StmtFunctionDef) {
        for decorator in &function.decorator_list {
            self.decorator(decorator, function.name.as_str());
        }
        self.frames.push(Frame::Function);
        self.visit_parameters(&function.parameters);
        self.visit_body(&function.body);
        self.frames.pop();
    }

    fn class(&mut self, class: &'a StmtClassDef) {
        for decorator in &class.decorator_list {
            self.decorator(decorator, class.name.as_str());
        }
        let arguments = class.arguments.as_deref();
        if let Some(arguments) = arguments {
            self.visit_arguments(arguments);
        }
        let frame = ClassFrame {
            decorators: class
                .decorator_list
                .iter()
                .filter_map(|decorator| decorator_parts(&decorator.expression))
                .collect(),
            bases: arguments.map_or_else(Vec::new, |arguments| {
                arguments.args.iter().filter_map(dotted).collect()
            }),
            env_prefix: env_prefix(&class.body),
        };
        self.frames.push(Frame::Class(frame));
        self.visit_body(&class.body);
        self.frames.pop();
    }

    /// Applies the decorator rules to one decorator of the function or class called `member`, then reads what the
    /// decorator's own arguments hold.
    fn decorator(&mut self, decorator: &'a Decorator, member: &str) {
        let line = self.line_of(decorator.expression.start());
        let (call, name) = match &decorator.expression {
            Expr::Call(call) => (Some(call), dotted(&call.func)),
            other => (None, dotted(other)),
        };
        if let Some(name) = name {
            let site = CallSite {
                callee: &name,
                arguments: call.map(|call| &call.arguments),
                member: Some(member),
            };
            let receiver = receiver_of(&name);
            let packs = self.packs;
            for pack in packs {
                for rule in &pack.decorators {
                    if !rule.decorator.matches(&name) {
                        continue;
                    }
                    let mut prefix = None;
                    if let Some(class) = &rule.class {
                        let Some(read) = self.class_prefix(class) else {
                            continue;
                        };
                        prefix = Some(read);
                    }
                    let mut scope = None;
                    if let Some(maker) = &rule.receiver {
                        let Some(read) = self.receiver_prefix(maker, receiver) else {
                            continue;
                        };
                        prefix = Some(match prefix {
                            Some(outer) => join_prefixes(&rule.kind, &outer, &read),
                            None => read,
                        });
                        scope = receiver;
                    }
                    let outcome = rule.apply(&pack.name, &site, prefix.as_deref(), line, &name);
                    self.record(outcome, scope);
                }
            }
        }
        // The decorator itself is not a call to read: only what is inside it is.
        match call {
            Some(call) => {
                self.visit_expr(&call.func);
                self.visit_arguments(&call.arguments);
            }
            None => self.visit_expr(&decorator.expression),
        }
    }

    /// The prefix a decorator rule's `class` reads from the decorators of the class around the statement.
    fn class_prefix(&self, class: &Prefix) -> Option<String> {
        let frame = self.top_class()?;
        frame
            .decorators
            .iter()
            .find(|(callee, _)| class.from.matches(callee))
            .map(|(callee, arguments)| {
                class.read(&CallSite {
                    callee,
                    arguments: *arguments,
                    member: None,
                })
            })
    }

    /// The prefix a rule's `receiver` reads from the call that made `var`, when `var` is a name of this file made
    /// by one of the callees the rule names.
    fn receiver_prefix(&self, maker: &Prefix, var: Option<&str>) -> Option<String> {
        let binding = self.bindings.get(var?)?;
        if !maker.from.matches(&binding.callee) {
            return None;
        }
        Some(maker.read(&CallSite {
            callee: &binding.callee,
            arguments: Some(binding.arguments),
            member: None,
        }))
    }

    /// Keeps what a rule made of a site. A serves fact under `scope` is a route `link` may need to move.
    fn record(&mut self, outcome: Outcome, scope: Option<&str>) {
        match outcome {
            Outcome::Fact(fact) => {
                if let Some(var) = scope
                    && matches!(fact.role, Role::Serves)
                    && matches!(fact.key.kind(), ContractKind::Http)
                {
                    self.extra.routes.push(Route {
                        var: var.to_owned(),
                        line: fact.line,
                    });
                }
                self.facts.contracts.push(fact);
            }
            Outcome::Blind(site) => self.facts.unresolved.push(site),
            Outcome::Skip => {}
        }
    }

    fn call(&mut self, call: &ExprCall) {
        // A call on something that is not a name, `make()()` or `(a or b)()`, has no callee to match.
        let Some(callee) = dotted(&call.func) else {
            return;
        };
        let line = self.line_of(call.start());
        if self.dynamic_import(call, &callee, line) || self.mount(call, &callee, line) {
            return;
        }
        let site = CallSite {
            callee: &callee,
            arguments: Some(&call.arguments),
            member: None,
        };
        let packs = self.packs;
        for pack in packs {
            for rule in &pack.calls {
                if !rule.callee.matches(&callee) {
                    continue;
                }
                let prefix = match &rule.receiver {
                    Some(maker) => {
                        let Some(read) = self.receiver_prefix(maker, receiver_of(&callee)) else {
                            continue;
                        };
                        Some(read)
                    }
                    None => None,
                };
                let outcome = rule.apply(&pack.name, &site, prefix.as_deref(), line, &callee);
                // A route a call declares, Django's `path()`, belongs to the module's URL list.
                self.record(outcome, Some(""));
            }
            for rule in &pack.embeds {
                if !rule.callee.matches(&callee) {
                    continue;
                }
                match rule.apply(&pack.name, &site, line, &callee) {
                    Ok(document) => self.facts.embedded.push(document),
                    // `conn.execute(text("…"))` and `session.execute(select(…))` hold a document or a query
                    // object, not a string built at run time; a call with no argument holds nothing at all.
                    Err(_)
                        if matches!(
                            call.arguments.find_positional(0),
                            None | Some(Expr::Call(_))
                        ) => {}
                    Err(site) => self.facts.unresolved.push(site),
                }
            }
        }
    }

    /// `importlib.import_module("a.b")` and `__import__("a.b")`. Returns whether the call was one.
    fn dynamic_import(&mut self, call: &ExprCall, callee: &str, line: Line) -> bool {
        if !matches!(
            callee,
            "importlib.import_module" | "import_module" | "__import__"
        ) {
            return false;
        }
        match call.arguments.find_positional(0) {
            Some(Expr::StringLiteral(name)) => {
                self.push_import(Import::new(name.value.to_str(), ImportKind::Dynamic, line));
            }
            Some(_) => self.unresolved(line, callee),
            None => {}
        }
        true
    }

    /// A call that mounts a router or an included URL module under a prefix. Returns whether the call was one.
    fn mount(&mut self, call: &ExprCall, callee: &str, line: Line) -> bool {
        let last = callee.rsplit('.').next().unwrap_or(callee);
        match last {
            "include_router" if self.has_pack("fastapi") => {
                self.router_mount(call, callee, line, ("router", "prefix"))
            }
            "register_blueprint" if self.has_pack("flask") => {
                self.router_mount(call, callee, line, ("blueprint", "url_prefix"))
            }
            "path" | "re_path" | "url" if self.has_pack("django") => self.include_mount(call, line),
            _ => false,
        }
    }

    /// `app.include_router(router, prefix="/api")`; `keywords` name the router's and the prefix's keyword.
    fn router_mount(
        &mut self,
        call: &ExprCall,
        callee: &str,
        line: Line,
        keywords: (&str, &str),
    ) -> bool {
        let Some(receiver) = receiver_of(callee) else {
            return false;
        };
        let Some(target) = call
            .arguments
            .find_argument_value(keywords.0, 0)
            .and_then(dotted)
        else {
            self.unresolved(line, callee);
            return true;
        };
        let site = CallSite {
            callee,
            arguments: Some(&call.arguments),
            member: None,
        };
        let mut prefix = String::new();
        if let Some(read) = site.keyword(keywords.1) {
            prefix = read.into_owned();
        } else if call.arguments.find_keyword(keywords.1).is_some() {
            // A prefix held in a variable: the router is still mounted, only its prefix is unknown.
            self.unresolved(line, callee);
        }
        self.extra.mounts.push(Mount {
            line,
            receiver: receiver.to_owned(),
            target: Target::Name(target),
            prefix,
        });
        true
    }

    /// `path("api/", include("bookings.urls"))`. Returns whether the call included something.
    fn include_mount(&mut self, call: &ExprCall, line: Line) -> bool {
        let Some(Expr::Call(include)) = call.arguments.find_positional(1) else {
            return false;
        };
        if dotted(&include.func).is_none_or(|name| name.rsplit('.').next() != Some("include")) {
            return false;
        }
        let mut prefix = String::new();
        if let Some(route) = call.arguments.find_positional(0).and_then(text_of) {
            // The anchors of a `re_path` pattern are not part of the path.
            let route = route.strip_prefix('^').unwrap_or(&route);
            route
                .strip_suffix('$')
                .unwrap_or(route)
                .clone_into(&mut prefix);
        } else {
            self.unresolved(line, "path");
        }
        // `include(("bookings.urls", "bookings"))` names its module first, like `include("bookings.urls")`.
        match include.arguments.find_positional(0).and_then(literal_of) {
            Some(module) => {
                let module = module.into_owned();
                self.push_import(Import::new(module.as_str(), ImportKind::Dynamic, line));
                self.extra.mounts.push(Mount {
                    line,
                    receiver: String::new(),
                    target: Target::Module(module),
                    prefix,
                });
            }
            None => self.unresolved(line, "include"),
        }
        true
    }

    fn subscript(&mut self, subscript: &ExprSubscript) {
        let Some(root) = dotted(&subscript.value) else {
            return;
        };
        let segment = match &*subscript.slice {
            Expr::StringLiteral(name) => Some(name.value.to_str()),
            _ => None,
        };
        let line = self.line_of(subscript.start());
        let packs = self.packs;
        for pack in packs {
            for rule in &pack.chains {
                if rule.root.matches(&root) {
                    let outcome = rule.apply(&pack.name, &[segment], line, &root);
                    self.record(outcome, None);
                }
            }
        }
    }

    /// Remembers `target` as made by `value` when `value` is a call.
    fn bind(&mut self, target: &Expr, value: &'a Expr) {
        let Expr::Call(call) = value else {
            return;
        };
        if let (Some(var), Some(callee)) = (dotted(target), dotted(&call.func)) {
            self.bindings.insert(
                var,
                Binding {
                    callee,
                    arguments: &call.arguments,
                },
            );
        }
    }

    fn assign(&mut self, assign: &'a StmtAssign) {
        for target in &assign.targets {
            self.bind(target, &assign.value);
            self.assign_rules(target, Some(&assign.value), false);
        }
    }

    fn ann_assign(&mut self, assign: &'a StmtAnnAssign) {
        if let Some(value) = assign.value.as_deref() {
            self.bind(&assign.target, value);
        }
        self.assign_rules(&assign.target, assign.value.as_deref(), true);
    }

    /// Applies the assignment rules to `target = value`; `annotated` tells a typed field from a plain assignment.
    fn assign_rules(&mut self, target: &Expr, value: Option<&Expr>, annotated: bool) {
        let Some(name) = dotted(target) else {
            return;
        };
        let line = self.line_of(target.start());
        let packs = self.packs;
        for pack in packs {
            for rule in &pack.assigns {
                if !rule.target.matches(&name) {
                    continue;
                }
                let class = self.top_class();
                if let Some(bases) = &rule.class
                    && !class
                        .is_some_and(|class| class.bases.iter().any(|base| bases.matches(base)))
                {
                    continue;
                }
                // A wildcard target means every field of the class, and only an annotated public name is one.
                if rule.target.iter().any(|pattern| pattern == "*") && !is_field(&name, annotated) {
                    continue;
                }
                let prefix = match class {
                    Some(class)
                        if rule.class.is_some() && matches!(rule.kind, ContractKind::Env) =>
                    {
                        class.env_prefix.as_str()
                    }
                    _ => "",
                };
                let site = AssignSite {
                    name: Cow::Owned(format!("{prefix}{name}")),
                    value,
                };
                let outcome = rule.apply(&pack.name, &site, line, &name);
                self.record(outcome, None);
            }
        }
    }
}

impl<'a> Visitor<'a> for Collector<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Import(import) => self.import(import),
            Stmt::ImportFrom(from) => self.import_from(from),
            Stmt::FunctionDef(function) => self.function(function),
            Stmt::ClassDef(class) => self.class(class),
            Stmt::If(branch) if is_type_checking(&branch.test) => self.type_checking(branch),
            Stmt::Try(attempt) if catches_import_error(attempt) => self.optional_imports(attempt),
            Stmt::Assign(assign) => {
                self.assign(assign);
                visitor::walk_stmt(self, stmt);
            }
            Stmt::AnnAssign(assign) => {
                self.ann_assign(assign);
                visitor::walk_stmt(self, stmt);
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Call(call) => self.call(call),
            Expr::Subscript(subscript) if matches!(subscript.ctx, ExprContext::Load) => {
                self.subscript(subscript);
            }
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }

    fn visit_with_item(&mut self, item: &'a WithItem) {
        if let Some(variable) = &item.optional_vars {
            self.bind(variable, &item.context_expr);
        }
        visitor::walk_with_item(self, item);
    }
}

impl<'a> Collector<'a> {
    /// `if TYPE_CHECKING:` flags the imports of its body; the `else` branches are ordinary code.
    fn type_checking(&mut self, branch: &'a StmtIf) {
        self.visit_expr(&branch.test);
        self.guarded(ImportFlags::TYPE_CHECKING, &branch.body);
        for clause in &branch.elif_else_clauses {
            self.visit_elif_else_clause(clause);
        }
    }

    /// A `try` that catches `ImportError` flags the imports of its body as optional.
    fn optional_imports(&mut self, attempt: &'a StmtTry) {
        self.guarded(ImportFlags::OPTIONAL, &attempt.body);
        for handler in &attempt.handlers {
            self.visit_except_handler(handler);
        }
        self.visit_body(&attempt.orelse);
        self.visit_body(&attempt.finalbody);
    }
}

/// The part of a dotted callee before its last segment: `router` of `router.get`, `self.client` of
/// `self.client.get`. `None` for a bare name.
fn receiver_of(callee: &str) -> Option<&str> {
    callee.rsplit_once('.').map(|(receiver, _)| receiver)
}

/// A decorator as `(dotted name, arguments when called)`.
fn decorator_parts(expression: &Expr) -> Option<(String, Option<&Arguments>)> {
    match expression {
        Expr::Call(call) => dotted(&call.func).map(|name| (name, Some(&call.arguments))),
        other => dotted(other).map(|name| (name, None)),
    }
}

fn is_type_checking(test: &Expr) -> bool {
    dotted(test).is_some_and(|name| name.rsplit('.').next() == Some("TYPE_CHECKING"))
}

/// Whether a handler of the `try` catches `ImportError` or `ModuleNotFoundError`, alone or in a tuple.
fn catches_import_error(attempt: &StmtTry) -> bool {
    attempt.handlers.iter().any(|handler| {
        let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = handler;
        handler.type_.as_deref().is_some_and(names_import_error)
    })
}

fn names_import_error(expr: &Expr) -> bool {
    match expr {
        Expr::Tuple(tuple) => tuple.elts.iter().any(names_import_error),
        other => dotted(other).is_some_and(|name| {
            matches!(
                name.rsplit('.').next(),
                Some("ImportError" | "ModuleNotFoundError")
            )
        }),
    }
}

/// Whether an assignment in a class body declares a field the way pydantic reads one: an annotated public name
/// that is not the model's own configuration.
fn is_field(name: &str, annotated: bool) -> bool {
    annotated && !name.starts_with('_') && name != "model_config"
}

fn is_name(expr: &Expr, wanted: &str) -> bool {
    matches!(expr, Expr::Name(name) if name.id.as_str() == wanted)
}

/// The literal `env_prefix` a settings class declares, in `model_config = SettingsConfigDict(env_prefix="APP_")`,
/// in a `model_config` dict or in an inner `class Config`. Empty when there is none.
fn env_prefix(body: &[Stmt]) -> String {
    body.iter()
        .find_map(|stmt| match stmt {
            Stmt::Assign(assign) if assign.targets.iter().any(|t| is_name(t, "model_config")) => {
                config_prefix(&assign.value)
            }
            Stmt::AnnAssign(assign) if is_name(&assign.target, "model_config") => {
                assign.value.as_deref().and_then(config_prefix)
            }
            Stmt::ClassDef(inner) if inner.name.as_str() == "Config" => {
                inner.body.iter().find_map(|stmt| match stmt {
                    Stmt::Assign(assign)
                        if assign.targets.iter().any(|t| is_name(t, "env_prefix")) =>
                    {
                        literal_of(&assign.value).map(Cow::into_owned)
                    }
                    _ => None,
                })
            }
            _ => None,
        })
        .unwrap_or_default()
}

fn config_prefix(config: &Expr) -> Option<String> {
    match config {
        Expr::Call(call) => call
            .arguments
            .find_keyword("env_prefix")
            .and_then(|keyword| literal_of(&keyword.value))
            .map(Cow::into_owned),
        Expr::Dict(_) => dict_prop(config, "env_prefix")
            .filter(|prefix| !prefix.contains("${"))
            .map(Cow::into_owned),
        _ => None,
    }
}
