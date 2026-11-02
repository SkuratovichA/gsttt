//! The call and decorator sites of a syntax tree as owned text the rule packs can read, and the member chains they start from.

use std::borrow::Cow;
use std::num::NonZeroU32;

use gsttt_core::contract::{has_parent_segment, path_after_origin, without_query_and_fragment};
use gsttt_core::rules::{Prefix, Site, ValueExpr, ValueSource};
use oxc_ast::ast::{
    Argument, BinaryExpression, ChainElement, Expression, FormalParameter, MemberExpression,
    NewExpression, ObjectPropertyKind, PropertyDefinition, PropertyKey, TemplateLiteral,
};
use oxc_syntax::operator::BinaryOperator;
use serde::{Deserialize, Serialize};

use crate::analyze::Constants;
use crate::parts::{MAX_COMPOSED, Mode, Part, Reads};

/// How a computed part of a template or a concatenation is written in the text a rule reads.
pub(crate) const COMPUTED: &str = "${}";

/// What a class decorator gives as the prefixes of the routes of its class.
pub(crate) struct ClassPrefix {
    /// The text of each prefix, once each: one empty text when the decorator carries none.
    pub(crate) texts: Vec<String>,
    /// The decorator holds a prefix that is not text here, so the routes of the class under it are not known.
    pub(crate) unread: bool,
    /// Some prefix names a constant of another file, which only the link can read.
    pub(crate) waits: bool,
}

impl ClassPrefix {
    /// What the sites of a class decorator give under the class prefix `rule` that names it, one site for each path of an array of paths as [`Shape::each`] tells them apart.
    pub(crate) fn read(rule: &Prefix, sites: &[Shape]) -> Self {
        let mut prefix = Self {
            texts: Vec::new(),
            unread: false,
            waits: rule
                .key
                .as_ref()
                .is_some_and(|key| sites.iter().any(|site| site.holds_name(key))),
        };
        for site in sites {
            match rule.reading(site).as_prefix() {
                Some(text) if prefix.texts.iter().all(|known| known != text) => {
                    prefix.texts.push(text.to_owned());
                }
                Some(_) => {}
                None => prefix.unread = true,
            }
        }
        prefix
    }
}

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
    /// The elements of an array literal, each read on its own, and none for an element that is a spread or a hole. Only the walk of a file reads them, so a site that is stored has been through [`Shape::stored`] first.
    #[serde(skip)]
    elements: Vec<Value>,
}

impl Arg {
    /// The argument as it is stored, without the elements of an array.
    fn stored(&self) -> Self {
        Self {
            value: self.value.clone(),
            object: self.object,
            props: self.props.clone(),
            spread: self.spread,
            elements: Vec::new(),
        }
    }
}

/// What a rule can read of an expression: its text when it has one, and the pieces that text is put together from when some of them are names that start from an import of the file, which the link phase may be able to read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Value {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parts: Option<Part>,
}

impl Value {
    fn of(expr: &Expression<'_>, constants: &Constants) -> Self {
        Self::from_part(part_of(expr, constants, Mode::Use))
    }

    fn from_part(part: Option<Part>) -> Self {
        match part {
            None => Self::default(),
            Some(Part::Text(text)) => Self::text(Some(text)),
            Some(part) => Self {
                text: part.read(&|_| None),
                parts: Some(part),
            },
        }
    }

    fn text(text: Option<String>) -> Self {
        Self { text, parts: None }
    }

