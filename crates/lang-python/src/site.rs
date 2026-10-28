//! Python call, decorator and assignment sites as the rule engine reads them, and the text of a Python value.
//!
//! The rules of `gsttt_core::rules` are written against [`Site`]; this file is the whole of what Python adds to
//! it: which expressions count as text, and how a dotted name is written.

use std::borrow::Cow;
use std::cell::Cell;
use std::iter;
use std::num::NonZeroU32;

use gsttt_core::contract::ContractKind;
use gsttt_core::fact::Line;
use gsttt_core::rules::Site;
use gsttt_core::text::LineIndex;
use ruff_python_ast::str_prefix::StringLiteralPrefix;
use ruff_python_ast::{
    Arguments, Expr, ExprStringLiteral, FStringPartRef, InterpolatedStringElement, Operator,
};
use ruff_text_size::{TextRange, TextSize};

/// The text a computed part of a template is written as, the way every connector hands it to the rules.
pub(crate) const COMPUTED: &str = "${}";

/// Stands in a document for a line break an escape made until the whole document is known, which is when [`settled`]
/// writes it as a space. A noncharacter, so that no text holds it.
const ESCAPED_BREAK: char = '\u{FFFE}';

/// A file's text and where its lines start, with what a comment that runs to the end of the line starts with in the
/// language of the document: what reading a document out of the file needs beside the expression.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Source<'a> {
    text: &'a str,
    lines: &'a LineIndex,
    comment: Option<&'static str>,
}

impl<'a> Source<'a> {
    /// The file `text`, whose lines `lines` index, for a document in the language `lang` of an embed rule. A language
    /// whose comments are not known has none, and a break an escape made is never in the way of one.
    pub(crate) fn new(text: &'a str, lines: &'a LineIndex, lang: &str) -> Self {
        let comment = match lang {
            "sql" => Some("--"),
            "graphql" => Some("#"),
            _ => None,
        };
        Self {
            text,
            lines,
            comment,
        }
    }

    fn line_of(self, offset: TextSize) -> Line {
        self.lines.line(u32::from(offset))
    }
}

/// A text and the part of the source its literals fill, from the start of the first to the end of the last.
struct Text<'a> {
    text: Cow<'a, str>,
    range: TextRange,
}

/// The text of a Python value, when it has any: a string literal, an f-string or a `+` concatenation with each
/// computed part written as `${}`, a `"…".format(…)` call as its template with each replacement field written the
/// same way, and a list, tuple or set as its first element that has text.
pub(crate) fn text_of(expr: &Expr) -> Option<Cow<'_, str>> {
    text_in(expr, None).map(|found| found.text)
}

/// Like [`text_of`], for a document that is read line by line: the text keeps the lines of the source, and the
/// line its first literal starts on comes with it.
///
/// A literal joined from several parts, by implicit concatenation or by `+`, has each part on the line it is on in
/// the source, which the plain text does not. A line break an escape such as `\n` makes is a line the source does
/// not have, so it is written as a space and the part stays on its line. That runs a comment of the language of the
/// document on into what follows it, so a document with a comment before such a break is not read.
pub(crate) fn document_of<'a>(expr: &'a Expr, source: Source<'_>) -> Option<(Cow<'a, str>, Line)> {
    let found = text_in(expr, Some(source))?;
    let text = settled(found.text, source.comment)?;
    Some((text, source.line_of(found.range.start())))
}

/// `text` with each break an escape made written as a space, which keeps the document on the lines of the source.
/// `None` when a comment, which ends where its line does, comes before such a break on its line: the space would run
/// it on into the text after it. A call whose document lost a statement that way is better an unread site.
fn settled<'a>(text: Cow<'a, str>, comment: Option<&str>) -> Option<Cow<'a, str>> {
    if !text.contains(ESCAPED_BREAK) {
        return Some(text);
    }
    let swallowed = comment.is_some_and(|marker| {
        text.split('\n').any(|line| {
            line.rsplit_once(ESCAPED_BREAK)
                .is_some_and(|(before, _)| before.contains(marker))
        })
    });
    (!swallowed).then(|| Cow::Owned(text.replace(ESCAPED_BREAK, " ")))
}

/// Like [`text_of`], but only when no part of the text is computed.
pub(crate) fn literal_of(expr: &Expr) -> Option<Cow<'_, str>> {
    text_of(expr).filter(|text| !text.contains(COMPUTED))
}

