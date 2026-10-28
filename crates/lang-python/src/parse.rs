//! Reads one Python file into facts with the ruff parser: its imports, the contract sites the packs in force
//! name, and what `link` needs to compose route prefixes across files.

use std::borrow::Cow;
use std::collections::HashMap;

use gsttt_core::connector::{Parsed, SourceFile};
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::fact::{
    FileFacts, Import, ImportFlags, ImportKind, Line, ParseFailure, ParseStatus, UnresolvedReason,
    UnresolvedSite,
};
use gsttt_core::rules::{EmbedRule, OneOrMany, Outcome, Pack, Prefix, Site, callee_matches};
use gsttt_core::text::LineIndex;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    Arguments, Decorator, Expr, ExprCall, ExprContext, ExprSubscript, Identifier, PySourceType,
    Stmt, StmtAnnAssign, StmtAssign, StmtClassDef, StmtFunctionDef, StmtIf, StmtImport,
    StmtImportFrom, StmtTry, WithItem,
};
use ruff_python_parser::{ParseError, parse_unchecked_source};
use ruff_text_size::{Ranged, TextSize};

use crate::extra::{Mount, PyExtra, Route, RouterPrefix, Target};
use crate::files::{is_package_init, is_python_source, tags_of};
use crate::imports::{Imports, Name, absolute_name, specifier};
use crate::manifest::{ManifestError, read_pyproject};
use crate::project::PythonProject;
use crate::site::{
    AssignSite, CallSite, EmbedSite, dict_prop, dotted, is_string_shaped, join_prefixes,
    literal_of, text_of,
};

