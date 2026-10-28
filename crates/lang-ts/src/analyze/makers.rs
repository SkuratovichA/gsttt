//! The variables a file makes with a call, found before the walk: `const app = express()`, `this.http = axios.create()`.

use std::collections::BTreeMap;

use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::rules::Pack;
use oxc_ast::ast::{
    AssignmentExpression, AssignmentTarget, BindingPattern, Expression, Program,
    PropertyDefinition, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};

use crate::shape::{Shape, chain_of, dotted, property_name};

/// A variable made by a call: what was called, the call as a rule reads it, and whether it serves routes.
#[derive(Debug)]
pub(super) struct Maker {
    pub(super) callee: String,
    pub(super) shape: Shape,
    /// Some pack serves HTTP routes on receivers made by this callee, so the variable is a node mounts connect.
    pub(super) serves: bool,
}

/// Every variable of `program` that is made by a call, by its name; a field is named `this.field`.
pub(super) fn collect(program: &Program<'_>, packs: &[Pack]) -> BTreeMap<String, Maker> {
    let mut collector = Collector {
        packs,
        found: BTreeMap::new(),
    };
    collector.visit_program(program);
    collector.found
}

struct Collector<'p> {
    packs: &'p [Pack],
    found: BTreeMap<String, Maker>,
}

impl Collector<'_> {
    fn add(&mut self, name: String, init: &Expression<'_>) {
        let (callee, arguments) = match init.get_inner_expression() {
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
        let shape = Shape::of_call(&callee, arguments.iter());
        self.found.insert(
            name,
            Maker {
                callee,
                shape,
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
            self.add(format!("this.{}", member.property.name.as_str()), &it.right);
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_property_definition(&mut self, it: &PropertyDefinition<'a>) {
        if !it.computed
            && let (Some(name), Some(value)) = (property_name(&it.key), &it.value)
        {
            self.add(format!("this.{name}"), value);
        }
        walk::walk_property_definition(self, it);
    }
}
