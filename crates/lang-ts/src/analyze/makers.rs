//! The variables a file makes with a call, found before the walk: `const app = express()`, `this.http = axios.create()`.

use std::collections::BTreeMap;

use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::rules::Pack;
use oxc_ast::ast::{
    Argument, AssignmentExpression, AssignmentTarget, BindingPattern, Expression, Program,
    PropertyDefinition, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};

use super::Constants;
use super::peel;
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
    /// Some pack serves HTTP routes on receivers made by this callee, so the variable is a node mounts connect.
    pub(super) serves: bool,
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
        found: BTreeMap::new(),
    };
    collector.visit_program(program);
    collector.found
}

struct Collector<'p> {
    packs: &'p [Pack],
    constants: &'p Constants,
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
        let serves = self.packs.iter().flat_map(|pack| &pack.calls).any(|rule| {
            rule.kind == ContractKind::Http
                && rule.role == Role::Serves
                && rule
                    .receiver
                    .as_ref()
                    .is_some_and(|receiver| receiver.from.matches(&callee))
        });
        let shape = Shape::of_call(&callee, arguments.iter(), self.constants);
        let root = match arguments.first().and_then(Argument::as_expression) {
            Some(Expression::Identifier(first)) if callee == NEST_FACTORY_CREATE => {
                Some(first.name.as_str().to_owned())
            }
            _ => None,
        };
        self.found.insert(
            name,
            Maker {
                callee,
                shape,
                root,
                serves,
            },
        );
    }
}

impl<'a> Visit<'a> for Collector<'_> {
    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let (BindingPattern::BindingIdentifier(id), Some(init)) = (&it.id, &it.init) {
            self.add(id.name.as_str().to_owned(), init);
        }
        walk::walk_variable_declarator(self, it);
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
