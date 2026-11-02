//! The call and decorator sites of a syntax tree as owned text the rule packs can read, and the member chains they start from.

use std::borrow::Cow;
use std::num::NonZeroU32;

use gsttt_core::contract::{path_after_origin, without_query_and_fragment};
use gsttt_core::rules::{Site, ValueSource};
use oxc_ast::ast::{
    Argument, ChainElement, Expression, FormalParameter, MemberExpression, NewExpression,
    ObjectPropertyKind, PropertyDefinition, PropertyKey, TemplateLiteral,
};
use oxc_syntax::operator::BinaryOperator;
use serde::{Deserialize, Serialize};

use crate::analyze::Constants;

/// How a computed part of a template or a concatenation is written in the text a rule reads.
pub(crate) const COMPUTED: &str = "${}";

/// A call or a decorator as the rules read it: the last segment of what is called, the declaration it sits on, and the text of each argument.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Shape {
    #[serde(skip_serializing_if = "Option::is_none")]
    last: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) member: Option<String>,
    args: Vec<Arg>,
    /// Where the first `...spread` argument is, counting from 0: it gives the arguments from that place on, and none of them is read at the place it is written.
    #[serde(skip_serializing_if = "Option::is_none")]
    spread: Option<usize>,
}

/// One argument: what it reads as, whether it is an object literal, and the properties of that literal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Arg {
    value: Value,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    object: bool,
    props: Vec<(String, Value)>,
    /// Whether the object literal has a `...spread` property, which may give any property that is not written.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    spread: bool,
}

/// What a rule can read of an expression: its text when it has one, and otherwise the name it is written as when that name starts from an import of the file, which the link phase may be able to read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Value {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

impl Value {
    fn of(expr: &Expression<'_>, constants: &Constants) -> Self {
        let text = text_with(expr, constants);
        let name = match text {
            Some(_) => None,
            None => constants.imported_name(expr),
        };
        Self { text, name }
    }

    fn text(text: Option<String>) -> Self {
        Self { text, name: None }
    }

    /// Reads the name of a value that has no text with `resolve`.
    fn fill(&mut self, resolve: &impl Fn(&str) -> Option<String>) {
        if self.text.is_none()
            && let Some(name) = &self.name
        {
            self.text = resolve(name);
        }
    }
}

impl Shape {
    /// The site of a call to the dotted `callee` with `arguments`, whose names are read against `constants`.
    pub(crate) fn of_call<'e, 'a: 'e>(
        callee: &str,
        arguments: impl IntoIterator<Item = &'e Argument<'a>>,
        constants: &Constants,
    ) -> Self {
        let arguments: Vec<&Argument<'a>> = arguments.into_iter().collect();
        Self {
            last: callee.rsplit('.').next().map(str::to_owned),
            member: None,
            args: arguments
                .iter()
                .map(|argument| {
                    argument
                        .as_expression()
                        .map(|expr| arg_of(expr, constants))
                        .unwrap_or_default()
                })
                .collect(),
            spread: arguments
                .iter()
                .position(|argument| matches!(argument, Argument::SpreadElement(_))),
        }
    }

    /// Whether some value has no text and is written as a name that starts from an import of the file.
    pub(crate) fn has_names(&self) -> bool {
        self.args.iter().any(|arg| {
            arg.value.name.is_some() || arg.props.iter().any(|(_, value)| value.name.is_some())
        })
    }

    /// Gives the text `resolve` finds for each name that has none yet.
    pub(crate) fn fill(&mut self, resolve: impl Fn(&str) -> Option<String>) {
        for arg in &mut self.args {
            arg.value.fill(&resolve);
            for (_, value) in &mut arg.props {
                value.fill(&resolve);
            }
        }
    }

    /// The site of a tagged template, where the text of the template is the first argument.
    pub(crate) fn of_tag(callee: &str, text: String) -> Self {
        Self {
            last: callee.rsplit('.').next().map(str::to_owned),
            member: None,
            args: vec![Arg {
                value: Value::text(Some(text)),
                ..Arg::default()
            }],
            spread: None,
        }
    }

    /// The site of an assignment, where the assigned value is the first argument.
    pub(crate) fn of_value(value: Option<String>) -> Self {
        Self {
            last: None,
            member: None,
            args: vec![Arg {
                value: Value::text(value),
                ..Arg::default()
            }],
            spread: None,
        }
    }

    /// The argument written at `position`, counting from 1, which is none when a spread stands there or before it.
    fn arg_at(&self, position: NonZeroU32) -> Option<&Arg> {
        if self.unpacked(position) {
            return None;
        }
        self.args
            .get(usize::try_from(position.get().checked_sub(1)?).ok()?)
    }

    /// Whether a `...spread` argument stands at `position`, counting from 1, or before it: the argument there is one the spread may give, and what is written there is not.
    fn unpacked(&self, position: NonZeroU32) -> bool {
        let Ok(position) = usize::try_from(position.get()) else {
            return false;
        };
        self.spread.is_some_and(|spread| spread < position)
    }
}

