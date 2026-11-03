//! What the functions, blocks, loops, catch clauses and class bodies of a file bind below its top level, found by position so that every reader of the constants answers for the scope a name stands in without keeping a scope of its own.

use std::collections::BTreeMap;
use std::mem;

use oxc_ast::ast::{
    ArrowFunctionExpression, BindingIdentifier, BindingPattern, BlockStatement, CatchClause, Class,
    Expression, ForInStatement, ForOfStatement, ForStatement, FormalParameter, FormalParameters,
    Function, FunctionBody, MethodDefinition, ObjectProperty, Program, Statement, StaticBlock,
    SwitchStatement, TSInterfaceDeclaration, TSModuleBlock, TSType, TSTypeAliasDeclaration,
    TSTypeParameterDeclaration, VariableDeclaration, VariableDeclarationKind, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::{GetSpan, Span};
use oxc_syntax::scope::ScopeFlags;

use super::super::jsdoc::{Block, Docs};
use super::super::roots::{declared_class, plain_class};
use super::{Constants, Home, Local, Locals, may_hold_text, rewrite};

/// Records in `constants` what each scope of `program` below its top level binds: first every name as hidden, then the constants among them, read the way those of the top level are. Two walks, because an initializer reads a name through the scopes around it, and one of them may bind that name further down. Drops the names that cannot change an answer, so that a file without a local of interest pays one check per lookup.
pub(super) fn collect(constants: &mut Constants, program: &Program<'_>, docs: Option<&Docs>) {
    Scopes {
        constants,
        docs,
        stack: Vec::new(),
        hoist: false,
        class: None,
        pending: None,
        documented: None,
        pattern_docs: None,
    }
    .visit_program(program);
    constants.keep_locals(|locals| locals.constant || locals.held);
    for locals in constants.locals.values_mut() {
        locals.index();
    }
    if constants.locals.values().any(|locals| locals.constant) {
        Initializers {
            constants,
            path: String::new(),
        }
        .visit_program(program);
        constants.keep_locals(|locals| {
            locals.held || locals.scopes.iter().any(|local| !local.values.is_empty())
        });
    }
}

impl Constants {
    /// Keeps the names that `matters` says so of, and those that hide something of the file: a constant or an import. Any other name that a scope binds can change no answer.
    fn keep_locals(&mut self, matters: impl Fn(&Locals) -> bool) {
        let Self {
            roots,
            imported,
            heads,
            locals,
            ..
        } = self;
        locals.retain(|name, locals| {
            matters(locals)
                || roots.contains(name)
                || imported.contains_key(name)
                || heads.contains(name)
        });
    }
}

/// The binding and the initializer of `declarator` when, in a declaration of `kind`, it binds one name to a constant that can hold text.
fn text_constant<'d, 'a>(
    kind: VariableDeclarationKind,
    declarator: &'d VariableDeclarator<'a>,
) -> Option<(&'d BindingIdentifier<'a>, &'d Expression<'a>)> {
    if kind != VariableDeclarationKind::Const {
        return None;
    }
    let BindingPattern::BindingIdentifier(id) = &declarator.id else {
        return None;
    };
    let init = declarator.init.as_ref()?;
    may_hold_text(init).then_some((&**id, init))
}

/// The second walk: reads the constants of the scopes in source order, once every binding is known.
struct Initializers<'c> {
    constants: &'c mut Constants,
    /// The buffer that the name of a constant is written in.
    path: String,
}

impl<'a> Visit<'a> for Initializers<'_> {
    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        for declarator in &it.declarations {
            if let Some((id, init)) = text_constant(it.kind, declarator) {
                let (name, at) = (id.name.as_str(), id.span.start);
                // The innermost scope that binds the name around its own declaration is the one that declares it. A constant of the top level has none.
                if self
                    .constants
                    .local(name, at)
                    .is_some_and(|local| !local.rebound)
                {
                    rewrite(&mut self.path, &[name]);
                    self.constants
                        .add(Home::Local(name, at), &mut self.path, init);
                }
            }
        }
        walk::walk_variable_declaration(self, it);
    }

    /// A type declares no constant, and it is most of what a typed file holds.
    fn visit_ts_type(&mut self, _it: &TSType<'a>) {}
}

/// A scope the walk is in.
struct Frame {
    span: Span,
    /// Where a `var` is bound: a function, a class static block or a namespace. A block, a loop and a catch clause are not.
    function: bool,
}

struct Scopes<'c> {
    constants: &'c mut Constants,
    /// What the `JSDoc` comments of a JavaScript file say about types.
    docs: Option<&'c Docs>,
    /// The scopes around the walk, the outermost first. Empty at the top level of the file, which is no scope below it.
    stack: Vec<Frame>,
    /// Whether the names being walked belong to a `var`, which a block does not close.
    hoist: bool,
    /// The class that the next name bound holds, set by the declarator or the parameter being walked.
    class: Option<Box<str>>,
    /// The `@param` comment above the statement, method or property being walked, until the next parameter list takes it.
    pending: Option<&'c Block>,
    /// The `@param` comment of the parameter list being walked.
    documented: Option<&'c Block>,
    /// The `@param` comment, while the walk is in a destructuring pattern of a parameter: the names in it are typed by it.
    pattern_docs: Option<&'c Block>,
}