/// The one reader of a value's text: plain with no `source`, with the lines of the source kept with them.
fn text_in<'a>(expr: &'a Expr, source: Option<Source<'_>>) -> Option<Text<'a>> {
    match expr {
        Expr::StringLiteral(string) => match source {
            Some(source) => literal_in(string, source),
            None => Some(Text {
                text: Cow::Borrowed(string.value.to_str()),
                range: string.range,
            }),
        },
        Expr::FString(fstring) => Some(Text {
            text: Cow::Owned(fstring_text(fstring)),
            range: fstring.range,
        }),
        Expr::BinOp(binop) if binop.op == Operator::Add => {
            let left = text_in(&binop.left, source);
            let right = text_in(&binop.right, source);
            concat(left.as_ref(), right.as_ref(), source)
        }
        Expr::Call(call) => match &*call.func {
            Expr::Attribute(attribute) if attribute.attr.as_str() == "format" => {
                let template = text_in(&attribute.value, source)?;
                Some(Text {
                    text: format_template(template.text)?,
                    range: template.range,
                })
            }
            _ => None,
        },
        Expr::List(list) => first_text(&list.elts, source),
        Expr::Tuple(tuple) => first_text(&tuple.elts, source),
        Expr::Set(set) => first_text(&set.elts, source),
        _ => None,
    }
}

/// Whether a value is written as a string: a literal, an f-string, `+` with a string on either side, `%` on a string or
/// `.format` on a string. A name, a subscript or a call is not, whatever it holds when the program runs.
pub(crate) fn is_string_shaped(expr: &Expr) -> bool {
    match expr {
        Expr::StringLiteral(_) | Expr::FString(_) => true,
        Expr::BinOp(binop) => match binop.op {
            Operator::Add => is_string_shaped(&binop.left) || is_string_shaped(&binop.right),
            Operator::Mod => is_string_shaped(&binop.left),
            _ => false,
        },
        Expr::Call(call) => matches!(
            &*call.func,
            Expr::Attribute(attribute)
                if attribute.attr.as_str() == "format" && is_string_shaped(&attribute.value)
        ),
        _ => false,
    }
}

fn first_text<'a>(elements: &'a [Expr], source: Option<Source<'_>>) -> Option<Text<'a>> {
    elements.iter().find_map(|element| text_in(element, source))
}

/// `left` followed by `right`, a side with no text being a computed part. With a `source`, `right` starts on the
/// line it is on in the source.
fn concat(
    left: Option<&Text<'_>>,
    right: Option<&Text<'_>>,
    source: Option<Source<'_>>,
) -> Option<Text<'static>> {
    let range = match (left, right) {
        (None, None) => return None,
        (Some(only), None) | (None, Some(only)) => only.range,
        (Some(first), Some(last)) => first.range.cover(last.range),
    };
    let head = left.map_or(COMPUTED, |left| &*left.text);
    let tail = right.map_or(COMPUTED, |right| &*right.text);
    let mut text = String::with_capacity(head.len() + tail.len());
    text.push_str(head);
    if let (Some(left), Some(right), Some(source)) = (left, right, source) {
        pad_to_line(
            &mut text,
            source.line_of(left.range.start()),
            source.line_of(right.range.start()),
        );
    }
    text.push_str(tail);
    Some(Text {
        text: Cow::Owned(text),
        range,
    })
}

/// The parts of a string literal, each on the line it starts on in the source.
///
/// A part keeps the line breaks the source has. One an escape makes is [`ESCAPED_BREAK`], and a backslash that ends a
/// line leaves its break out of the value, which the next break of the source puts back so that what follows stays on
/// its line. `None` when the value has a break the source does not account for, as `\x0a` writes one.
fn literal_in<'a>(string: &'a ExprStringLiteral, source: Source<'_>) -> Option<Text<'a>> {
    let first = source.line_of(string.range.start());
    let mut text = String::new();
    for part in &string.value {
        pad_to_line(&mut text, first, source.line_of(part.range.start()));
        let range = part.content_range();
        let content = source
            .text
            .get(range.start().to_usize()..range.end().to_usize())?;
        let raw = matches!(part.flags.prefix(), StringLiteralPrefix::Raw { .. });
        let mut pieces = part.value.split('\n');
        text.push_str(pieces.next()?);
        let mut owed = 0;
        for kind in breaks_in(content, raw) {
            match kind {
                Break::Continuation => {
                    owed += 1;
                    continue;
                }
                Break::Escape => text.push(ESCAPED_BREAK),
                Break::Source => {
                    text.extend(iter::repeat_n('\n', owed + 1));
                    owed = 0;
                }
            }
            text.push_str(pieces.next()?);
        }
        if pieces.next().is_some() {
            return None;
        }
    }
    Some(Text {
        text: Cow::Owned(text),
        range: string.range,
    })
}