impl Site for Shape {
    fn callee_last(&self) -> Option<Cow<'_, str>> {
        self.last.as_deref().map(Cow::Borrowed)
    }

    fn arg(&self, position: NonZeroU32) -> Option<Cow<'_, str>> {
        self.arg_at(position)?
            .value
            .text
            .as_deref()
            .map(Cow::Borrowed)
    }

    fn arg_prop(&self, position: NonZeroU32, prop: &str) -> Option<Cow<'_, str>> {
        let (_, value) = self
            .arg_at(position)?
            .props
            .iter()
            .find(|(name, _)| name == prop)?;
        value.text.as_deref().map(Cow::Borrowed)
    }

    fn keyword(&self, _name: &str) -> Option<Cow<'_, str>> {
        None
    }

    fn member(&self) -> Option<Cow<'_, str>> {
        self.member.as_deref().map(Cow::Borrowed)
    }

    fn holds(&self, source: &ValueSource) -> bool {
        match source {
            ValueSource::Arg(position) => {
                self.unpacked(*position)
                    || self
                        .arg_at(*position)
                        .is_some_and(|arg| arg.value.text.is_none() && !arg.object)
            }
            ValueSource::ArgProp(position, prop) => {
                self.unpacked(*position)
                    || self.arg_at(*position).is_some_and(|arg| {
                        if arg.object {
                            arg.spread
                                || arg
                                    .props
                                    .iter()
                                    .any(|(name, value)| name == prop && value.text.is_none())
                        } else {
                            arg.value.text.is_none()
                        }
                    })
            }
            ValueSource::Callee
            | ValueSource::Member
            | ValueSource::Keyword(_)
            | ValueSource::Literal(_) => false,
        }
    }
}

