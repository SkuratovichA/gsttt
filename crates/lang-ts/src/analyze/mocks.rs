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

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_ast::ast::Statement;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    use super::*;

    /// What `replaces` says of the second argument of the call `vi.mock('./b', <factory>)`.
    fn replaces_factory(factory: &str) -> bool {
        let code = format!("vi.mock('./b', {factory})");
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, &code, SourceType::ts()).parse();
        assert!(
            parsed.diagnostics.is_empty(),
            "{code}: {:?}",
            parsed.diagnostics
        );
        let Some(Statement::ExpressionStatement(statement)) = parsed.program.body.first() else {
            panic!("{code} is no statement");
        };
        let Expression::CallExpression(call) = &statement.expression else {
            panic!("{code} is no call");
        };
        replaces(call.arguments.get(1))
    }

    #[test]
    fn a_factory_that_ignores_its_parameter_replaces_the_module() {
        assert!(replaces_factory("() => ({})"));
        assert!(replaces_factory("(original) => ({})"));
        assert!(replaces_factory("function () { return {} }"));
        assert!(replaces_factory("async (original) => { return {} }"));
    }

    #[test]
    fn a_parameter_used_in_a_nested_closure_keeps_the_test() {
        assert!(!replaces_factory("(original) => ({ run: () => original })"));
        assert!(!replaces_factory(
            "async (original) => ({ run: async () => (await original()).run() })"
        ));
    }

    #[test]
    fn a_first_parameter_with_a_default_value_is_a_name_like_any_other() {
        // The default is read where it is written, and it uses nothing here; a default that loads the module is a load of its own.
        assert!(replaces_factory("(original = {}) => ({})"));
        assert!(!replaces_factory("(original = {}) => ({ original })"));
        assert!(!replaces_factory("({ a } = {}) => ({})"));
    }

    #[test]
    fn arguments_in_a_function_expression_is_the_first_parameter() {
        assert!(!replaces_factory("function () { return arguments[0] }"));
        assert!(!replaces_factory(
            "function (original) { return arguments }"
        ));
        // An arrow function has no `arguments` of its own.
        assert!(replaces_factory("() => ({ n: arguments.length })"));
    }

    #[test]
    fn a_parameter_that_a_nested_function_shadows_keeps_the_test() {
        // The inner `original` is another variable, but the check does not tell, so the test stays.
        assert!(!replaces_factory(
            "(original) => ({ run: (original) => original })"
        ));
        assert!(!replaces_factory(
            "function (original) { var original = 1; return { original } }"
        ));
    }

    #[test]
    fn a_parenthesised_factory_is_read_as_the_factory_inside() {
        assert!(replaces_factory("(() => ({}))"));
        assert!(replaces_factory("((() => ({})))"));
        assert!(!replaces_factory("((original) => original)"));
    }

    #[test]
    fn a_factory_under_satisfies_or_as_is_read_as_the_factory_inside() {
        assert!(replaces_factory("(() => ({})) satisfies Factory"));
        assert!(replaces_factory("(() => ({})) as Factory"));
        assert!(!replaces_factory(
            "((original) => original) satisfies Factory"
        ));
    }

    #[test]
    fn a_factory_that_is_not_a_function_here_keeps_the_test() {
        assert!(!replaces_factory("factory"));
        assert!(!replaces_factory("{ spy: true }"));
        assert!(!replaces_factory("makeFactory()"));
    }
}