/// Reads `file`. A file that is not Python source, such as a manifest, comes back with its tags, and with the status
/// `manifest_status` gives it.
pub(crate) fn parse(project: &PythonProject, file: &SourceFile<'_>) -> Parsed<PyExtra> {
    let tags = tags_of(file.path);
    if !is_python_source(file.path) {
        return Parsed::new(FileFacts {
            tags,
            status: manifest_status(file),
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
    let mut read = collector.finish(is_package_init(file.path));
    read.facts.tags = tags;
    read.facts.status = status_of(parsed.errors(), &lines, file.text.len());
    read
}

/// A `pyproject.toml` that is not valid TOML has failed: the project reads nothing from it, and the packs it would have
/// switched on are missing, which the coverage should not hide. Every other manifest is read leniently and is fine.
fn manifest_status(file: &SourceFile<'_>) -> ParseStatus {
    if file.path.file_name() != "pyproject.toml" {
        return ParseStatus::Ok;
    }
    match read_pyproject(file.path.as_str(), file.text) {
        Ok(_) => ParseStatus::Ok,
        Err(ManifestError::Toml {
            offset, message, ..
        }) => ParseStatus::Failed(ParseFailure::Syntax {
            line: error_line(&LineIndex::new(file.text), offset, file.text.len()),
            message,
        }),
    }
}

/// `len` is the length of the text: the parser puts the error of a bracket left open at the end of the text, one byte
/// past the last line, so an offset is held to the last byte to name a line the file has.
fn status_of(errors: &[ParseError], lines: &LineIndex, len: usize) -> ParseStatus {
    let Some(first) = errors
        .iter()
        .map(|error| usize::from(error.location.start()))
        .min()
    else {
        return ParseStatus::Ok;
    };
    ParseStatus::Recovered {
        errors: u32::try_from(errors.len()).unwrap_or(u32::MAX),
        first_error_line: error_line(lines, first, len),
    }
}

/// The line of the byte at `offset` in a text of `len` bytes, an offset past the end being held to the last byte.
fn error_line(lines: &LineIndex, offset: usize, len: usize) -> Line {
    let last_byte = len.saturating_sub(1);
    lines.line(u32::try_from(offset.min(last_byte)).unwrap_or(u32::MAX))
}

/// A name assigned a call, `router = APIRouter(prefix="/bookings")`. Rules written for a receiver read it to tell
/// what the receiver was made by.
struct Binding<'a> {
    callee: Name,
    arguments: &'a Arguments,
}

/// What encloses the statement being read.
enum Frame<'a> {
    Function,
    Class(ClassFrame<'a>),
}

struct ClassFrame<'a> {
    name: &'a str,
    /// The decorators of the class by dotted name, with their arguments when they are called.
    decorators: Vec<(Name, Option<&'a Arguments>)>,
    bases: Vec<Name>,
    /// What a settings class puts before the name of each field.
    env_prefix: String,
}

/// Walks the syntax tree once, in source order, and collects everything the file holds.
struct Collector<'a> {
    packs: &'a [Pack],
    lines: &'a LineIndex,
    facts: FileFacts,
    extra: PyExtra,
    imports: Imports<'a>,
    flags: ImportFlags,
    frames: Vec<Frame<'a>>,
    /// Names assigned a call anywhere in the file. A name assigned twice keeps its last assignment: the file is
    /// read as one scope, which is how a module-level router or client is used.
    bindings: HashMap<String, Binding<'a>>,
    /// The buffers the name of each call and subscript is read into, so that none of them allocates.
    scratch: Name,
}

impl<'a> Collector<'a> {
    fn new(packs: &'a [Pack], lines: &'a LineIndex) -> Self {
        Self {
            packs,
            lines,
            facts: FileFacts::default(),
            extra: PyExtra::default(),
            imports: Imports::default(),
            flags: ImportFlags::NONE,
            frames: Vec::new(),
            bindings: HashMap::new(),
            scratch: Name::default(),
        }
    }

    /// `package_init` tells a package's `__init__`, whose imports `link` follows a router through when another file
    /// mounts the name the package gives it.
    fn finish(mut self, package_init: bool) -> Parsed<PyExtra> {
        // The prefix of a router matters to every file that makes one, because another file may fill it with routes or
        // mount it; the binds matter to a file that mounts something and to the file a router is re-exported from.
        if self
            .bindings
            .values()
            .any(|binding| is_router_type(&binding.callee))
        {
            self.extra.routers = self.router_prefixes();
        }
        if package_init || !self.extra.mounts.is_empty() {
            self.extra.binds = self.imports.binds();
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

    fn import(&mut self, stmt: &'a StmtImport) {
        let line = self.line_of(stmt.start());
        for alias in &stmt.names {
            let spec = alias.name.as_str();
            self.push_import(Import::new(spec, ImportKind::Static, line));
            let asname = alias.asname.as_ref().map(Identifier::as_str);
            self.imports.module(spec, asname, line);
        }
    }

    fn import_from(&mut self, stmt: &'a StmtImportFrom) {
        let line = self.line_of(stmt.start());
        let module = stmt.module.as_ref().map_or("", Identifier::as_str);
        let spec = specifier(stmt.level, module);
        let mut import = Import::new(spec.as_str(), ImportKind::Static, line);
        // `from m import a as b, a as c` takes `a` from the module once.
        for alias in &stmt.names {
            let name = alias.name.as_str();
            if !import.names.iter().any(|seen| seen == name) {
                import.names.push(name.to_owned());
            }
        }
        if import.names.iter().any(|name| name.as_str() == "*") {
            import.kind = ImportKind::ReExportAll;
        }
        self.push_import(import);
        for alias in stmt.names.iter().filter(|alias| alias.name.as_str() != "*") {
            let asname = alias.asname.as_ref().map(Identifier::as_str);
            self.imports
                .name(module, stmt.level, alias.name.as_str(), asname, line);
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
            name: class.name.as_str(),
            decorators: class
                .decorator_list
                .iter()
                .filter_map(|decorator| decorator_parts(&decorator.expression, &self.imports))
                .collect(),
            bases: arguments.map_or_else(Vec::new, |arguments| {
                arguments
                    .args
                    .iter()
                    .filter_map(|base| Name::of(base, &self.imports))
                    .collect()
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
            Expr::Call(call) => (Some(call), Name::of(&call.func, &self.imports)),
            other => (None, Name::of(other, &self.imports)),
        };
        if let Some(name) = name {
            let receiver = receiver_of(name.written());
            let packs = self.packs;
            for pack in packs {
                for rule in &pack.decorators {
                    let Some(matched) = name.matching(&rule.decorator) else {
                        continue;
                    };
                    let site = CallSite {
                        callee: matched,
                        arguments: call.map(|call| &call.arguments),
                        member: Some(member),
                    };
                    let mut prefix = None;
                    if let Some(class) = &rule.class {
                        let Some(read) = self.class_prefix(class) else {
                            continue;
                        };
                        prefix = Some(read);
                    }
                    let mut router = None;
                    if let Some(maker) = &rule.receiver {
                        let Some(read) = self.receiver_prefix(maker, receiver) else {
                            continue;
                        };
                        if self.is_router(receiver) {
                            // Whether a router's own prefix applies depends on how the router is mounted, and
                            // `link` knows that; the route keeps the path it was written with.
                            prefix = Some(prefix.unwrap_or_default());
                            router = receiver;
                        } else {
                            prefix = Some(match prefix {
                                Some(outer) => join_prefixes(&rule.kind, &outer, &read),
                                None => read,
                            });
                        }
                    }
                    let outcome =
                        rule.apply(&pack.name, &site, prefix.as_deref(), line, name.written());
                    self.record(outcome, router);
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
        frame.decorators.iter().find_map(|(callee, arguments)| {
            let matched = callee.matching(&class.from)?;
            Some(class.read(&CallSite {
                callee: matched,
                arguments: *arguments,
                member: None,
            }))
        })
    }

    /// The prefix a rule's `receiver` reads from the call that made `var`, when `var` is a name of this file made
    /// by one of the callees the rule names.
    fn receiver_prefix(&self, maker: &Prefix, var: Option<&str>) -> Option<String> {
        let binding = self.bindings.get(var?)?;
        prefix_made_by(maker, &binding.callee, binding.arguments)
    }

    /// Whether `var` is a name of this file made by a router constructor, as opposed to an application or a client.
    fn is_router(&self, var: Option<&str>) -> bool {
        var.and_then(|var| self.bindings.get(var))
            .is_some_and(|binding| is_router_type(&binding.callee))
    }

    /// The prefix each router variable of the file declares in its own constructor, read the way the packs read the
    /// receiver of a route. A router without a prefix is left out, which `link` reads as the empty prefix.
    fn router_prefixes(&self) -> Vec<RouterPrefix> {
        let makers: Vec<&Prefix> = self
            .packs
            .iter()
            .flat_map(|pack| {
                let decorators = pack.decorators.iter().map(|rule| rule.receiver.as_ref());
                let calls = pack.calls.iter().map(|rule| rule.receiver.as_ref());
                decorators.chain(calls).flatten()
            })
            .collect();
        let mut routers: Vec<RouterPrefix> = self
            .bindings
            .iter()
            .filter(|(_, binding)| is_router_type(&binding.callee))
            .filter_map(|(var, binding)| {
                let prefix = makers
                    .iter()
                    .find_map(|maker| prefix_made_by(maker, &binding.callee, binding.arguments))?;
                (!prefix.is_empty()).then(|| RouterPrefix {
                    var: var.clone(),
                    prefix,
                })
            })
            .collect();
        // The map has no order, and the cache entry has to be the same for the same file.
        routers.sort_by(|a, b| a.var.cmp(&b.var));
        routers
    }

    /// Keeps what a rule made of a site. A route a rule finds on `router` is not a fact of the file: `link` serves
    /// it behind the prefixes of the mounts above the router.
    fn record(&mut self, outcome: Outcome, router: Option<&str>) {
        match outcome {
            Outcome::Fact(fact) => match router {
                Some(var)
                    if matches!(fact.role, Role::Serves)
                        && matches!(fact.key.kind(), ContractKind::Http) =>
                {
                    self.extra.routes.push(Route {
                        var: var.to_owned(),
                        fact,
                    });
                }
                _ => self.facts.contracts.push(fact),
            },
            Outcome::Blind(site) => self.facts.unresolved.push(site),
            Outcome::Skip => {}
        }
    }

    /// Reads `expr` as a name into the collector's buffers and hands it to `read`. A call or a subscript on anything
    /// else, `make()()` or `(a or b)()`, has no name to match.
    fn named(&mut self, expr: &Expr, read: impl FnOnce(&mut Self, &Name)) {
        let mut name = std::mem::take(&mut self.scratch);
        if name.read(expr, &self.imports) {
            read(self, &name);
        }
        self.scratch = name;
    }

    fn call(&mut self, call: &ExprCall) {
        self.named(&call.func, |this, callee| this.read_call(call, callee));
    }

    fn read_call(&mut self, call: &ExprCall, callee: &Name) {
        let line = self.line_of(call.start());
        if self.dynamic_import(call, callee, line) || self.mount(call, callee, line) {
            return;
        }
        let packs = self.packs;
        for pack in packs {
            for rule in &pack.calls {
                let Some(matched) = callee.matching(&rule.callee) else {
                    continue;
                };
                let site = CallSite {
                    callee: matched,
                    arguments: Some(&call.arguments),
                    member: None,
                };
                let prefix = match &rule.receiver {
                    Some(maker) => {
                        let read = match inline_maker(&call.func, &self.imports) {
                            Some((made, arguments)) => prefix_made_by(maker, &made, arguments),
                            None => self.receiver_prefix(maker, receiver_of(callee.written())),
                        };
                        let Some(read) = read else {
                            continue;
                        };
                        // A client made with no base URL adds nothing, so its calls are read as the module-level
                        // ones are, which is what tells a URL with no literal path.
                        (!read.is_empty()).then_some(read)
                    }
                    None => None,
                };
                let outcome =
                    rule.apply(&pack.name, &site, prefix.as_deref(), line, callee.written());
                // A route a call declares, Django's `path()`, belongs to the module's URL list, which an
                // including module mounts like a router.
                self.record(outcome, Some(""));
            }
            for rule in &pack.embeds {
                let Some(matched) = callee.matching(&rule.callee) else {
                    continue;
                };
                let site = CallSite {
                    callee: matched,
                    arguments: Some(&call.arguments),
                    member: None,
                };
                // The document starts where the text the rule read does, which is not the line of the call when
                // the argument is on a line of its own.
                let embed = EmbedSite::new(site, self.lines, line);
                match rule.apply(&pack.name, &embed, line, callee.written()) {
                    Ok(mut document) => {
                        document.line = embed.line();
                        self.facts.embedded.push(document);
                    }
                    Err(site) => {
                        if leaves_unread(rule, matched, call.arguments.find_positional(0)) {
                            self.facts.unresolved.push(site);
                        }
                    }
                }
            }
        }
    }

    /// `importlib.import_module("a.b")` and `__import__("a.b")`. Returns whether the call was one.
    fn dynamic_import(&mut self, call: &ExprCall, callee: &Name, line: Line) -> bool {
        let imported = callee.imported();
        if !matches!(
            imported,
            "importlib.import_module" | "import_module" | "__import__"
        ) {
            return false;
        }
        match call.arguments.find_argument_value("name", 0) {
            Some(Expr::StringLiteral(name)) => {
                match dynamic_spec(call, imported, name.value.to_str()) {
                    Some(spec) => self.push_import(Import::new(spec, ImportKind::Dynamic, line)),
                    None => self.unresolved(line, callee.written()),
                }
            }
            Some(_) => self.unresolved(line, callee.written()),
            None => {}
        }
        true
    }

    /// A call that mounts a router or an included URL module under a prefix. Returns whether the call was one.
    fn mount(&mut self, call: &ExprCall, callee: &Name, line: Line) -> bool {
        // The function is told by the name its import gives it, `path` for `p` after `from django.urls import path as
        // p`; the router the mount is called on is read as written, because `link` follows it through the binds.
        let called = callee.imported();
        let last = called.rsplit('.').next().unwrap_or(called);
        match last {
            "include_router" if self.has_pack("fastapi") => {
                self.router_mount(call, callee.written(), line, ("router", "prefix"), false)
            }
            // Flask replaces the blueprint's own `url_prefix` with the registration's; FastAPI puts it in front.
            "register_blueprint" if self.has_pack("flask") => self.router_mount(
                call,
                callee.written(),
                line,
                ("blueprint", "url_prefix"),
                true,
            ),
            "path" | "re_path" | "url" if self.has_pack("django") => self.include_mount(call, line),
            _ => false,
        }
    }

    /// `app.include_router(router, prefix="/api")`; `keywords` name the router's and the prefix's keyword, and
    /// `replaces` tells a prefix that stands in for the router's own from one that goes in front of it.
    fn router_mount(
        &mut self,
        call: &ExprCall,
        callee: &str,
        line: Line,
        keywords: (&str, &str),
        replaces: bool,
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
        let mut prefix = None;
        let mut unread_prefix = false;
        if let Some(read) = site.keyword(keywords.1) {
            prefix = Some(read.into_owned());
        } else if call.arguments.find_keyword(keywords.1).is_some() {
            // A prefix held in a variable: the router is still mounted, but not at a path that is known.
            self.unresolved(line, callee);
            unread_prefix = true;
        }
        self.extra.mounts.push(Mount {
            line,
            receiver: receiver.to_owned(),
            target: Target::Name(target),
            prefix,
            unread_prefix,
            replaces,
        });
        true
    }

    /// `path("api/", include("bookings.urls"))`. Returns whether the call included something.
    fn include_mount(&mut self, call: &ExprCall, line: Line) -> bool {
        let Some(Expr::Call(include)) = call.arguments.find_positional(1) else {
            return false;
        };
        let includes = Name::of(&include.func, &self.imports)
            .is_some_and(|name| name.imported().rsplit('.').next() == Some("include"));
        if !includes {
            return false;
        }
        let mut prefix = None;
        let mut unread_prefix = false;
        if let Some(route) = call.arguments.find_positional(0).and_then(text_of) {
            // The anchors of a `re_path` pattern are not part of the path.
            let route = route.strip_prefix('^').unwrap_or(&route);
            prefix = Some(route.strip_suffix('$').unwrap_or(route).to_owned());
        } else {
            self.unresolved(line, "path");
            unread_prefix = true;
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
                    unread_prefix,
                    replaces: false,
                });
            }
            None => self.unresolved(line, "include"),
        }
        true
    }

    fn subscript(&mut self, subscript: &ExprSubscript) {
        self.named(&subscript.value, |this, root| {
            this.read_subscript(subscript, root);
        });
    }

    fn read_subscript(&mut self, subscript: &ExprSubscript, root: &Name) {
        let segment = match &*subscript.slice {
            Expr::StringLiteral(name) => Some(name.value.to_str()),
            _ => None,
        };
        let line = self.line_of(subscript.start());
        let packs = self.packs;
        for pack in packs {
            for rule in &pack.chains {
                if root.matching(&rule.root).is_some() {
                    let outcome = rule.apply(&pack.name, &[segment], line, root.written());
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
        if let (Some(var), Some(callee)) = (dotted(target), Name::of(&call.func, &self.imports)) {
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

    /// Whether the name assigned here matches one of the `patterns` of a rule.
    ///
    /// A bare name in a class body is also matched as qualified by the classes around it, as many as the pattern has
    /// segments before its last, so `*.Meta.db_table` reads a `db_table` in a class `Meta` that sits in another class
    /// and `*.__tablename__` a `__tablename__` in the body of a class. A pattern of one segment matches the name
    /// wherever it is assigned.
    fn target_matches(&self, patterns: &OneOrMany, name: &str) -> bool {
        patterns.iter().any(|pattern| {
            if name.contains('.') {
                return callee_matches(pattern, name);
            }
            let mut segments = pattern.rsplit('.');
            if !segments
                .next()
                .is_some_and(|last| last == "*" || last == name)
            {
                return false;
            }
            let mut classes = self.frames.iter().rev();
            segments.all(|segment| match classes.next() {
                Some(Frame::Class(class)) => segment == "*" || segment == class.name,
                _ => false,
            })
        })
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
                if !self.target_matches(&rule.target, &name) {
                    continue;
                }
                let class = self.top_class();
                if let Some(bases) = &rule.class
                    && !class.is_some_and(|class| {
                        class
                            .bases
                            .iter()
                            .any(|base| base.matching(bases).is_some())
                    })
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

/// Whether a call whose document `rule` could not read leaves something unread, given the call's first argument.
///
/// A string built at run time is a document that cannot be read, whoever calls. For the rest the callee tells:
/// `text(query)` can only hold a statement, but `pool.execute(task)` matched `*.execute` by its name alone and may be
/// no SQL at all, so a rule matched through a wildcard stays silent about a name, an attribute or a subscript. A call
/// holds a document or a query object of its own, read where it is (`conn.execute(text("…"))`,
/// `session.execute(select(…))`), and a call with no argument holds nothing.
fn leaves_unread(rule: &EmbedRule, matched: &str, argument: Option<&Expr>) -> bool {
    let Some(argument) = argument else {
        return false;
    };
    if is_string_shaped(argument) {
        return true;
    }
    !matches!(argument, Expr::Call(_)) && rule.callee.iter().any(|pattern| pattern == matched)
}

/// The prefix a rule's `receiver` reads from the call of `callee` that made the receiver, when the rule names that
/// callee.
fn prefix_made_by(maker: &Prefix, callee: &Name, arguments: &Arguments) -> Option<String> {
    let matched = callee.matching(&maker.from)?;
    Some(maker.read(&CallSite {
        callee: matched,
        arguments: Some(arguments),
        member: None,
    }))
}

/// The call a method is called on where it is written, `httpx.Client(base_url="…")` of
/// `httpx.Client(base_url="…").get(…)`: a receiver that no name holds.
fn inline_maker<'e>(func: &'e Expr, imports: &Imports<'_>) -> Option<(Name, &'e Arguments)> {
    let Expr::Attribute(attribute) = func else {
        return None;
    };
    let Expr::Call(made) = &*attribute.value else {
        return None;
    };
    Some((Name::of(&made.func, imports)?, &made.arguments))
}

/// Whether a callee builds an object that routes are declared on and that is mounted somewhere else, as opposed to
/// an application, which nothing mounts.
fn is_router_type(callee: &Name) -> bool {
    matches!(
        callee.imported().rsplit('.').next(),
        Some("APIRouter" | "Blueprint")
    )
}

/// The part of a dotted callee before its last segment: `router` of `router.get`, `self.client` of
/// `self.client.get`. `None` for a bare name.
fn receiver_of(callee: &str) -> Option<&str> {
    callee.rsplit_once('.').map(|(receiver, _)| receiver)
}

/// A decorator as `(dotted name, arguments when called)`.
fn decorator_parts<'e>(
    expression: &'e Expr,
    imports: &Imports<'_>,
) -> Option<(Name, Option<&'e Arguments>)> {
    match expression {
        Expr::Call(call) => Name::of(&call.func, imports).map(|name| (name, Some(&call.arguments))),
        other => Name::of(other, imports).map(|name| (name, None)),
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

/// The specifier `importlib.import_module(name, package)` imports. A relative name is taken from the literal `package`;
/// with no package, or with `__package__`, it is taken from the package of the file, as a relative import is. `None`
/// when the package is only known at run time or the name climbs out of it. `__import__` reads the package a
/// relative name starts from out of `globals` and not out of an argument, so its name stays as written.
fn dynamic_spec(call: &ExprCall, imported: &str, name: &str) -> Option<String> {
    if imported == "__import__" || !name.starts_with('.') {
        return Some(name.to_owned());
    }
    match call.arguments.find_argument_value("package", 1) {
        None => Some(name.to_owned()),
        Some(package) if is_name(package, "__package__") => Some(name.to_owned()),
        Some(Expr::StringLiteral(package)) => absolute_name(name, package.value.to_str()),
        Some(_) => None,
    }
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