fn arg_of(expr: &Expression<'_>, constants: &Constants) -> Arg {
    let object = matches!(expr.get_inner_expression(), Expression::ObjectExpression(_));
    let props = match expr.get_inner_expression() {
        Expression::ObjectExpression(object) => object
            .properties
            .iter()
            .filter_map(|property| match property {
                ObjectPropertyKind::ObjectProperty(property) if !property.computed => Some((
                    property_name(&property.key)?,
                    Value::of(&property.value, constants),
                )),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let spread = matches!(
        expr.get_inner_expression(),
        Expression::ObjectExpression(object)
            if object.properties.iter().any(|property| matches!(property, ObjectPropertyKind::SpreadProperty(_)))
    );
    Arg {
        value: Value::of(expr, constants),
        object,
        props,
        spread,
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

/// The text of a string, a template or a concatenation, with each computed part written `${}` unless it is a name that `constants` give the text of; the first element that has text for an array; the URL that `new URL(path, base)` builds, alone or followed by `.href`, `.pathname` or `.toString()`. `None` for anything else.
pub(crate) fn text_with(expr: &Expression<'_>, constants: &Constants) -> Option<String> {
    match expr.get_inner_expression() {
        Expression::StringLiteral(text) => text.value.as_str().map(str::to_owned),
        Expression::TemplateLiteral(template) => Some(template_with(template, |part, text| {
            text.push_str(constants.value(part).unwrap_or(COMPUTED));
        })),
        Expression::BinaryExpression(binary) if binary.operator == BinaryOperator::Addition => {
            let left = text_with(&binary.left, constants);
            let right = text_with(&binary.right, constants);
            if left.is_none() && right.is_none() {
                return None;
            }
            Some(format!(
                "{}{}",
                left.as_deref().unwrap_or(COMPUTED),
                right.as_deref().unwrap_or(COMPUTED)
            ))
        }
        Expression::ArrayExpression(array) => array.elements.iter().find_map(|element| {
            element
                .as_expression()
                .and_then(|element| text_with(element, constants))
        }),
        Expression::NewExpression(new) => url_text(new, constants),
        // A member of a constant object may be called `href` too, so a URL that is not there leaves the name to the constants.
        inner @ Expression::StaticMemberExpression(member)
            if matches!(member.property.name.as_str(), "href" | "pathname") =>
        {
            url_object_text(&member.object, constants)
                .or_else(|| constants.value(inner).map(str::to_owned))
        }
        Expression::CallExpression(call) if call.arguments.is_empty() => {
            match call.callee.get_inner_expression() {
                Expression::StaticMemberExpression(member)
                    if member.property.name.as_str() == "toString" =>
                {
                    url_object_text(&member.object, constants)
                }
                _ => None,
            }
        }
        other => constants.value(other).map(str::to_owned),
    }
}

/// The URL that `expr` builds when it is `new URL(…)`, possibly in parentheses.
fn url_object_text(expr: &Expression<'_>, constants: &Constants) -> Option<String> {
    match expr.get_inner_expression() {
        Expression::NewExpression(new) => url_text(new, constants),
        _ => None,
    }
}

/// The URL that `new URL(path, base)` builds, with `None` when the callee is not `URL` or `ns.URL`, or the path has no text.
fn url_text(new: &NewExpression<'_>, constants: &Constants) -> Option<String> {
    let callee = dotted_name(&new.callee)?;
    if callee != "URL" && !callee.ends_with(".URL") {
        return None;
    }
    let mut args = new.arguments.iter().map(|arg| arg.as_expression());
    let path = text_with(args.next()??, constants)?;
    let Some(base) = args.next() else {
        return Some(path);
    };
    let base = base.and_then(|base| text_with(base, constants));
    url_join(&path, base.as_deref())
}

/// The text of the URL that `path` makes against `base`, written the way the HTTP rules read it. A path with an origin, or one that starts with a computed part, which is a base URL, is the URL, a path that starts with `/` has a base URL that the normaliser drops, and a relative path joins the directory of `base`, or follows a dropped base URL when there is no text for it. `None` for an empty path without a base and for a path with a `..` segment, which would climb out of the text.
pub(crate) fn url_join(path: &str, base: Option<&str>) -> Option<String> {
    if path.is_empty() {
        return base.map(str::to_owned);
    }
    if path_after_origin(path).is_some() || path.starts_with(COMPUTED) {
        return Some(path.to_owned());
    }
    if path.starts_with("//") {
        return Some(format!("{COMPUTED}:{path}"));
    }
    if path.starts_with('/') {
        return Some(format!("{COMPUTED}{path}"));
    }
    if without_query_and_fragment(path)
        .split('/')
        .any(|segment| segment == "..")
    {
        return None;
    }
    let relative = path.strip_prefix("./").unwrap_or(path);
    let Some(base) = base else {
        return Some(format!("{COMPUTED}/{relative}"));
    };
    let base = without_query_and_fragment(base);
    let directory = if path_after_origin(base) == Some("") {
        format!("{base}/")
    } else {
        base.rsplit_once('/')
            .map_or_else(|| format!("{base}/"), |(head, _)| format!("{head}/"))
    };
    Some(format!("{directory}{relative}"))
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
    template_with(template, |_, text| text.push_str(placeholder))
}

/// The text of a template, with what `fill` writes where each expression stood.
pub(crate) fn template_with(
    template: &TemplateLiteral<'_>,
    mut fill: impl FnMut(&Expression<'_>, &mut String),
) -> String {
    let mut text = String::new();
    for (index, quasi) in template.quasis.iter().enumerate() {
        if let Some(part) = index
            .checked_sub(1)
            .and_then(|before| template.expressions.get(before))
        {
            fill(part, &mut text);
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

#[cfg(test)]
mod tests {
    use gsttt_core::contract::normalize_http_path;
    use gsttt_core::rules::{Reading, ValueExpr};
    use oxc_allocator::Allocator;
    use oxc_ast::ast::{Expression, Statement};
    use oxc_parser::Parser;
    use oxc_span::SourceType;
    use proptest::prelude::*;

    use super::{Arg, Shape, Value, text_with, url_join};
    use crate::analyze::Constants;

    #[test]
    fn a_path_with_an_origin_is_the_url() {
        assert_eq!(
            url_join("https://a.test/x", Some("https://b.test/y")).as_deref(),
            Some("https://a.test/x")
        );
        assert_eq!(
            url_join("https://a.test/x", None).as_deref(),
            Some("https://a.test/x")
        );
    }

    #[test]
    fn a_path_that_starts_with_two_slashes_drops_the_host() {
        let url = url_join("//cdn.test/x", Some("https://b.test/")).unwrap();
        assert_eq!(url, "${}://cdn.test/x");
        assert_eq!(normalize_http_path(&url), "/x");
    }

    #[test]
    fn a_path_that_starts_with_a_slash_has_a_dropped_base() {
        assert_eq!(
            url_join("/users", Some("https://a.test/v1/")).as_deref(),
            Some("${}/users")
        );
        assert_eq!(url_join("/users", None).as_deref(), Some("${}/users"));
    }

    #[test]
    fn a_path_with_a_parent_segment_is_not_read() {
        assert_eq!(url_join("../users", Some("https://a.test/v1/")), None);
        assert_eq!(url_join("a/../b", None), None);
    }

    #[test]
    fn a_relative_path_without_a_base_follows_a_dropped_base() {
        assert_eq!(url_join("users", None).as_deref(), Some("${}/users"));
        assert_eq!(url_join("./users", None).as_deref(), Some("${}/users"));
    }

    #[test]
    fn a_relative_path_joins_the_directory_of_the_base() {
        assert_eq!(
            url_join("users", Some("https://a.test/v1/")).as_deref(),
            Some("https://a.test/v1/users")
        );
        assert_eq!(
            url_join("users", Some("https://a.test/v1")).as_deref(),
            Some("https://a.test/users")
        );
        assert_eq!(
            url_join("users", Some("https://a.test")).as_deref(),
            Some("https://a.test/users")
        );
        assert_eq!(
            url_join("users", Some("${}/v1/")).as_deref(),
            Some("${}/v1/users")
        );
        assert_eq!(url_join("users", Some("${}")).as_deref(), Some("${}/users"));
        assert_eq!(
            url_join("users", Some("https://a.test/v1/?x=1#top")).as_deref(),
            Some("https://a.test/v1/users")
        );
    }

    #[test]
    fn a_path_that_starts_with_a_computed_part_is_the_url() {
        assert_eq!(
            url_join("${}/users", Some("https://a.test/v1/")).as_deref(),
            Some("${}/users")
        );
        assert_eq!(url_join("${}/users", None).as_deref(), Some("${}/users"));
    }

    #[test]
    fn a_url_of_one_argument_is_its_text() {
        assert_eq!(
            text_of_expression("new URL(`${baseUrl}/users`)").as_deref(),
            Some("${}/users")
        );
        assert_eq!(
            text_of_expression("new URL(`${API}/orders`, window.location.origin)").as_deref(),
            Some("${}/orders")
        );
        assert_eq!(
            text_of_expression("new URL('users')").as_deref(),
            Some("users")
        );
    }

    #[test]
    fn an_empty_path_is_the_base() {
        assert_eq!(
            url_join("", Some("https://a.test/v1")).as_deref(),
            Some("https://a.test/v1")
        );
        assert_eq!(url_join("", None), None);
    }

    #[test]
    fn a_query_in_the_path_stays_for_the_normaliser() {
        let url = url_join("users?x=1", None).unwrap();
        assert_eq!(url, "${}/users?x=1");
        assert_eq!(normalize_http_path(&url), "/users");
    }

    #[test]
    fn a_url_built_from_constants_is_read() {
        let allocator = Allocator::default();
        let code = "const BASE = 'https://a.test/v1/'; const ROUTES = { href: '/h' }; (new URL(`${BASE}x`)); (ROUTES.href);";
        let parsed = Parser::new(&allocator, code, SourceType::ts()).parse();
        let constants = Constants::collect(&parsed.program);
        let texts: Vec<String> = parsed
            .program
            .body
            .iter()
            .filter_map(|statement| match statement {
                Statement::ExpressionStatement(statement) => {
                    text_with(&statement.expression, &constants)
                }
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["https://a.test/v1/x", "/h"]);
    }

    fn text_of_expression(source: &str) -> Option<String> {
        let allocator = Allocator::default();
        let code = format!("({source});");
        let parsed = Parser::new(&allocator, &code, SourceType::ts()).parse();
        let Some(Statement::ExpressionStatement(statement)) = parsed.program.body.first() else {
            panic!("no expression in {source}");
        };
        text_with(&statement.expression, &Constants::default())
    }

    #[test]
    fn the_members_of_a_url_that_are_paths_are_read() {
        let base = "'https://a.test/v1/'";
        for form in [
            format!("new URL('users', {base})"),
            format!("new URL('users', {base}).href"),
            format!("new URL('users', {base}).pathname"),
            format!("new URL('users', {base}).toString()"),
            format!("(new URL('users', {base})).href"),
            format!("new url.URL('users', {base})"),
        ] {
            assert_eq!(
                text_of_expression(&form).as_deref(),
                Some("https://a.test/v1/users"),
                "{form}"
            );
        }
        assert_eq!(
            text_of_expression("new URL('/a', 'https://a.test/')").as_deref(),
            Some("${}/a")
        );
    }

    #[test]
    fn other_members_and_other_constructors_are_not_read() {
        for form in [
            "new URL('users', 'https://a.test/').origin",
            "new URL('users', 'https://a.test/').search",
            "new URL('users', 'https://a.test/').toString(1)",
            "new Other('users')",
            "new URL(path, base)",
            "new URL()",
        ] {
            assert_eq!(text_of_expression(form), None, "{form}");
        }
    }

    proptest! {
        #[test]
        fn a_relative_path_without_a_base_is_rooted(path in "[a-z0-9]{1,8}(/[a-z0-9]{1,8}){0,3}") {
            let url = url_join(&path, None).unwrap();
            prop_assert_eq!(normalize_http_path(&url), normalize_http_path(&format!("/{path}")));
        }
    }

    fn named(name: &str) -> Value {
        Value {
            text: None,
            name: Some(name.to_owned()),
        }
    }

    /// A decorator `@Get(paths.LOGIN, { path: ROOT })` whose second argument is `/x` as written.
    fn with_names() -> Shape {
        Shape {
            last: Some("Get".to_owned()),
            member: None,
            args: vec![
                Arg {
                    value: named("paths.LOGIN"),
                    ..Arg::default()
                },
                Arg {
                    value: Value::text(Some("/x".to_owned())),
                    object: true,
                    props: vec![("path".to_owned(), named("ROOT"))],
                    spread: false,
                },
            ],
            spread: None,
        }
    }

    #[test]
    fn the_names_of_a_shape_survive_the_cache() {
        let shape = with_names();
        let cached = serde_json::to_string(&shape).unwrap();
        assert_eq!(serde_json::from_str::<Shape>(&cached).unwrap(), shape);
    }

    #[test]
    fn a_shape_without_names_stores_no_name() {
        let shape = Shape {
            last: Some("fetch".to_owned()),
            member: None,
            args: vec![Arg {
                value: Value::text(Some("/x".to_owned())),
                ..Arg::default()
            }],
            spread: None,
        };
        assert!(!shape.has_names());
        assert!(!serde_json::to_string(&shape).unwrap().contains("name"));
    }

    #[test]
    fn fill_reads_a_name_only_when_the_value_has_no_text() {
        let mut shape = with_names();
        assert!(shape.has_names());
        shape.fill(|name| (name == "paths.LOGIN").then(|| "/login".to_owned()));
        assert_eq!(shape.args[0].value.text.as_deref(), Some("/login"));
        // The first argument was read, the property is still a name nothing could read.
        assert_eq!(shape.args[1].value.text.as_deref(), Some("/x"));
        assert_eq!(shape.args[1].props[0].1.text, None);
        assert!(shape.args[1].props[0].1.name.is_some());
    }

    /// The shape of the decorator `@Controller(<arguments>)` as the parse reads it, with no constant in scope.
    fn controller(arguments: &str) -> Shape {
        let allocator = Allocator::default();
        let code = format!("Controller({arguments});");
        let parsed = Parser::new(&allocator, &code, SourceType::ts()).parse();
        let Some(Statement::ExpressionStatement(statement)) = parsed.program.body.first() else {
            panic!("no call in {code}");
        };
        let Expression::CallExpression(call) = &statement.expression else {
            panic!("no call in {code}");
        };
        Shape::of_call("Controller", call.arguments.iter(), &Constants::default())
    }

    #[test]
    fn a_prefix_is_unread_only_when_the_decorator_holds_a_value_that_is_not_text() {
        let key = "$arg1|$arg1.path".parse::<ValueExpr>().unwrap();
        for (arguments, unread) in [
            ("", false),
            ("{ version: '1' }", false),
            ("{}", false),
            ("'auth'", false),
            ("['a', 'b']", false),
            ("PREFIX", true),
            ("prefix()", true),
            ("ROUTES.AUTH", true),
            ("{ path: PREFIX }", true),
        ] {
            assert_eq!(
                key.read(&controller(arguments)) == Reading::Unread,
                unread,
                "@Controller({arguments})"
            );
        }
    }

    #[test]
    fn a_literal_stands_in_only_for_a_value_the_site_does_not_carry() {
        let method = "$arg2.method|GET".parse::<ValueExpr>().unwrap();
        for (arguments, reading) in [
            ("'/x'", Reading::Text("GET".to_owned())),
            ("'/x', {}", Reading::Text("GET".to_owned())),
            ("'/x', options", Reading::Unread),
            ("'/x', { method: verb }", Reading::Unread),
            ("'/x', { method: 'PUT' }", Reading::Text("PUT".to_owned())),
        ] {
            assert_eq!(method.read(&controller(arguments)), reading, "{arguments}");
        }
    }

    #[test]
    fn a_spread_may_give_the_property_the_call_does_not_write() {
        let method = "$arg2.method|GET".parse::<ValueExpr>().unwrap();
        let put = || Reading::Text("PUT".to_owned());
        for (arguments, reading) in [
            ("'/x', { ...options }", Reading::Unread),
            ("'/x', { other: 1, ...options }", Reading::Unread),
            ("'/x', { method: 'PUT', ...options }", put()),
            ("'/x', { ...options, method: 'PUT' }", put()),
            ("'/x', { other: 1 }", Reading::Text("GET".to_owned())),
            ("'/x', ...rest", Reading::Unread),
            ("...args", Reading::Unread),
        ] {
            assert_eq!(method.read(&controller(arguments)), reading, "{arguments}");
        }
    }

    #[test]
    fn nothing_is_read_at_or_after_a_spread_argument() {
        let read = |key: &str, arguments: &str| {
            key.parse::<ValueExpr>()
                .unwrap()
                .read(&controller(arguments))
        };
        let text = |text: &str| Reading::Text(text.to_owned());
        // What is written before the spread is the argument at its place, and nothing after the spread is.
        assert_eq!(read("$arg1", "'/x', ...rest"), text("/x"));
        assert_eq!(read("$arg2", "'/x', ...rest"), Reading::Unread);
        assert_eq!(read("$arg1", "...parts, '/x'"), Reading::Unread);
        assert_eq!(read("$arg2", "...parts, '/x'"), Reading::Unread);
        assert_eq!(read("$arg3", "'/x', 'y'"), Reading::Absent);
    }

    #[test]
    fn the_spreads_of_a_shape_survive_the_cache() {
        let shape = controller("'/x', { ...options }, ...rest");
        let cached = serde_json::to_string(&shape).unwrap();
        assert_eq!(serde_json::from_str::<Shape>(&cached).unwrap(), shape);
        let plain = serde_json::to_string(&controller("'/x', { a: 1 }")).unwrap();
        assert!(!plain.contains("spread"), "{plain}");
    }

    #[test]
    fn the_object_flag_of_an_argument_survives_the_cache() {
        let shape = controller("{ path: PREFIX }");
        let cached = serde_json::to_string(&shape).unwrap();
        assert_eq!(serde_json::from_str::<Shape>(&cached).unwrap(), shape);
        assert!(
            !serde_json::to_string(&controller("'a'"))
                .unwrap()
                .contains("object")
        );
    }
}