/// How a line break of the source between the quotes of a literal meets the value of the literal.
#[derive(Clone, Copy, Debug)]
enum Break {
    /// A line break the source has and the value has.
    Source,
    /// A backslash and `n`: a line break the value has and the source does not.
    Escape,
    /// A backslash that ends a line: a line break the source has and the value does not.
    Continuation,
}

/// The line breaks of `content`, the source between the quotes of a literal, in the order they come. A raw string has
/// no escapes, so its backslashes are text.
fn breaks_in(content: &str, raw: bool) -> impl Iterator<Item = Break> + '_ {
    let mut bytes = content.bytes().peekable();
    iter::from_fn(move || {
        loop {
            match bytes.next()? {
                b'\n' => return Some(Break::Source),
                b'\\' if !raw => match bytes.next()? {
                    b'n' => return Some(Break::Escape),
                    b'\n' => return Some(Break::Continuation),
                    // `\<CR><LF>` ends a line; a lone `\<CR>` is no line break of the source.
                    b'\r' if bytes.next_if_eq(&b'\n').is_some() => {
                        return Some(Break::Continuation);
                    }
                    // Any other escape is one character that is no line break, or none; taking the byte after the
                    // backslash is what keeps `\\` from beginning one.
                    _ => {}
                },
                _ => {}
            }
        }
    })
}

/// Adds the line breaks that put what comes next on the line `next`, when `text`, which starts on the line
/// `first`, ends on an earlier one.
fn pad_to_line(text: &mut String, first: Line, next: Line) {
    let reached = first.saturating_add(newlines(text));
    for _ in reached..next {
        text.push('\n');
    }
}

fn newlines(text: &str) -> Line {
    Line::try_from(text.bytes().filter(|byte| *byte == b'\n').count()).unwrap_or(Line::MAX)
}

/// A `str.format` template with each replacement field written as [`COMPUTED`] and each doubled brace as the brace it
/// stands for, which is how Python reads it. `None` for a template Python refuses: a brace that closes nothing or is
/// never closed.
fn format_template(template: Cow<'_, str>) -> Option<Cow<'_, str>> {
    if !template.contains(['{', '}']) {
        return Some(template);
    }
    let mut text = String::with_capacity(template.len());
    // What the receiver already holds as computed is not a replacement field of the template.
    for (index, part) in template.split(COMPUTED).enumerate() {
        if index > 0 {
            text.push_str(COMPUTED);
        }
        let mut chars = part.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '{' | '}' if chars.next_if_eq(&c).is_some() => text.push(c),
                '{' => {
                    skip_field(chars.by_ref())?;
                    text.push_str(COMPUTED);
                }
                '}' => return None,
                _ => text.push(c),
            }
        }
    }
    Some(Cow::Owned(text))
}

/// Consumes the rest of a replacement field, up to the brace that closes it, past the fields its format spec nests.
/// `None` when it is never closed.
fn skip_field(chars: impl Iterator<Item = char>) -> Option<()> {
    let mut open = 1_usize;
    for c in chars {
        match c {
            '{' => open += 1,
            '}' if open == 1 => return Some(()),
            '}' => open -= 1,
            _ => {}
        }
    }
    None
}

fn fstring_text(fstring: &ruff_python_ast::ExprFString) -> String {
    let mut text = String::new();
    for part in &fstring.value {
        match part {
            FStringPartRef::Literal(literal) => text.push_str(&literal.value),
            FStringPartRef::FString(fstring) => {
                for element in &fstring.elements {
                    match element {
                        InterpolatedStringElement::Literal(literal) => {
                            text.push_str(&literal.value);
                        }
                        InterpolatedStringElement::Interpolation(_) => text.push_str(COMPUTED),
                    }
                }
            }
        }
    }
    text
}

/// The property `prop` of a dict literal, as text.
pub(crate) fn dict_prop<'a>(expr: &'a Expr, prop: &str) -> Option<Cow<'a, str>> {
    dict_value(expr, prop).and_then(text_of)
}