    /// Reads the pieces of a value with `resolve`.
    fn fill(&mut self, resolve: &impl Fn(&str) -> Option<String>) {
        if let Some(parts) = &self.parts {
            self.text = parts.read(resolve);
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

    /// The sites a rule reads for `expr`: one for each element of the array literal that its first `$argN[]` source takes, which holds that element in place of the array, and the site itself when `expr` has no such source, or the array no element.
    pub(crate) fn each(&self, expr: &ValueExpr) -> Cow<'_, [Shape]> {
        let array = expr.sources().iter().find_map(|source| {
            let ValueSource::ArgEach(position) = source else {
                return None;
            };
            let arg = self.arg_at(*position)?;
            (!arg.elements.is_empty()).then_some((slot(*position)?, arg))
        });
        let Some((slot, array)) = array else {
            return Cow::Borrowed(std::slice::from_ref(self));
        };
        Cow::Owned(
            array
                .elements
                .iter()
                .map(|element| {
                    let mut site = self.stored();
                    if let Some(arg) = site.args.get_mut(slot) {
                        *arg = Arg {
                            value: element.clone(),
                            ..Arg::default()
                        };
                    }
                    site
                })
                .collect(),
        )
    }

    /// The site as it is stored for the link phase: without the elements of its arrays, which [`Shape::each`] has told apart by then.
    pub(crate) fn stored(&self) -> Self {
        Self {
            last: self.last.clone(),
            member: self.member.clone(),
            args: self.args.iter().map(Arg::stored).collect(),
            spread: self.spread,
        }
    }

    /// Whether some source of `expr` holds a name that starts from an import of the file: the argument, or the property of the object literal, that it reads.
    pub(crate) fn holds_name(&self, expr: &ValueExpr) -> bool {
        expr.sources().iter().any(|source| {
            self.any_value(source, |value| {
                value.is_some_and(|value| value.parts.is_some())
            })
        })
    }

    /// Reads again every value that holds a name, with the text `resolve` finds for the names.
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
        self.args.get(slot(position)?)
    }

    /// Whether a `...spread` argument stands at `position`, counting from 1, or before it: the argument there is one the spread may give, and what is written there is not.
    fn unpacked(&self, position: NonZeroU32) -> bool {
        let Ok(position) = usize::try_from(position.get()) else {
            return false;
        };
        self.spread.is_some_and(|spread| spread < position)
    }