impl Scopes<'_> {
    /// Walks inside a new scope. Nothing walked there takes the names of a `var` that is being declared, nor those of a destructured parameter.
    fn within(&mut self, span: Span, function: bool, body: impl FnOnce(&mut Self)) {
        let hoist = mem::take(&mut self.hoist);
        let pattern_docs = self.pattern_docs.take();
        self.stack.push(Frame { span, function });
        body(self);
        self.stack.pop();
        self.hoist = hoist;
        self.pattern_docs = pattern_docs;
    }

    /// The comment above the token that starts at `at`, when it documents parameters, for the next parameter list.
    fn expect_params(&mut self, at: u32) {
        self.pending = self
            .docs
            .and_then(|docs| docs.block_at(at))
            .filter(|block| block.has_params());
    }

    /// Notes that the scope around binds `name`: as hidden, and as `constant` when an initializer may give it text, which [`Initializers`] reads. A second binding of the name in the scope leaves it standing for nothing there. The top level of the file is no scope below it, so nothing is noted there.
    fn bind(&mut self, name: &str, hoist: bool, constant: bool, class: Option<Box<str>>) {
        let mut frames = self.stack.iter().rev();
        let frame = if hoist {
            frames.find(|frame| frame.function)
        } else {
            frames.next()
        };
        let Some(scope) = frame.map(|frame| frame.span) else {
            return;
        };
        // A name that a scope bound before is not copied again, and a minified bundle binds the same few names in every function.
        let locals = match self.constants.locals.get_mut(name) {
            Some(locals) => locals,
            None => self.constants.locals.entry(name.to_owned()).or_default(),
        };
        locals.constant |= constant;
        locals.held |= class.is_some();
        // The walk is in source order, so a scope that ended before this one started was noted before any binding of this one: stopping there keeps a name bound in every function of a minified bundle linear.
        if let Some(known) = locals
            .scopes
            .iter_mut()
            .rev()
            .take_while(|local| local.scope.end > scope.start)
            .find(|local| local.scope == scope)
        {
            known.rebound = true;
            return;
        }
        locals.scopes.push(Local {
            scope,
            values: BTreeMap::new(),
            rebound: false,
            class,
        });
    }

    /// The class that the `@type` above a declaration of one name gives to it.
    fn documented_value(&self, it: &VariableDeclaration<'_>) -> Option<Box<str>> {
        let [declarator] = it.declarations.as_slice() else {
            return None;
        };
        if !matches!(declarator.id, BindingPattern::BindingIdentifier(_)) {
            return None;
        }
        let class = self.docs?.block_at(it.span.start)?.declared_class()?;
        Some(Box::from(class))
    }

    /// Walks the parameters of a function and its body, the body in a scope inside that of the parameters: a default value does not see what the body declares.
    fn parameters_and_body<'a>(
        &mut self,
        parameters: &FormalParameters<'a>,
        body: &FunctionBody<'a>,
    ) {
        self.visit_formal_parameters(parameters);
        self.within(body.span, true, |scopes| scopes.visit_function_body(body));
    }
}