/// The value of the property `prop` of a dict literal.
pub(crate) fn dict_value<'a>(expr: &'a Expr, prop: &str) -> Option<&'a Expr> {
    let Expr::Dict(dict) = expr else {
        return None;
    };
    dict.items.iter().find_map(|item| {
        let Some(Expr::StringLiteral(key)) = &item.key else {
            return None;
        };
        (key.value.to_str() == prop).then_some(&item.value)
    })
}

/// The dotted name of a `Name` or an attribute chain: `os.environ`, `self.router`. A chain that starts at
/// something else, such as a call or a subscript, has `?` for that segment so that `*` patterns still match:
/// `Session().execute` is `?.execute`. `None` when the expression is not a name at all.
pub(crate) fn dotted(expr: &Expr) -> Option<String> {
    let mut name = String::new();
    write_dotted(expr, &mut name).then_some(name)
}

/// Appends the dotted name of `expr` to `out`, as [`dotted`] reads it, and tells whether `expr` is one. The calls of
/// a file are many, so they are read into one buffer instead of a `String` each.
pub(crate) fn write_dotted(expr: &Expr, out: &mut String) -> bool {
    match expr {
        Expr::Name(name) => out.push_str(name.id.as_str()),
        Expr::Attribute(attribute) => {
            if !write_dotted(&attribute.value, out) {
                out.push('?');
            }
            out.push('.');
            out.push_str(attribute.attr.as_str());
        }
        _ => return false,
    }
    true
}

/// `outer` and `inner` as one prefix, with `/` between them for HTTP and `.` otherwise; an empty side is left out.
///
/// `gsttt_core::rules` joins the class and receiver prefixes of a decorator rule the same way before it reads the
/// rule's own `prefix`; its `join` is private, so this is the copy, and `joins_match_the_rule_engine` pins it.
pub(crate) fn join_prefixes(kind: &ContractKind, outer: &str, inner: &str) -> String {
    match (outer.is_empty(), inner.is_empty()) {
        (true, _) => inner.to_owned(),
        (false, true) => outer.to_owned(),
        (false, false) => match kind {
            ContractKind::Http => format!("{outer}/{inner}"),
            _ => format!("{outer}.{inner}"),
        },
    }
}

/// A call, or a decorator that may be a call: `requests.get("/x")`, `@router.get("/x")`, `@app.route`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CallSite<'a> {
    pub(crate) callee: &'a str,
    /// `None` for a decorator that is not called.
    pub(crate) arguments: Option<&'a Arguments>,
    /// The function or class a decorator sits on.
    pub(crate) member: Option<&'a str>,
}

impl<'a> CallSite<'a> {
    /// The argument at `position`, counting from 1.
    fn positional(&self, position: NonZeroU32) -> Option<&'a Expr> {
        let index = usize::try_from(position.get().checked_sub(1)?).ok()?;
        self.arguments?.find_positional(index)
    }

    fn keyword_value(&self, name: &str) -> Option<&'a Expr> {
        Some(&self.arguments?.find_keyword(name)?.value)
    }
}

impl Site for CallSite<'_> {
    fn callee_last(&self) -> Option<Cow<'_, str>> {
        self.callee.rsplit('.').next().map(Cow::Borrowed)
    }

    fn arg(&self, position: NonZeroU32) -> Option<Cow<'_, str>> {
        text_of(self.positional(position)?)
    }

    fn arg_prop(&self, position: NonZeroU32, prop: &str) -> Option<Cow<'_, str>> {
        dict_prop(self.positional(position)?, prop)
    }

    fn keyword(&self, name: &str) -> Option<Cow<'_, str>> {
        text_of(self.keyword_value(name)?)
    }

    fn member(&self) -> Option<Cow<'_, str>> {
        self.member.map(Cow::Borrowed)
    }
}

/// A call as the rules that embed documents read it: the same site, with text that keeps the lines of the source.
/// It remembers the line the last text it handed out starts on, which is the line of the document the rule makes.
#[derive(Debug)]
pub(crate) struct EmbedSite<'a> {
    call: CallSite<'a>,
    source: Source<'a>,
    line: Cell<Line>,
}

impl<'a> EmbedSite<'a> {
    /// `line` is the line of the call, the one a document has when none of the call's text says otherwise.
    pub(crate) fn new(call: CallSite<'a>, source: Source<'a>, line: Line) -> Self {
        Self {
            call,
            source,
            line: Cell::new(line),
        }
    }

    /// The line the document starts on: where the text the rule read begins.
    pub(crate) fn line(&self) -> Line {
        self.line.get()
    }