    /// Whether `pred` is true of some value that `source` reads: `Some(value)` for a value the call writes, and `None` for a value that a spread may give in place of what is written. An object literal written as an argument is no value of its own; its properties are.
    fn any_value(&self, source: &ValueSource, pred: impl Fn(Option<&Value>) -> bool) -> bool {
        match source {
            ValueSource::Arg(position) | ValueSource::ArgEach(position) => {
                (self.unpacked(*position) && pred(None))
                    || self
                        .arg_at(*position)
                        .is_some_and(|arg| !arg.object && pred(Some(&arg.value)))
            }
            ValueSource::ArgProp(position, prop) => {
                (self.unpacked(*position) && pred(None))
                    || self.arg_at(*position).is_some_and(|arg| {
                        if !arg.object {
                            return pred(Some(&arg.value));
                        }
                        (arg.spread && pred(None))
                            || arg
                                .props
                                .iter()
                                .any(|(name, value)| name == prop && pred(Some(value)))
                    })
            }
            ValueSource::Callee
            | ValueSource::Member
            | ValueSource::Keyword(_)
            | ValueSource::Literal(_) => false,
        }
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
        self.any_value(source, |value| {
            value.is_none_or(|value| value.text.is_none())
        })
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
    let elements = match expr.get_inner_expression() {
        Expression::ArrayExpression(array) => array
            .elements
            .iter()
            .map(|element| {
                element
                    .as_expression()
                    .map(|element| Value::of(element, constants))
                    .unwrap_or_default()
            })
            .collect(),
        _ => Vec::new(),
    };
    Arg {
        value: Value::of(expr, constants),
        object,
        props,
        spread,
        elements,
    }
}

/// Where the argument at `position`, counting from 1, is in the arguments of a call.
fn slot(position: NonZeroU32) -> Option<usize> {
    usize::try_from(position.get().checked_sub(1)?).ok()
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

/// The text of a string, a template or a concatenation, with each computed part written `${}` unless it is a name that `constants` give the text of, and each name that another file declares written `${}` as well; the first element that has text for an array; the URL that `new URL(path, base)` builds, alone or followed by `.href`, `.pathname` or `.toString()`. `None` for anything else.
pub(crate) fn text_with(expr: &Expression<'_>, constants: &Constants) -> Option<String> {
    part_of(expr, constants, Mode::Use)?.lax()
}

/// What `expr` reads as, put together from its pieces when some are names that another file declares, so that the link phase can read them: for a use, what [`text_with`] gives, and for the initializer of a constant, the text only when every piece of it reads, as a constant is. `None` when it has no text.
pub(crate) fn part_of(expr: &Expression<'_>, constants: &Constants, mode: Mode) -> Option<Part> {
    match expr.get_inner_expression() {
        Expression::StringLiteral(text) => {
            text.value.as_str().map(|text| Part::Text(text.to_owned()))
        }
        Expression::TemplateLiteral(template) => template_part(template, constants, mode),
        Expression::BinaryExpression(binary) if binary.operator == BinaryOperator::Addition => {
            sum_part(binary, constants, mode)
        }
        // An array, a URL and the text of a URL may hold a part that reads as no text, which a constant is not allowed to.
        Expression::ArrayExpression(array) if mode == Mode::Use => array
            .elements
            .iter()
            .find_map(|element| {
                element
                    .as_expression()
                    .and_then(|element| text_with(element, constants))
            })
            .map(Part::Text),
        Expression::NewExpression(new) if mode == Mode::Use => {
            url_text(new, constants).map(Part::Text)
        }
        // A member of a constant object may be called `href` too, so a URL that is not there leaves the name to the constants.
        inner @ Expression::StaticMemberExpression(member)
            if mode == Mode::Use
                && matches!(member.property.name.as_str(), "href" | "pathname") =>
        {
            url_object_text(&member.object, constants)
                .map(Part::Text)
                .or_else(|| constants.part(inner).map(Cow::into_owned))
        }
        Expression::CallExpression(call) if mode == Mode::Use && call.arguments.is_empty() => {
            match call.callee.get_inner_expression() {
                Expression::StaticMemberExpression(member)
                    if member.property.name.as_str() == "toString" =>
                {
                    url_object_text(&member.object, constants).map(Part::Text)
                }
                _ => None,
            }
        }
        other => constants.part(other).map(Cow::into_owned),
    }
}

/// What `a + b + c` reads as. The operands nest to the left, one level for each `+`, and a sum of thousands of them is not a depth to recurse to, so the left spine is walked down to the first operand and then up again, which puts the operands together in the order that reading each left operand first would. A piece that reads as no text is `${}` in a use, and leaves a constant no constant.
fn sum_part(first: &BinaryExpression<'_>, constants: &Constants, mode: Mode) -> Option<Part> {
    let operand = |part: Option<Part>| match part {
        None if mode == Mode::Use => Some(Part::Computed),
        part => part,
    };
    // The right operands from the outermost `+` inwards, and the operand that is the left of the innermost.
    let mut rights = vec![&first.right];
    let mut innermost = &first.left;
    while let Expression::BinaryExpression(binary) = innermost.get_inner_expression() {
        if binary.operator != BinaryOperator::Addition {
            break;
        }
        rights.push(&binary.right);
        innermost = &binary.left;
    }
    let mut sum = part_of(innermost, constants, mode);
    for right in rights.into_iter().rev() {
        let left = operand(sum)?;
        let right = operand(part_of(right, constants, mode))?;
        sum = append(left, right, mode.sum());
    }
    sum
}

/// `left` followed by `right`, as one level of pieces when `left` is already a sum of the same kind that reads as the sum of `left` and `right` would: a constant reads when all its pieces do, which is no matter how they are grouped, and a concatenation that holds a text reads always. So a long sum, which nests to the left, stays as flat as it is written and the depth a part may nest is for the sums written in parentheses and the templates inside one another.
fn append(left: Part, right: Part, reads: Reads) -> Option<Part> {
    match left {
        Part::Cat(own, mut parts)
            if own == reads
                && (reads == Reads::All
                    || parts.iter().any(|part| matches!(part, Part::Text(_)))) =>
        {
            parts.push(right);
            Part::cat(reads, parts)
        }
        left => Part::cat(reads, vec![left, right]),
    }
}

/// What a template reads as. The text is kept in one string until a piece holds a name, so that the common template, with none, allocates the string it reads as and nothing else. What the holes add to the template is limited to [`MAX_COMPOSED`] bytes, and the template is no text past it, because a constant that names another twice doubles at every step; the text the template is written with is never counted, since the file bounds it.
fn template_part(
    template: &TemplateLiteral<'_>,
    constants: &Constants,
    mode: Mode,
) -> Option<Part> {
    // `text` is the run of text since the last piece that holds a name; `parts` is what came before it.
    let mut text = String::new();
    let mut parts = Vec::new();
    let mut budget = MAX_COMPOSED;
    for piece in pieces(template) {
        let hole = match piece {
            Piece::Text(chunk) => {
                text.push_str(chunk);
                continue;
            }
            Piece::Hole(hole) => hole,
        };
        let Some(part) = constants.part(hole) else {
            match mode {
                Mode::Use => text.push_str(COMPUTED),
                Mode::Declare => return None,
            }
            continue;
        };
        if let Part::Text(chunk) = &*part {
            budget = budget.checked_sub(chunk.len())?;
            text.push_str(chunk);
            continue;
        }
        if !part.fits(&mut budget) {
            return None;
        }
        if !text.is_empty() {
            parts.push(Part::Text(std::mem::take(&mut text)));
        }
        parts.push(part.into_owned());
    }
    if parts.is_empty() {
        return Some(Part::Text(text));
    }
    if !text.is_empty() {
        parts.push(Part::Text(text));
    }
    Part::cat(mode.template(), parts)
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
    if has_parent_segment(path) {
        return None;
    }
    let relative = path.trim_start_matches("./");
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
    pieces(template)
        .map(|piece| match piece {
            Piece::Text(text) => text,
            Piece::Hole(_) => placeholder,
        })
        .collect()
}

/// One piece of a template, in source order.
pub(crate) enum Piece<'t, 'a> {
    /// The text between expressions, as the source means it.
    Text(&'t str),
    /// An expression, `${…}`.
    Hole(&'t Expression<'a>),
}

/// The pieces of `template` in source order, without the texts that are empty.
pub(crate) fn pieces<'t, 'a>(
    template: &'t TemplateLiteral<'a>,
) -> impl Iterator<Item = Piece<'t, 'a>> {
    template
        .quasis
        .iter()
        .enumerate()
        .flat_map(move |(index, quasi)| {
            let hole = index
                .checked_sub(1)
                .and_then(|before| template.expressions.get(before))
                .map(Piece::Hole);
            let raw = quasi.value.raw.as_str();
            let text = match quasi.value.cooked {
                Some(cooked) => cooked.as_str().unwrap_or(raw),
                None => raw,
            };
            hole.into_iter()
                .chain((!text.is_empty()).then_some(Piece::Text(text)))
        })
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
    use std::borrow::Cow;
    use std::num::NonZeroU32;

    use gsttt_core::contract::normalize_http_path;
    use gsttt_core::rules::{Reading, Site, ValueExpr};
    use oxc_allocator::Allocator;
    use oxc_ast::ast::{Expression, Statement};
    use oxc_parser::Parser;
    use oxc_span::SourceType;
    use proptest::prelude::*;

    use super::{Arg, Shape, Value, part_of, text_with, url_join};
    use crate::analyze::Constants;
    use crate::extra::{Deferred, DeferredTarget, RuleRef, TsExtra};
    use crate::parts::{MAX_COMPOSED, MAX_DEPTH, Mode, Part, Reads};

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
            parts: Some(Part::Name(name.to_owned())),
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
                    ..Arg::default()
                },
            ],
            spread: None,
        }
    }

    fn key(source: &str) -> ValueExpr {
        source.parse().unwrap()
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
        assert!(!shape.holds_name(&key("$arg1")));
        assert!(!serde_json::to_string(&shape).unwrap().contains("parts"));
    }

    #[test]
    fn a_shape_holds_the_names_of_the_values_an_expression_reads_and_no_others() {
        let shape = with_names();
        assert!(shape.holds_name(&key("$arg1")));
        assert!(shape.holds_name(&key("$arg2.path")));
        assert!(shape.holds_name(&key("$arg3|$arg1")));
        assert!(!shape.holds_name(&key("$arg2")));
        assert!(!shape.holds_name(&key("$arg2.method|GET")));
        assert!(!shape.holds_name(&key("$callee|$member")));
    }

    #[test]
    fn fill_reads_again_every_value_that_holds_a_name() {
        let mut shape = with_names();
        shape.fill(|name| (name == "paths.LOGIN").then(|| "/login".to_owned()));
        assert_eq!(shape.args[0].value.text.as_deref(), Some("/login"));
        // The first argument was read, the property is still a name nothing could read.
        assert_eq!(shape.args[1].value.text.as_deref(), Some("/x"));
        assert_eq!(shape.args[1].props[0].1.text, None);
        assert!(shape.args[1].props[0].1.parts.is_some());
    }

    /// The call `fetch(<arguments>)` after `prelude`, as the parse reads it.
    fn fetch_after(prelude: &str, arguments: &str) -> Shape {
        let allocator = Allocator::default();
        let code = format!("{prelude}\nfetch({arguments});");
        let parsed = Parser::new(&allocator, &code, SourceType::ts()).parse();
        let constants = Constants::collect(&parsed.program);
        let Some(Statement::ExpressionStatement(statement)) = parsed.program.body.last() else {
            panic!("no call in {code}");
        };
        let Expression::CallExpression(call) = &statement.expression else {
            panic!("no call in {code}");
        };
        Shape::of_call("fetch", call.arguments.iter(), &constants)
    }

    /// What the first argument of `fetch(<arguments>)` reads as after `prelude`, before and after the link reads `BASE` as `/api/v1` and no other name.
    fn first_argument(prelude: &str, arguments: &str) -> (Option<String>, Option<String>) {
        let mut shape = fetch_after(prelude, arguments);
        let one = NonZeroU32::MIN;
        let parsed = shape.arg(one).map(Cow::into_owned);
        shape.fill(|name| (name == "BASE").then(|| "/api/v1".to_owned()));
        (parsed, shape.arg(one).map(Cow::into_owned))
    }

    const IMPORT: &str = "import { BASE, OTHER } from './config';";

    #[test]
    fn a_template_with_an_imported_name_is_read_with_the_name_at_the_link() {
        assert_eq!(
            first_argument(IMPORT, "`${BASE}/users`"),
            (
                Some("${}/users".to_owned()),
                Some("/api/v1/users".to_owned())
            )
        );
        assert_eq!(
            first_argument(IMPORT, "`${OTHER}/users`"),
            (Some("${}/users".to_owned()), Some("${}/users".to_owned()))
        );
    }

    #[test]
    fn a_concatenation_with_an_imported_name_is_read_with_the_name_at_the_link() {
        assert_eq!(
            first_argument(IMPORT, "BASE + '/users'"),
            (
                Some("${}/users".to_owned()),
                Some("/api/v1/users".to_owned())
            )
        );
        assert_eq!(
            first_argument(IMPORT, "'/x' + OTHER + BASE"),
            (Some("/x${}${}".to_owned()), Some("/x${}/api/v1".to_owned()))
        );
    }

    #[test]
    fn a_concatenation_of_operands_that_are_no_text_is_no_text() {
        assert_eq!(
            first_argument(IMPORT, "BASE + OTHER"),
            (None, Some("/api/v1${}".to_owned()))
        );
        assert_eq!(first_argument(IMPORT, "OTHER + OTHER"), (None, None));
        assert_eq!(first_argument(IMPORT, "compute() + OTHER"), (None, None));
    }

    #[test]
    fn a_nested_concatenation_of_operands_that_are_no_text_is_one_computed_part() {
        assert_eq!(
            first_argument(IMPORT, "(OTHER + OTHER) + '/x'"),
            (Some("${}/x".to_owned()), Some("${}/x".to_owned()))
        );
        assert_eq!(
            first_argument("", "(compute() + compute()) + '/x'"),
            (Some("${}/x".to_owned()), Some("${}/x".to_owned()))
        );
    }

    #[test]
    fn a_bare_imported_name_is_no_text_until_the_link_reads_it() {
        assert_eq!(
            first_argument(IMPORT, "BASE"),
            (None, Some("/api/v1".to_owned()))
        );
        assert_eq!(first_argument(IMPORT, "OTHER"), (None, None));
    }

    #[test]
    fn a_constant_built_from_an_imported_name_is_text_only_when_the_link_reads_the_name() {
        let prelude = format!("{IMPORT}\nconst USERS = `${{BASE}}/users`;\nconst ALIAS = BASE;");
        assert_eq!(
            first_argument(&prelude, "USERS"),
            (None, Some("/api/v1/users".to_owned()))
        );
        assert_eq!(
            first_argument(&prelude, "ALIAS"),
            (None, Some("/api/v1".to_owned()))
        );
        assert_eq!(
            first_argument(&prelude, "`${USERS}/x`"),
            (Some("${}/x".to_owned()), Some("/api/v1/users/x".to_owned()))
        );
        let prelude = format!("{IMPORT}\nconst USERS = `${{OTHER}}/users`;");
        assert_eq!(first_argument(&prelude, "USERS"), (None, None));
    }

    #[test]
    fn a_name_inside_an_array_or_a_url_is_not_followed() {
        assert_eq!(
            first_argument(IMPORT, "[BASE, '/x']"),
            (Some("/x".to_owned()), Some("/x".to_owned()))
        );
        assert_eq!(
            first_argument(IMPORT, "new URL(`${BASE}/x`, 'https://a.test/')"),
            (Some("${}/x".to_owned()), Some("${}/x".to_owned()))
        );
    }

    /// What each site that `key` is read for in `fetch(<arguments>)` gives, with `BASE` imported.
    fn each_reading(arguments: &str, key: &str) -> Vec<Reading> {
        let shape = fetch_after(IMPORT, arguments);
        let key = key.parse::<ValueExpr>().unwrap();
        shape.each(&key).iter().map(|site| key.read(site)).collect()
    }

    fn text(text: &str) -> Reading {
        Reading::Text(text.to_owned())
    }

    #[test]
    fn each_element_of_an_array_literal_is_a_site_of_its_own() {
        assert_eq!(
            each_reading("['/a', '/b/:id']", "$arg1[]"),
            [text("/a"), text("/b/:id")]
        );
        assert_eq!(
            each_reading("['/a', `/b/${id}`], 1", "$arg1[]|none"),
            [text("/a"), text("/b/${}")]
        );
        // The arguments around the array are what they were.
        assert_eq!(
            each_reading("'/x', ['/a', '/b']", "$arg2[]"),
            [text("/a"), text("/b")]
        );
        let shape = fetch_after(IMPORT, "'/x', ['/a', '/b']");
        let key = key("$arg2[]");
        for site in shape.each(&key).iter() {
            assert_eq!(self::key("$arg1").read(site), text("/x"));
        }
    }

    #[test]
    fn an_element_that_is_not_text_is_a_site_that_is_not_read_and_the_others_stay_read() {
        assert_eq!(
            each_reading("['/a', compute(), ...rest, , '/b']", "$arg1[]"),
            [
                text("/a"),
                Reading::Unread,
                Reading::Unread,
                Reading::Unread,
                text("/b")
            ]
        );
    }

    #[test]
    fn what_is_no_array_literal_with_an_element_is_one_site_that_is_not_copied() {
        for (arguments, key) in [
            ("'/a'", "$arg1[]"),
            ("[]", "$arg1[]"),
            ("paths", "$arg1[]"),
            ("{ path: ['/a', '/b'] }", "$arg1[]"),
            // Without the `[]`, an array reads as its first element with text, as it always did.
            ("['/a', '/b']", "$arg1"),
            ("['/a', '/b']", "$arg2[]"),
            ("...args", "$arg1[]"),
            ("'/x', ...args, ['/a']", "$arg3[]"),
        ] {
            let shape = fetch_after(IMPORT, arguments);
            let key = key.parse::<ValueExpr>().unwrap();
            assert!(
                matches!(shape.each(&key), Cow::Borrowed([_])),
                "{key} over {arguments}"
            );
        }
        assert_eq!(each_reading("[]", "$arg1[]"), [Reading::Unread]);
        assert_eq!(each_reading("['/a', '/b']", "$arg1"), [text("/a")]);
    }

    #[test]
    fn an_element_that_starts_from_an_import_waits_for_the_link_on_its_own() {
        let shape = fetch_after(IMPORT, "[BASE, '/x']");
        let key = key("$arg1[]");
        let sites = shape.each(&key);
        assert_eq!(
            sites
                .iter()
                .map(|site| site.holds_name(&key))
                .collect::<Vec<_>>(),
            [true, false]
        );
        let mut waiting = sites[0].clone();
        waiting.fill(|name| (name == "BASE").then(|| "/api/v1".to_owned()));
        assert_eq!(key.read(&waiting), text("/api/v1"));
        assert_eq!(key.read(&sites[1]), text("/x"));
    }

    #[test]
    fn a_site_is_stored_without_the_elements_of_its_arrays() {
        let key = key("$arg1[]");
        for arguments in ["['/a', BASE]", "'/x', ['/a']", "{ path: ['/a'] }"] {
            let shape = fetch_after(IMPORT, arguments);
            let stored = shape.stored();
            let cached = serde_json::to_string(&stored).unwrap();
            assert_eq!(serde_json::from_str::<Shape>(&cached).unwrap(), stored);
        }
        for site in fetch_after(IMPORT, "['/a', BASE]").each(&key).iter() {
            let cached = serde_json::to_string(site).unwrap();
            assert_eq!(&serde_json::from_str::<Shape>(&cached).unwrap(), site);
            assert!(matches!(site.each(&key), Cow::Borrowed(_)));
        }
    }

    #[test]
    fn only_the_sources_of_a_key_decide_whether_a_name_matters() {
        let shape = fetch_after(IMPORT, "compute(), { method: BASE }");
        assert!(!shape.holds_name(&key("$arg1")));
        assert!(shape.holds_name(&key("$arg2.method|GET")));
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
    fn a_spread_may_give_any_value_but_never_a_name() {
        let method = key("$arg2.method");
        for (arguments, unread, name) in [
            ("'/x', { ...options }", true, false),
            ("'/x', { method: BASE }", true, true),
            ("'/x', { method: BASE, ...options }", true, true),
            ("'/x', ...rest", true, false),
            ("...args", true, false),
            ("'/x', { method: 'PUT' }", false, false),
            ("'/x', {}", false, false),
        ] {
            let shape = fetch_after(IMPORT, arguments);
            assert_eq!(
                method.read(&shape) == Reading::Unread,
                unread,
                "{arguments}"
            );
            assert_eq!(shape.holds_name(&method), name, "{arguments}");
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

    /// `levels` pieces of `Cat`, one inside the other, around a name.
    fn nested(levels: usize) -> Part {
        (0..levels).fold(Part::Name("BASE".to_owned()), |inner, _| {
            Part::Cat(Reads::Always, vec![inner, Part::Text("/x".to_owned())])
        })
    }

    /// The link data of a file that keeps `part` in the deepest place a part is kept: a property of an object argument of a deferred site.
    fn extra_holding(part: Part) -> TsExtra {
        TsExtra {
            deferred: vec![Deferred {
                rule: RuleRef::Call { pack: 0, rule: 0 },
                shape: Shape {
                    last: Some("fetch".to_owned()),
                    member: None,
                    args: vec![Arg {
                        value: Value::default(),
                        object: true,
                        props: vec![(
                            "url".to_owned(),
                            Value {
                                text: None,
                                parts: Some(part),
                            },
                        )],
                        ..Arg::default()
                    }],
                    spread: None,
                },
                prefix: None,
                line: 1,
                what: "fetch".to_owned(),
                target: DeferredTarget::Own,
            }],
            ..TsExtra::default()
        }
    }

    fn reads_back(levels: usize) -> bool {
        let extra = extra_holding(nested(levels));
        let bytes = serde_json::to_vec(&extra).unwrap();
        serde_json::from_slice::<TsExtra>(&bytes).is_ok_and(|read| read == extra)
    }

    #[test]
    fn a_part_at_the_depth_limit_is_read_back() {
        assert!(reads_back(MAX_DEPTH));
    }

    #[test]
    fn the_depth_limit_keeps_clear_of_what_serde_json_refuses() {
        let refused = (MAX_DEPTH..MAX_DEPTH * 2).find(|&levels| !reads_back(levels));
        assert!(
            refused.is_some_and(|levels| levels > MAX_DEPTH),
            "a part of {refused:?} levels is the first that does not read back"
        );
    }

    /// What `source` reads as in `mode`, with the names that `IMPORT` binds. The constants are collected from the import alone: the walk that collects them from a file recurses into every expression, and a sum of thousands of operands is not what this reads.
    fn sum_in(source: &str, mode: Mode) -> Option<Part> {
        let allocator = Allocator::default();
        let imports = Parser::new(&allocator, IMPORT, SourceType::ts()).parse();
        let constants = Constants::collect(&imports.program);
        let code = format!("({source});");
        let parsed = Parser::new(&allocator, &code, SourceType::ts()).parse();
        let Some(Statement::ExpressionStatement(statement)) = parsed.program.body.last() else {
            panic!("no expression in {code}");
        };
        part_of(&statement.expression, &constants, mode)
    }

    /// `terms` operands of a sum: the digits in turn as strings, and the name `OTHER` at every thousandth. The source, and the text the sum reads as when `OTHER` reads as `X`.
    fn long_sum(terms: usize) -> (String, String) {
        let mut source = Vec::new();
        let mut text = String::new();
        for term in 0..terms {
            if term % 1000 == 999 {
                source.push("OTHER".to_owned());
                text.push('X');
            } else {
                source.push(format!("'{}'", term % 10));
                text.push_str(&(term % 10).to_string());
            }
        }
        (source.join(" + "), text)
    }

    #[test]
    fn a_sum_of_thousands_of_operands_is_read_on_a_small_stack() {
        let (source, text) = long_sum(5000);
        let read = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || {
                let x = |name: &str| (name == "OTHER").then(|| "X".to_owned());
                let used = sum_in(&source, Mode::Use).unwrap();
                let declared = sum_in(&source, Mode::Declare).unwrap();
                let flat = |part: &Part| {
                    matches!(part, Part::Cat(_, parts) if parts.iter().all(|piece| !matches!(piece, Part::Cat(..))))
                };
                (
                    used.read(&x),
                    used.read(&|_| None),
                    declared.read(&x),
                    declared.read(&|_| None),
                    flat(&used) && flat(&declared),
                )
            })
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(read.0.as_deref(), Some(text.as_str()));
        assert_eq!(read.1, Some(text.replace('X', "${}")));
        assert_eq!(read.2.as_deref(), Some(text.as_str()));
        assert_eq!(read.3, None);
        assert!(read.4, "a long sum nests no deeper than it is written");
    }

    #[test]
    fn a_sum_that_holds_a_piece_with_no_text_is_a_use_with_a_hole_and_no_constant() {
        for source in [
            "'a' + compute() + 'b' + OTHER + 'c'",
            "(compute() + compute()) + 'a' + 'b'",
        ] {
            let used = sum_in(source, Mode::Use).and_then(Part::lax);
            assert!(used.is_some_and(|text| text.contains("${}")), "{source}");
            assert_eq!(sum_in(source, Mode::Declare), None, "{source}");
        }
        assert_eq!(
            sum_in("'a' + compute() + 'b'", Mode::Use).and_then(Part::lax),
            Some("a${}b".to_owned())
        );
    }

    #[test]
    fn a_sum_of_names_that_read_as_no_text_is_one_computed_part_where_it_is_grouped() {
        assert_eq!(
            first_argument(IMPORT, "OTHER + OTHER + '/x'"),
            (Some("${}/x".to_owned()), Some("${}/x".to_owned()))
        );
        assert_eq!(
            first_argument(IMPORT, "'/a' + OTHER + OTHER + '/x'"),
            (Some("/a${}${}/x".to_owned()), Some("/a${}${}/x".to_owned()))
        );
    }

    #[test]
    fn what_a_template_adds_to_a_text_is_limited_and_the_text_it_is_written_with_is_not() {
        let most = "x".repeat(MAX_COMPOSED / 4 * 3);
        let prelude = format!("const A = '{most}';");
        assert_eq!(length_in(&prelude, "`${A}`"), Some(most.len()));
        assert_eq!(length_in(&prelude, "`${A}${A}`"), None);
        // A constant is no constant past the limit either.
        let doubled = format!("{prelude}\nconst B = `${{A}}${{A}}`;");
        assert_eq!(length_in(&doubled, "B"), None);
        // The text a template is written with is its own, and the file bounds it.
        let written = "y".repeat(MAX_COMPOSED + 1);
        assert_eq!(length_in("", &format!("`{written}`")), Some(written.len()));
    }

    /// The length of the text the first argument of a `fetch` reads as, written after `prelude`.
    fn length_in(prelude: &str, arguments: &str) -> Option<usize> {
        fetch_after(prelude, arguments)
            .arg(NonZeroU32::MIN)
            .map(|text| text.len())
    }
}
