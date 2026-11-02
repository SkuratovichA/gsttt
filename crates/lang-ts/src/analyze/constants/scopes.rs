//! What the functions, blocks, loops, catch clauses and class bodies of a file bind below its top level, found by position so that every reader of the constants answers for the scope a name stands in without keeping a scope of its own.

use std::collections::BTreeMap;
use std::mem;

use oxc_ast::ast::{
    ArrowFunctionExpression, BindingIdentifier, BindingPattern, BlockStatement, CatchClause, Class,
    Expression, ForInStatement, ForOfStatement, ForStatement, Function, Program, StaticBlock,
    SwitchStatement, TSInterfaceDeclaration, TSModuleBlock, TSType, TSTypeAliasDeclaration,
    TSTypeParameterDeclaration, VariableDeclaration, VariableDeclarationKind,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::Span;
use oxc_syntax::scope::ScopeFlags;

use super::{Constants, Home, Local, may_hold_text, rewrite};

/// Records in `constants` what each scope of `program` below its top level binds: the constants among it, read the way those of the top level are, and every other name as hidden. Then drops the names that cannot change an answer, so that a file without a local of interest pays one check per lookup.
pub(super) fn collect(constants: &mut Constants, program: &Program<'_>) {
    let mut scopes = Scopes {
        constants,
        stack: Vec::new(),
        hoist: false,
        path: String::new(),
    };
    scopes.visit_program(program);
    let Constants {
        roots,
        imported,
        locals,
        ..
    } = scopes.constants;
    // A hidden name matters when something it hides is there: a constant or an import of the file, or a constant of a scope around it.
    locals.retain(|name, scopes| {
        roots.contains(name)
            || imported.contains_key(name)
            || scopes.iter().any(|local| !local.values.is_empty())
    });
}

/// A scope the walk is in.
struct Frame {
    span: Span,
    /// Where a `var` is bound: a function, a class static block or a namespace. A block, a loop and a catch clause are not.
    function: bool,
}

struct Scopes<'c> {
    constants: &'c mut Constants,
    /// The scopes around the walk, the outermost first. Empty at the top level of the file, which is no scope below it.
    stack: Vec<Frame>,
    /// Whether the names being walked belong to a `var`, which a block does not close.
    hoist: bool,
    /// The buffer that the name of a constant is written in.
    path: String,
}

impl Scopes<'_> {
    /// Walks inside a new scope. Nothing walked there takes the names of a `var` that is being declared.
    fn within(&mut self, span: Span, function: bool, body: impl FnOnce(&mut Self)) {
        let hoist = mem::take(&mut self.hoist);
        self.stack.push(Frame { span, function });
        body(self);
        self.stack.pop();
        self.hoist = hoist;
    }

    /// Notes that the scope around binds `name`, as nothing: a name that its initializer reads as text is filled in by [`Scopes::constant`]. True when this is the first binding of the name in the scope. A second one clears what the first read, because the name then stands for nothing in the whole scope. The top level of the file is no scope below it, so nothing is noted there.
    fn bind(&mut self, name: &str, hoist: bool) -> bool {
        let mut frames = self.stack.iter().rev();
        let frame = if hoist {
            frames.find(|frame| frame.function)
        } else {
            frames.next()
        };
        let Some(scope) = frame.map(|frame| frame.span) else {
            return false;
        };
        let locals = self.constants.locals.entry(name.to_owned()).or_default();
        if let Some(known) = locals.iter_mut().find(|local| local.scope == scope) {
            known.values.clear();
            return false;
        }
        locals.push(Local {
            scope,
            values: BTreeMap::new(),
        });
        true
    }

    /// Notes that the scope around binds `name` to `init`, which can hold text, and reads it as the top level reads a constant. It sees the scopes noted so far, and the name itself as hidden.
    fn constant(&mut self, name: &str, init: &Expression<'_>) {
        if self.bind(name, false) {
            rewrite(&mut self.path, &[name]);
            self.constants.add(Home::Local(name), &mut self.path, init);
        }
    }
}

impl<'a> Visit<'a> for Scopes<'_> {
    /// Every name that no visitor below classifies is bound to something other than text: a parameter, a destructured name, a function, a class, a catch clause.
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.bind(it.name.as_str(), self.hoist);
    }

    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        for declarator in &it.declarations {
            if it.kind == VariableDeclarationKind::Const
                && let BindingPattern::BindingIdentifier(id) = &declarator.id
                && let Some(init) = &declarator.init
                && may_hold_text(init)
            {
                self.constant(id.name.as_str(), init);
            } else {
                self.hoist = it.kind == VariableDeclarationKind::Var;
                self.visit_binding_pattern(&declarator.id);
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
            self.bind(id.name.as_str(), false);
        }
        self.within(it.span, true, |scopes| {
            if it.is_expression()
                && let Some(id) = &it.id
            {
                scopes.bind(id.name.as_str(), false);
            }
            scopes.visit_formal_parameters(&it.params);
            scopes.visit_function_body(body);
        });
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.within(it.span, true, |scopes| {
            walk::walk_arrow_function_expression(scopes, it);
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