    fn document(&self, expr: Option<&'a Expr>) -> Option<Cow<'a, str>> {
        let (text, line) = document_of(expr?, self.source)?;
        self.line.set(line);
        Some(text)
    }
}

impl Site for EmbedSite<'_> {
    fn callee_last(&self) -> Option<Cow<'_, str>> {
        self.call.callee_last()
    }

    fn arg(&self, position: NonZeroU32) -> Option<Cow<'_, str>> {
        self.document(self.call.positional(position))
    }

    fn arg_prop(&self, position: NonZeroU32, prop: &str) -> Option<Cow<'_, str>> {
        let dict = self.call.positional(position)?;
        self.document(dict_value(dict, prop))
    }

    fn keyword(&self, name: &str) -> Option<Cow<'_, str>> {
        self.document(self.call.keyword_value(name))
    }

    fn member(&self) -> Option<Cow<'_, str>> {
        self.call.member()
    }
}

/// An assignment `target = value`: the target is the callee and the value the first argument.
#[derive(Clone, Debug)]
pub(crate) struct AssignSite<'a> {
    /// The name read as the callee: the assigned name, or the environment variable a settings field maps to.
    pub(crate) name: Cow<'a, str>,
    /// `None` for an annotation without a value.
    pub(crate) value: Option<&'a Expr>,
}

impl Site for AssignSite<'_> {
    fn callee_last(&self) -> Option<Cow<'_, str>> {
        self.name.rsplit('.').next().map(Cow::Borrowed)
    }

    fn arg(&self, position: NonZeroU32) -> Option<Cow<'_, str>> {
        if position != NonZeroU32::MIN {
            return None;
        }
        text_of(self.value?)
    }

    fn arg_prop(&self, position: NonZeroU32, prop: &str) -> Option<Cow<'_, str>> {
        if position != NonZeroU32::MIN {
            return None;
        }
        dict_prop(self.value?, prop)
    }

    /// The keyword `name` of the call the value is, as in `url: str = Field(alias="DB_URL")`.
    ///
    /// A keyword that is there but not a literal names something the rule cannot read, so it comes back as a computed
    /// text and the rule records an unread site: left out, the rule would go on to the next alternative and name a
    /// variable the field does not read.
    fn keyword(&self, name: &str) -> Option<Cow<'_, str>> {
        let Expr::Call(call) = self.value? else {
            return None;
        };
        let value = &call.arguments.find_keyword(name)?.value;
        Some(literal_of(value).unwrap_or(Cow::Borrowed(COMPUTED)))
    }
}

#[cfg(test)]
mod tests {
    use gsttt_core::rules::{CallRule, Outcome};
    use proptest::prelude::*;
    use ruff_python_ast::PySourceType;
    use ruff_python_parser::{parse_module, parse_unchecked_source};

    use super::*;

    /// The keyword `name` of the value assigned by the one-line assignment `source`.
    fn assigned_keyword(source: &str, name: &str) -> Option<String> {
        let parsed = parse_module(source).unwrap();
        let Some(ruff_python_ast::Stmt::Assign(assign)) = parsed.suite().first() else {
            panic!("not an assignment");
        };
        let site = AssignSite {
            name: Cow::Borrowed("field"),
            value: Some(&assign.value),
        };
        site.keyword(name).map(Cow::into_owned)
    }

    #[test]
    fn a_keyword_of_the_assigned_call_is_read_and_one_that_is_not_a_literal_is_computed() {
        let read = |source| assigned_keyword(source, "alias");
        assert_eq!(
            read("field = Field(alias='DB_URL')").as_deref(),
            Some("DB_URL")
        );
        assert_eq!(read("field = Field(alias=NAME)").as_deref(), Some("${}"));
        assert_eq!(
            read("field = Field(alias=f'{PREFIX}_URL')").as_deref(),
            Some("${}")
        );
        assert_eq!(read("field = Field(alias=None)").as_deref(), Some("${}"));
        assert_eq!(read("field = Field('DB_URL', description='d')"), None);
        assert_eq!(read("field = 'DB_URL'"), None);
        assert_eq!(read("field = NAME"), None);
    }

    fn call_text(source: &str, position: u32) -> Option<String> {
        let parsed = parse_module(source).unwrap();
        let Some(ruff_python_ast::Stmt::Expr(statement)) = parsed.suite().first() else {
            panic!("not an expression statement");
        };
        let Expr::Call(call) = &*statement.value else {
            panic!("not a call");
        };
        let site = CallSite {
            callee: "f",
            arguments: Some(&call.arguments),
            member: None,
        };
        site.arg(NonZeroU32::new(position).unwrap())
            .map(Cow::into_owned)
    }

