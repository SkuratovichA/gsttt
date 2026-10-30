//! The call and decorator sites of a syntax tree as owned text the rule packs can read, and the member chains they start from.

use std::borrow::Cow;
use std::num::NonZeroU32;

use gsttt_core::rules::Site;
use oxc_ast::ast::{
    Argument, ChainElement, Expression, FormalParameter, MemberExpression, ObjectPropertyKind,
    PropertyDefinition, PropertyKey, TemplateLiteral,
};
use oxc_syntax::operator::BinaryOperator;

/// How a computed part of a template or a concatenation is written in the text a rule reads.
const COMPUTED: &str = "${}";

/// A call or a decorator as the rules read it: the last segment of what is called, the declaration it sits on, and the text of each argument.
#[derive(Debug, Clone, Default)]
pub(crate) struct Shape {
    last: Option<String>,
    pub(crate) member: Option<String>,
    args: Vec<Arg>,
}

/// One argument: its text when it is a literal, and the properties of an object literal.
#[derive(Debug, Clone, Default)]
struct Arg {
    text: Option<String>,
    props: Vec<(String, Option<String>)>,
}

impl Shape {
    /// The site of a call to the dotted `callee` with `arguments`.
    pub(crate) fn of_call<'e, 'a: 'e>(
        callee: &str,
        arguments: impl IntoIterator<Item = &'e Argument<'a>>,
    ) -> Self {
        Self {
            last: callee.rsplit('.').next().map(str::to_owned),
            member: None,
            args: arguments
                .into_iter()
                .map(|argument| argument.as_expression().map(arg_of).unwrap_or_default())
                .collect(),
        }
    }

    /// The site of a tagged template, where the text of the template is the first argument.
    pub(crate) fn of_tag(callee: &str, text: String) -> Self {
        Self {
            last: callee.rsplit('.').next().map(str::to_owned),
            member: None,
            args: vec![Arg {
                text: Some(text),
                props: Vec::new(),
            }],
        }
    }

    /// The site of an assignment, where the assigned value is the first argument.
    pub(crate) fn of_value(value: Option<String>) -> Self {
        Self {
            last: None,
            member: None,
            args: vec![Arg {
                text: value,
                props: Vec::new(),
            }],
        }
    }

    fn arg_at(&self, position: NonZeroU32) -> Option<&Arg> {
        self.args
            .get(usize::try_from(position.get().checked_sub(1)?).ok()?)
    }
}

impl Site for Shape {
    fn callee_last(&self) -> Option<Cow<'_, str>> {
        self.last.as_deref().map(Cow::Borrowed)
    }

    fn arg(&self, position: NonZeroU32) -> Option<Cow<'_, str>> {
        self.arg_at(position)?.text.as_deref().map(Cow::Borrowed)
    }

    fn arg_prop(&self, position: NonZeroU32, prop: &str) -> Option<Cow<'_, str>> {
        let (_, value) = self
            .arg_at(position)?
            .props
            .iter()
            .find(|(name, _)| name == prop)?;
        value.as_deref().map(Cow::Borrowed)
    }

    fn keyword(&self, _name: &str) -> Option<Cow<'_, str>> {
        None
    }

    fn member(&self) -> Option<Cow<'_, str>> {
        self.member.as_deref().map(Cow::Borrowed)
    }
}

fn arg_of(expr: &Expression<'_>) -> Arg {
    let props = match expr.get_inner_expression() {
        Expression::ObjectExpression(object) => object
            .properties
            .iter()
            .filter_map(|property| match property {
                ObjectPropertyKind::ObjectProperty(property) if !property.computed => {
                    Some((property_name(&property.key)?, text_of(&property.value)))
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Arg {
        text: text_of(expr),
        props,
    }
}

/// The name of a property key written as an identifier or a string, as the source spells it.
pub(crate) fn property_str<'a>(key: &PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(name) => Some(name.name.as_str()),
        PropertyKey::StringLiteral(text) => text.value.as_str(),
        _ => None,
    }
}

/// The name of a property key written as an identifier or a string.
pub(crate) fn property_name(key: &PropertyKey<'_>) -> Option<String> {
    property_str(key).map(str::to_owned)
}

/// The name of the field that `field` declares in its class, `None` for a computed key.
pub(crate) fn field_name<'a>(field: &PropertyDefinition<'a>) -> Option<&'a str> {
    if field.computed {
        return None;
    }
    property_str(&field.key)
}

/// Whether a constructor parameter also declares a property of its class: `constructor(private readonly db: Db)` does, and `constructor(db: Db)` does not.
pub(crate) fn is_parameter_property(parameter: &FormalParameter<'_>) -> bool {
    parameter.accessibility.is_some() || parameter.readonly
}

/// The name under which a chain that starts at the property `name` of `this` is looked up: `this.db` for the property `db`.
pub(crate) fn this_key(name: &str) -> String {
    format!("this.{name}")
}