impl<'a> Visit<'a> for Scopes<'_> {
    /// Every name that no visitor below classifies is bound to something other than text: a parameter, a destructured name, a function, a class, a catch clause.
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        let name = it.name.as_str();
        let class = self.class.take().or_else(|| {
            let class = self.pattern_docs?.param_class(name, true)?;
            Some(Box::from(class))
        });
        self.bind(name, self.hoist, false, class);
    }

    /// The comment above a statement documents the parameters of the first function in it.
    fn visit_statement(&mut self, it: &Statement<'a>) {
        self.expect_params(it.span().start);
        walk::walk_statement(self, it);
        self.pending = None;
    }

    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        self.expect_params(it.span.start);
        walk::walk_method_definition(self, it);
        self.pending = None;
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        self.expect_params(it.span.start);
        walk::walk_object_property(self, it);
        self.pending = None;
    }

    /// The parameters of a list are documented by the comment that was waiting for it, and those of a function in a default value by none.
    fn visit_formal_parameters(&mut self, it: &FormalParameters<'a>) {
        let outer = mem::replace(&mut self.documented, self.pending.take());
        walk::walk_formal_parameters(self, it);
        self.documented = outer;
    }

    /// A parameter with a plain class type holds that class, with or without a default value, and so does one that a `@param` documents with a class. A destructured one types the names in it by the path they are documented under.
    fn visit_formal_parameter(&mut self, it: &FormalParameter<'a>) {
        self.visit_decorators(&it.decorators);
        match &it.pattern {
            BindingPattern::BindingIdentifier(id) => {
                self.class = it
                    .type_annotation
                    .as_deref()
                    .and_then(plain_class)
                    .map(|class| Box::from(&*class))
                    .or_else(|| {
                        let class = self.documented?.param_class(id.name.as_str(), false)?;
                        Some(Box::from(class))
                    });
                self.visit_binding_pattern(&it.pattern);
            }
            pattern => {
                self.pattern_docs = self.documented;
                self.visit_binding_pattern(pattern);
                self.pattern_docs = None;
            }
        }
        self.class = None;
        if let Some(type_annotation) = &it.type_annotation {
            self.visit_ts_type_annotation(type_annotation);
        }
        if let Some(initializer) = &it.initializer {
            self.visit_expression(initializer);
        }
    }

    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        for declarator in &it.declarations {
            let class = (it.kind == VariableDeclarationKind::Const
                && matches!(declarator.id, BindingPattern::BindingIdentifier(_)))
            .then(|| declared_class(declarator, |name, _| self.constants.heads.contains(name)))
            .flatten()
            .map(|class| Box::from(&*class))
            .or_else(|| self.documented_value(it));
            if let Some((id, _)) = text_constant(it.kind, declarator) {
                self.bind(id.name.as_str(), false, true, class);
            } else {
                self.class = class;
                self.hoist = it.kind == VariableDeclarationKind::Var;
                self.visit_binding_pattern(&declarator.id);
                self.class = None;
                self.hoist = false;
            }
            if let Some(init) = &declarator.init {
                self.visit_expression(init);
            }
        }
    }

    /// An overload, a `declare function` and an abstract method have no body, and the parameters of a signature without a body bind nothing.
    fn visit_function(&mut self, it: &Function<'a>, _flags: ScopeFlags) {
        let Some(body) = &it.body else {
            return;
        };
        // A declaration binds its name where it stands, and an expression only inside itself.
        if it.is_function_declaration()
            && let Some(id) = &it.id
        {
            self.bind(id.name.as_str(), false, false, None);
        }
        self.within(it.span, true, |scopes| {
            if it.is_expression()
                && let Some(id) = &it.id
            {
                scopes.bind(id.name.as_str(), false, false, None);
            }
            scopes.parameters_and_body(&it.params, body);
        });
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.within(it.span, true, |scopes| {
            match it.body.as_function_body() {
                Some(body) => scopes.parameters_and_body(&it.params, body),
                // An expression binds nothing of its own.
                None => walk::walk_arrow_function_expression(scopes, it),
            }
        });
    }

    /// The name of a class expression is visible inside the class only, and that of a declaration where the class stands.
    fn visit_class(&mut self, it: &Class<'a>) {
        if it.is_expression() && it.id.is_some() {
            self.within(it.span, false, |scopes| walk::walk_class(scopes, it));
        } else {
            walk::walk_class(self, it);
        }
    }

    fn visit_block_statement(&mut self, it: &BlockStatement<'a>) {
        self.within(it.span, false, |scopes| {
            walk::walk_block_statement(scopes, it);
        });
    }

    fn visit_for_statement(&mut self, it: &ForStatement<'a>) {
        self.within(it.span, false, |scopes| {
            walk::walk_for_statement(scopes, it);
        });
    }

    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        self.within(it.span, false, |scopes| {
            walk::walk_for_in_statement(scopes, it);
        });
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        self.within(it.span, false, |scopes| {
            walk::walk_for_of_statement(scopes, it);
        });
    }

    fn visit_catch_clause(&mut self, it: &CatchClause<'a>) {
        self.within(it.span, false, |scopes| {
            walk::walk_catch_clause(scopes, it);
        });
    }

    /// All the cases share one scope.
    fn visit_switch_statement(&mut self, it: &SwitchStatement<'a>) {
        self.within(it.span, false, |scopes| {
            walk::walk_switch_statement(scopes, it);
        });
    }

    fn visit_static_block(&mut self, it: &StaticBlock<'a>) {
        self.within(it.span, true, |scopes| {
            walk::walk_static_block(scopes, it);
        });
    }

    fn visit_ts_module_block(&mut self, it: &TSModuleBlock<'a>) {
        self.within(it.span, true, |scopes| {
            walk::walk_ts_module_block(scopes, it);
        });
    }

    /// A type binds no name of the file: the parameters of its function types and the keys of its mapped types only describe.
    fn visit_ts_type(&mut self, _it: &TSType<'a>) {}

    /// The name of a type parameter belongs to the types.
    fn visit_ts_type_parameter_declaration(&mut self, _it: &TSTypeParameterDeclaration<'a>) {}

    fn visit_ts_type_alias_declaration(&mut self, _it: &TSTypeAliasDeclaration<'a>) {}

    /// The members of an interface are signatures without a body.
    fn visit_ts_interface_declaration(&mut self, _it: &TSInterfaceDeclaration<'a>) {}
}