    #[test]
    fn values_read_as_text() {
        let cases = [
            (r#"f("/bookings")"#, Some("/bookings")),
            (r#"f(f"{BASE}/bookings/{id}")"#, Some("${}/bookings/${}")),
            (r#"f(BASE + "/bookings")"#, Some("${}/bookings")),
            (
                r#"f("/bookings/" + id + "/cancel")"#,
                Some("/bookings/${}/cancel"),
            ),
            (r#"f("/bookings/{}".format(id))"#, Some("/bookings/${}")),
            (
                r#"f("{0}/{name}/{:>4}/{!r}".format(a, b, c, d, name=e))"#,
                Some("${}/${}/${}/${}"),
            ),
            (r#"f("{:{width}}".format(x, width=1))"#, Some("${}")),
            (r#"f("{{}}".format())"#, Some("{}")),
            (r#"f("{{{}}}".format(x))"#, Some("{${}}")),
            (r#"f("a}}b".format())"#, Some("a}b")),
            (r#"f(("a" + b + "{}").format(x))"#, Some("a${}${}")),
            (r#"f(f"{a}/{{}}".format(x))"#, Some("${}/${}")),
            // Python refuses a brace that closes nothing or is never closed.
            (r#"f("{".format(x))"#, None),
            (r#"f("}".format(x))"#, None),
            (r#"f("a {b".format(x))"#, None),
            (r#"f("{{{".format(x))"#, None),
            (r#"f("}}}".format(x))"#, None),
            (r#"f("a" "b")"#, Some("ab")),
            (r#"f(["GET", "POST"])"#, Some("GET")),
            (r"f(url)", None),
            (r"f(1 + 2)", None),
            (r"f(g())", None),
        ];
        for (source, want) in cases {
            assert_eq!(call_text(source, 1).as_deref(), want, "{source}");
        }
    }

    /// Whether the expression statement `source` is written as a string.
    fn string_shaped(source: &str) -> bool {
        let parsed = parse_module(source).unwrap();
        let Some(ruff_python_ast::Stmt::Expr(statement)) = parsed.suite().first() else {
            panic!("not an expression statement");
        };
        is_string_shaped(&statement.value)
    }

    #[test]
    fn values_are_told_to_be_strings_by_how_they_are_written() {
        let strings = [
            r#""SELECT 1""#,
            r#""SELECT " "1""#,
            r#"f"SELECT {id}""#,
            r#""SELECT " + table"#,
            r#"table + " WHERE""#,
            r#"a + b + " WHERE""#,
            r#""SELECT %s" % id"#,
            r#""SELECT {}".format(id)"#,
            r#"("SELECT " + table).format(id)"#,
        ];
        for source in strings {
            assert!(string_shaped(source), "{source}");
        }
        let others = [
            "query",
            "queries[0]",
            "queries.insert",
            "compose(query)",
            "text(query)",
            "query.format(id)",
            "a + b",
            "a % b",
            r#"a % "b""#,
            "1 + 2",
            "-1",
        ];
        for source in others {
            assert!(!string_shaped(source), "{source}");
        }
    }

    /// The document an expression statement holds, read as one in the language `lang`, and the line it starts on.
    fn document_in(source: &str, lang: &str) -> Option<(String, Line)> {
        let parsed = parse_module(source).unwrap();
        let Some(ruff_python_ast::Stmt::Expr(statement)) = parsed.suite().first() else {
            panic!("not an expression statement");
        };
        let lines = LineIndex::new(source);
        let (text, line) = document_of(&statement.value, Source::new(source, &lines, lang))?;
        Some((text.into_owned(), line))
    }

    /// The document an expression statement holds, as SQL, and the line it starts on.
    fn document(source: &str) -> Option<(String, Line)> {
        document_in(source, "sql")
    }

    #[test]
    fn documents_keep_the_lines_of_the_source() {
        let cases = [
            (r#""SELECT 1""#, "SELECT 1", 1),
            // Parts on one line are joined as written; parts on lines of their own keep them.
            (r#""a" "b""#, "ab", 1),
            ("(\n    \"a \"\n    \"b\"\n)", "a \nb", 2),
            ("(\n    \"a\"\n\n    \"b\"\n)", "a\n\nb", 2),
            ("(\n    \"a\"\n    + \"b\"\n)", "a\nb", 2),
            ("(\n    \"a\" +\n    \"b\"\n)", "a\nb", 2),
            // The lines of a literal over several lines are the source's.
            (
                "\"\"\"\nSELECT 1\nFROM t\n\"\"\"",
                "\nSELECT 1\nFROM t\n",
                1,
            ),
            ("(\n    \"\"\"a\nb\"\"\"\n    \"c\"\n)", "a\nb\nc", 2),
            // A line break an escape makes is not a line of the source; the part after it is still on its own.
            (r#""a\nb""#, "a b", 1),
            ("(\n    \"a\\nb\"\n    \"c\"\n)", "a b\nc", 2),
            // A break of the source stays where an escape is in the same literal, before it or after it.
            ("\"\"\"a\\nb\nc\"\"\"", "a b\nc", 1),
            ("\"\"\"a\nb\\nc\"\"\"", "a\nb c", 1),
            // A line continuation takes one away, which the next break of the source gives back, or the next part.
            ("(\n    \"\"\"a \\\nb\"\"\"\n    \"c\"\n)", "a b\n\nc", 2),
            ("\"\"\"a \\\nb\nc\"\"\"", "a b\n\nc", 1),
            ("\"\"\"a \\\r\nb\nc\"\"\"", "a b\n\nc", 1),
            // An escaped backslash is no continuation, and a raw string has no escapes.
            ("\"\"\"a\\\\\nb\"\"\"", "a\\\nb", 1),
            (r#"r"a\nb""#, r"a\nb", 1),
            // A document starts on the line of its first literal, not of what is computed before it.
            (r#"BASE + "x""#, "${}x", 1),
            ("(\n    BASE\n    + \"x\"\n)", "${}x", 3),
            (r#"("SELECT {}".format(1))"#, "SELECT ${}", 1),
        ];
        for (source, text, line) in cases {
            assert_eq!(document(source), Some((text.to_owned(), line)), "{source}");
        }
        assert_eq!(document("name"), None);
    }

    #[test]
    fn a_comment_before_a_break_an_escape_made_is_not_read() {
        // The space the break becomes would run the comment on into `FROM t`, in a part, across parts and across `+`.
        let swallowed = [
            r#""SELECT a -- note\nFROM t""#,
            r#""SELECT a -- x " "\nFROM t""#,
            r#""SELECT a -- x" + "\nFROM t""#,
            "\"\"\"SELECT a\n-- x\nb -- y\\nFROM t\"\"\"",
        ];
        for source in swallowed {
            assert_eq!(document(source), None, "{source}");
        }
        // A comment after the break, or on a line of its own, ends where its line does in the document too.
        let kept = [
            (r#""SELECT a\nFROM t -- note""#, "SELECT a FROM t -- note"),
            (
                "\"\"\"-- x\nSELECT a\\nFROM t\"\"\"",
                "-- x\nSELECT a FROM t",
            ),
        ];
        for (source, text) in kept {
            assert_eq!(
                document(source).map(|found| found.0),
                Some(text.to_owned()),
                "{source}"
            );
        }
    }

    #[test]
    fn a_comment_is_the_one_of_the_language_of_the_document() {
        let source = r#""a # x\nb -- y\nc""#;
        // `#` starts a comment in GraphQL and `--` in SQL; a language whose comments are not known has none.
        assert_eq!(document_in(source, "graphql"), None);
        assert_eq!(document_in(source, "sql"), None);
        let hash = r#""a # x\nb""#;
        assert_eq!(document_in(hash, "graphql"), None);
        assert_eq!(
            document_in(hash, "sql").map(|found| found.0),
            Some("a # x b".to_owned())
        );
        assert_eq!(
            document_in(hash, "cypher").map(|found| found.0),
            Some("a # x b".to_owned())
        );
    }

    #[test]
    fn a_literal_the_parser_could_not_close_is_read_without_a_panic() {
        // The parser keeps the statements it could read out of a broken file, and a string it recovered from may be
        // shorter than its own quotes.
        let sources = [
            "'",
            "\"",
            "'''",
            "\"\"\"abc",
            "'abc",
            "\"abc\\",
            "'a\nb'",
            "u'",
            "r\"",
            "'a' 'b",
            "f'",
            "x = '",
        ];
        for source in sources {
            let parsed = parse_unchecked_source(source, PySourceType::Python);
            let lines = LineIndex::new(source);
            for statement in parsed.suite() {
                if let ruff_python_ast::Stmt::Expr(statement) = statement {
                    drop(document_of(
                        &statement.value,
                        Source::new(source, &lines, "sql"),
                    ));
                }
            }
        }
    }

    #[test]
    fn a_literal_whose_value_has_a_break_the_source_does_not_show_is_not_read() {
        // `\x0a` and `\N{LINE FEED}` write the break another way than `\n`, which the walk of the source cannot see.
        for source in [r#""a\x0ab""#, r#""a\N{LINE FEED}b""#, r#""a\12b""#] {
            assert_eq!(document(source), None, "{source}");
        }
    }

    /// One part of a joined literal, whose content starts with `marker`; the last kind has a second marker, `marker`
    /// with `b` before its closing `@`, after the line continuation and the line break that follows it.
    fn part_of_kind(kind: u8, marker: &str) -> String {
        match kind {
            0 => format!("\"{marker} x\""),
            1 => format!("\"{marker}\\nx\\ny\""),
            2 => format!("\"\"\"{marker}\nx\ny\"\"\""),
            3 => format!("\"\"\"{marker}\\nx\ny\"\"\""),
            _ => {
                let second = format!("{}b@", marker.trim_end_matches('@'));
                format!("\"\"\"{marker} x \\\ny\n{second}\"\"\"")
            }
        }
    }

    proptest! {
        #[test]
        fn every_part_of_a_document_stays_on_its_line(
            parts in proptest::collection::vec((0u8..5, 0usize..3, 0usize..4), 1..6),
            added in any::<bool>(),
            lead in 0usize..3,
        ) {
            let mut source = format!("({}", "\n".repeat(lead));
            for (index, (kind, breaks, indent)) in parts.iter().enumerate() {
                if index > 0 {
                    source.push_str(&"\n".repeat(*breaks));
                    source.push_str(&" ".repeat(*indent));
                    source.push_str(if added { " + " } else { " " });
                }
                source.push_str(&part_of_kind(*kind, &format!("@{index}@")));
            }
            source.push(')');
            let (text, line) = document(&source).unwrap();
            let markers = parts.iter().enumerate().flat_map(|(index, (kind, _, _))| {
                let second = (*kind == 4).then(|| format!("@{index}b@"));
                iter::once(format!("@{index}@")).chain(second)
            });
            for marker in markers {
                let in_source = source.split(&marker).next().unwrap().matches('\n').count();
                let in_document = text.split(&marker).next().unwrap().matches('\n').count();
                prop_assert_eq!(
                    line + Line::try_from(in_document).unwrap(),
                    1 + Line::try_from(in_source).unwrap(),
                    "{} in {:?}",
                    marker,
                    source,
                );
            }
        }
    }

    #[test]
    fn dotted_names_use_a_placeholder_for_calls() {
        let parsed = parse_module("a.b.c\nSession().execute\nx[0].y").unwrap();
        let names: Vec<Option<String>> = parsed
            .suite()
            .iter()
            .map(|stmt| match stmt {
                ruff_python_ast::Stmt::Expr(statement) => dotted(&statement.value),
                _ => None,
            })
            .collect();
        assert_eq!(
            names,
            [
                Some("a.b.c".to_owned()),
                Some("?.execute".to_owned()),
                Some("?.y".to_owned())
            ]
        );
    }

    #[test]
    fn joins_match_the_rule_engine() {
        // The engine joins a decorator rule's prefix after the one the connector passes, with the same separators.
        let rule: CallRule = serde_json::from_value(serde_json::json!({
            "callee": "f", "kind": "http", "role": "serves", "key": "$arg1", "method": "GET"
        }))
        .unwrap();
        let parsed = parse_module(r#"f("/y")"#).unwrap();
        let Some(ruff_python_ast::Stmt::Expr(statement)) = parsed.suite().first() else {
            panic!("not an expression statement");
        };
        let Expr::Call(call) = &*statement.value else {
            panic!("not a call");
        };
        let site = CallSite {
            callee: "f",
            arguments: Some(&call.arguments),
            member: None,
        };
        let joined = join_prefixes(&ContractKind::Http, "/a", "/b");
        let Outcome::Fact(fact) = rule.apply("test", &site, Some(&joined), 1, "f") else {
            panic!("no fact");
        };
        assert_eq!(fact.key.key(), "GET /a/b/y");
        assert_eq!(join_prefixes(&ContractKind::Table, "a", "b"), "a.b");
        assert_eq!(join_prefixes(&ContractKind::Http, "", "/b"), "/b");
    }
}
