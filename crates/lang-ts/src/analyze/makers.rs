//! The variables a file makes with a call, found before the walk: `const app = express()`, `this.http = axios.create()`.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::rules::{Pack, serves_http};
use oxc_ast::ast::{
    Argument, AssignmentExpression, AssignmentTarget, BindingPattern, ExportDefaultDeclaration,
    Expression, Program, PropertyDefinition, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_syntax::scope::{ScopeFlags, ScopeId};

use super::constants::exported_locals;
use super::{Constants, peel};
use crate::extra::{Export, LinkMaker};
use crate::shape::{Shape, chain_of, dotted, field_name, this_key};

/// The callee that makes a NestJS app, whose root module the global prefix of the app is mounted on.
pub(super) const NEST_FACTORY_CREATE: &str = "NestFactory.create";

/// A variable made by a call: what was called, the call as a rule reads it, and whether it serves routes.
#[derive(Debug)]
pub(super) struct Maker {
    pub(super) callee: String,
    pub(super) shape: Shape,
    /// The root module of a NestJS app when it is a plain name: `AppModule` for `NestFactory.create(AppModule)`; `None` for any other call.
    pub(super) root: Option<String>,
    /// Some call rule takes the receiver of its calls from a variable made by this callee, so a file that imports the variable can have its calls read.
    pub(super) receiver: bool,
    /// Some pack serves HTTP routes on receivers made by this callee, so the variable is a node mounts connect.
    pub(super) serves: bool,
    /// The variable is declared at the top level of the module, not inside a function, a block or a class: the only kind a file can export, and the one that wins over a variable of the same name declared deeper.
    pub(super) top_level: bool,
}

/// The makers of `makers` that a call rule takes a receiver from and that the link reads, in the order of their names: those declared at the top level that `exports` name as a local, for the files that import them, and those a call of the file waits on (`waited`), whose names are declared wherever.
pub(super) fn linked(
    makers: &BTreeMap<String, Maker>,
    exports: &[Export],
    waited: &BTreeSet<&str>,
) -> Vec<LinkMaker> {
    let locals = exported_locals(exports);
    makers
        .iter()
        .filter(|(name, maker)| {
            maker.receiver
                && ((maker.top_level && locals.contains(name.as_str()))
                    || waited.contains(name.as_str()))
        })
        .map(|(name, maker)| LinkMaker {
            name: name.clone(),
            callee: maker.callee.clone(),
            shape: maker.shape.stored(),
        })
        .collect()
}

/// Every variable of `program`, whose source is `text`, that is made by a call, by its name; a field is named `this.field`. `nestjs` is whether that pack is active, whose `setGlobalPrefix` is read from the app that `NestFactory.create` made.
pub(super) fn collect(
    program: &Program<'_>,
    text: &str,
    packs: &[Pack],
    nestjs: bool,
    constants: &Constants,
) -> BTreeMap<String, Maker> {
    // Only a rule that reads a receiver, or a `setGlobalPrefix` call of a NestJS app, looks at what a variable was made by, so a file without either skips the walk.
    let needed = packs
        .iter()
        .flat_map(|pack| &pack.calls)
        .any(|rule| rule.receiver.is_some())
        || (nestjs && text.contains("setGlobalPrefix"));
    if !needed {
        return BTreeMap::new();
    }
    let mut collector = Collector {
        packs,
        constants,
        depth: 0,
        found: BTreeMap::new(),
    };
    collector.visit_program(program);
    collector.found
}

struct Collector<'p> {
    packs: &'p [Pack],
    constants: &'p Constants,
    /// How many scopes the walk is in. The program is the first, so the top level of the module is 1.
    depth: u32,
    found: BTreeMap<String, Maker>,
}

impl Collector<'_> {
    fn add(&mut self, name: String, init: &Expression<'_>) {
        let (callee, arguments) = match peel(init) {
            Expression::CallExpression(call) => (&call.callee, &call.arguments),
            Expression::NewExpression(new) => (&new.callee, &new.arguments),
            _ => return,
        };
        let Some(callee) = chain_of(callee).and_then(|names| dotted(&names)) else {
            return;
        };
        let mut receiver = false;
        let mut serves = false;
        for rule in self.packs.iter().flat_map(|pack| &pack.calls) {
            if rule
                .receiver
                .as_ref()
                .is_some_and(|receiver| receiver.from.matches(&callee))
            {
                receiver = true;
                serves |= serves_http(&rule.kind, rule.role);
            }
        }
        let shape = Shape::of_call(&callee, arguments.iter(), self.constants);
        let root = match arguments.first().and_then(Argument::as_expression) {
            Some(Expression::Identifier(first)) if callee == NEST_FACTORY_CREATE => {
                Some(first.name.as_str().to_owned())
            }
            _ => None,
        };
        // A variable declared deeper never replaces one the module declares at its top level, whichever comes first in the source.
        let top_level = self.depth <= 1;
        if !top_level && self.found.get(&name).is_some_and(|known| known.top_level) {
            return;
        }
        self.found.insert(
            name,
            Maker {
                callee,
                shape,
                root,
                receiver,
                serves,
                top_level,
            },
        );
    }
}

impl<'a> Visit<'a> for Collector<'_> {
    fn enter_scope(&mut self, _flags: ScopeFlags, _scope_id: &Cell<Option<ScopeId>>) {
        self.depth = self.depth.saturating_add(1);
    }

    fn leave_scope(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let (BindingPattern::BindingIdentifier(id), Some(init)) = (&it.id, &it.init) {
            self.add(id.name.as_str().to_owned(), init);
        }
        walk::walk_variable_declarator(self, it);
    }

    fn visit_export_default_declaration(&mut self, it: &ExportDefaultDeclaration<'a>) {
        if let Some(init) = it.declaration.as_expression() {
            self.add("default".to_owned(), init);
        }
        walk::walk_export_default_declaration(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if let AssignmentTarget::StaticMemberExpression(member) = &it.left
            && matches!(member.object, Expression::ThisExpression(_))
        {
            self.add(this_key(member.property.name.as_str()), &it.right);
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_property_definition(&mut self, it: &PropertyDefinition<'a>) {
        if let (Some(name), Some(value)) = (field_name(it), &it.value) {
            self.add(this_key(name), value);
        }
        walk::walk_property_definition(self, it);
    }
}