/// The text of a string, a template or a concatenation, with each computed part written `${}`; the first element that has text for an array. `None` for anything else.
pub(crate) fn text_of(expr: &Expression<'_>) -> Option<String> {
    match expr.get_inner_expression() {
        Expression::StringLiteral(text) => text.value.as_str().map(str::to_owned),
        Expression::TemplateLiteral(template) => Some(template_text(template, COMPUTED)),
        Expression::BinaryExpression(binary) if binary.operator == BinaryOperator::Addition => {
            let left = text_of(&binary.left);
            let right = text_of(&binary.right);
            if left.is_none() && right.is_none() {
                return None;
            }
            Some(format!(
                "{}{}",
                left.as_deref().unwrap_or(COMPUTED),
                right.as_deref().unwrap_or(COMPUTED)
            ))
        }
        Expression::ArrayExpression(array) => array
            .elements
            .iter()
            .find_map(|element| element.as_expression().and_then(text_of)),
        _ => None,
    }
}

/// The text of a string, or of a template with nothing computed in it.
pub(crate) fn literal_text(expr: &Expression<'_>) -> Option<String> {
    match expr.get_inner_expression() {
        Expression::StringLiteral(text) => text.value.as_str().map(str::to_owned),
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
            Some(template_text(template, ""))
        }
        _ => None,
    }
}

/// The text of a template with `placeholder` where each expression stood.
pub(crate) fn template_text(template: &TemplateLiteral<'_>, placeholder: &str) -> String {
    let mut text = String::new();
    for (index, quasi) in template.quasis.iter().enumerate() {
        if index > 0 {
            text.push_str(placeholder);
        }
        let raw = quasi.value.raw.as_str();
        text.push_str(match quasi.value.cooked {
            Some(cooked) => cooked.as_str().unwrap_or(raw),
            None => raw,
        });
    }
    text
}

/// The names of a chain of member accesses from its base, `process.env.A` as `["process", "env", "A"]`, with `None` for a segment that is computed. `None` when the chain does not start from a name, `this` or `import.meta`.
pub(crate) fn chain_of(expr: &Expression<'_>) -> Option<Vec<Option<String>>> {
    let mut names = Vec::new();
    push_chain(expr, &mut names).then_some(names)
}

/// Like [`chain_of`] for an expression that is already a member access.
pub(crate) fn member_chain(member: &MemberExpression<'_>) -> Option<Vec<Option<String>>> {
    let mut names = Vec::new();
    let found = match member {
        MemberExpression::StaticMemberExpression(member) => {
            push_static(&member.object, member.property.name.as_str(), &mut names)
        }
        MemberExpression::ComputedMemberExpression(member) => {
            push_computed(&member.object, &member.expression, &mut names)
        }
        MemberExpression::PrivateFieldExpression(_) => false,
    };
    found.then_some(names)
}

/// The names of `object.property`.
pub(crate) fn static_chain(object: &Expression<'_>, property: &str) -> Option<Vec<Option<String>>> {
    let mut names = Vec::new();
    push_static(object, property, &mut names).then_some(names)
}

/// The names joined with dots, when none is computed.
pub(crate) fn dotted(names: &[Option<String>]) -> Option<String> {
    let parts: Option<Vec<&str>> = names.iter().map(Option::as_deref).collect();
    parts.map(|parts| parts.join("."))
}

/// The name that `expr` writes when it is a name or a chain of names, `Base` for `Base` and `ns.Base` for `ns.Base`, with `None` for anything else. A single name is borrowed from the source, so that the common case allocates nothing.
pub(crate) fn dotted_name<'a>(expr: &Expression<'a>) -> Option<Cow<'a, str>> {
    if let Expression::Identifier(name) = expr.get_inner_expression() {
        return Some(Cow::Borrowed(name.name.as_str()));
    }
    chain_of(expr)
        .and_then(|names| dotted(&names))
        .map(Cow::Owned)
}

fn push_chain(expr: &Expression<'_>, names: &mut Vec<Option<String>>) -> bool {
    match expr.get_inner_expression() {
        Expression::Identifier(identifier) => {
            names.push(Some(identifier.name.as_str().to_owned()));
            true
        }
        Expression::ThisExpression(_) => {
            names.push(Some("this".to_owned()));
            true
        }
        Expression::ImportMeta(_) => {
            names.push(Some("import".to_owned()));
            names.push(Some("meta".to_owned()));
            true
        }
        Expression::StaticMemberExpression(member) => {
            push_static(&member.object, member.property.name.as_str(), names)
        }
        Expression::ComputedMemberExpression(member) => {
            push_computed(&member.object, &member.expression, names)
        }
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::StaticMemberExpression(member) => {
                push_static(&member.object, member.property.name.as_str(), names)
            }
            ChainElement::ComputedMemberExpression(member) => {
                push_computed(&member.object, &member.expression, names)
            }
            _ => false,
        },
        _ => false,
    }
}

fn push_static(object: &Expression<'_>, property: &str, names: &mut Vec<Option<String>>) -> bool {
    if !push_chain(object, names) {
        return false;
    }
    names.push(Some(property.to_owned()));
    true
}

fn push_computed(
    object: &Expression<'_>,
    expression: &Expression<'_>,
    names: &mut Vec<Option<String>>,
) -> bool {
    if !push_chain(object, names) {
        return false;
    }
    names.push(literal_text(expression));
    true
}
