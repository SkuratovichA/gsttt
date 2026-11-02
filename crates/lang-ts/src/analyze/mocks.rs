//! Whether the factory of a `vi.mock` or `jest.mock` call stands in for its module.

use oxc_ast::ast::{Argument, BindingPattern, Expression, FormalParameters, IdentifierReference};
use oxc_ast_visit::Visit;

/// Whether `factory`, the second argument of a mock call, replaces the module: an arrow function or a function expression that never uses its first parameter, the `importOriginal` of a partial mock. One without a parameter replaces it.
///
/// A first parameter that is a rest element or a pattern binds the argument where this check cannot follow it, so it counts as used. So does any reference to the name, also one that a nested function shadows, and `arguments` in a function expression: the answer errs towards keeping a test.
pub(super) fn replaces(factory: Option<&Argument<'_>>) -> bool {
    let Some(expression) = factory.and_then(Argument::as_expression) else {
        return false;
    };
    let mut uses = Uses {
        first: "",
        has_arguments: false,
        found: false,
    };
    match expression.get_inner_expression() {
        Expression::ArrowFunctionExpression(arrow) => {
            let Some(first) = first_parameter(&arrow.params) else {
                return false;
            };
            uses.first = first;
            uses.visit_formal_parameters(&arrow.params);
            uses.visit_arrow_function_body(&arrow.body);
        }
        Expression::FunctionExpression(function) => {
            let Some(first) = first_parameter(&function.params) else {
                return false;
            };
            uses.first = first;
            uses.has_arguments = true;
            uses.visit_formal_parameters(&function.params);
            if let Some(body) = &function.body {
                uses.visit_function_body(body);
            }
        }
        _ => return false,
    }
    !uses.found
}

/// The name of the first parameter, `None` when it is a rest element or a pattern, which binds the argument where a reference cannot be followed, and an empty name when there is no parameter at all, which nothing can refer to.
fn first_parameter<'p>(params: &'p FormalParameters<'_>) -> Option<&'p str> {
    match (params.items.first(), &params.rest) {
        (Some(first), _) => match &first.pattern {
            BindingPattern::BindingIdentifier(name) => Some(name.name.as_str()),
            _ => None,
        },
        (None, Some(_)) => None,
        (None, None) => Some(""),
    }
}

/// Looks for a reference to the first parameter of a factory.
struct Uses<'n> {
    /// The name of the first parameter, empty when there is none, which no reference can have.
    first: &'n str,
    /// Whether `arguments` is the first parameter too, as it is in a function expression.
    has_arguments: bool,
    found: bool,
}

impl<'a> Visit<'a> for Uses<'_> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        let name = it.name.as_str();
        self.found |= name == self.first || (self.has_arguments && name == "arguments");
    }
}
