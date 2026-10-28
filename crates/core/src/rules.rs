//! Rule packs: how a framework spells a contract in code, written down as data.
//!
//! A pack tells one language connector which calls, decorators, member chains and embedded documents name a
//! contract, and where the key is: `fetch("/bookings")` calls `http GET /bookings`, `@Get(":id")` inside
//! `@Controller("bookings")` serves `http GET /bookings/{}`, `process.env.API_URL` calls `env API_URL`. Packs are
//! TOML files: the built-in ones live next to the connector they are for (`crates/lang-ts/packs/`), a user's own in
//! `.gsttt/packs/`. They arrive here through serde. A language connector finds the sites a rule names with
//! [`callee_matches`] and hands each one to the rule's `apply`, which makes every decision after that: where the
//! key is, whether the site is a contract at all, and whether it is a blind spot. So a rule means the same thing in
//! TypeScript, Python and Rust.
//!
//! Callees are dotted member paths in every language. `a?.b` is read as `a.b`, a subscript with a string literal
//! (`process.env["X"]`) is a segment, and a Rust path is written with dots: `env.var` matches `env::var(…)`.

use std::borrow::Cow;
use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::contract::{ContractKey, ContractKind, Role, normalize_http_path};
use crate::fact::{ContractFact, Embedded, Line, UnresolvedReason, UnresolvedSite};

/// The rules for one framework or library.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    /// The framework: `nestjs`, `fastapi`, `axios`. Answers name it as the way a contract was found. In a file of `.gsttt/packs/` it is the name of the file without `.toml`.
    pub name: String,
    /// The language connector the rules are for: `ts`, `python` or `rust`.
    pub language: String,
    /// Packages that switch the pack on for a project whose manifest depends on one of them. Empty: always on.
    #[serde(default)]
    pub detect: Vec<String>,
    #[serde(default)]
    pub calls: Vec<CallRule>,
    #[serde(default)]
    pub decorators: Vec<DecoratorRule>,
    #[serde(default)]
    pub chains: Vec<ChainRule>,
    #[serde(default)]
    pub embeds: Vec<EmbedRule>,
    #[serde(default)]
    pub assigns: Vec<AssignRule>,
}

impl Pack {
    /// Whether a project whose manifest lists `dependencies` gets this pack. Python names arrive normalised
    /// (PEP 503: lower case, runs of `-_.` as `-`).
    pub fn applies_to<'a>(&self, mut dependencies: impl Iterator<Item = &'a str>) -> bool {
        self.detect.is_empty()
            || dependencies.any(|dependency| self.detect.iter().any(|name| name == dependency))
    }
}

/// A call that names a contract: `fetch("/x")`, `axios.get("/x")`, `requests.post(url=…)`, `app.get("/x", h)`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CallRule {
    /// Callee patterns: `fetch`, `axios.*`, `this.http.*`. `*` matches one segment.
    pub callee: OneOrMany,
    #[schemars(with = "String")]
    pub kind: ContractKind,
    pub role: Role,
    /// Where the key is: `$arg1`, `$arg1.url`, `$url|$arg1`.
    #[schemars(with = "String")]
    pub key: ValueExpr,
    /// For HTTP, where the method is: `GET`, `$callee`, `$arg2.method|GET`. Unset: any method (`*`).
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    pub method: Option<ValueExpr>,
    /// A prefix taken from the call that made the receiver: `router = APIRouter(prefix="/x")` before
    /// `router.get("/y")`.
    #[serde(default)]
    pub receiver: Option<Prefix>,
}

/// A decorator that names a contract: NestJS `@Get(":id")`, FastAPI `@router.get("/x")`, Flask `@bp.route("/x")`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DecoratorRule {
    /// Decorator patterns, matched like callees: `Get`, `*.get`, `app.route`.
    pub decorator: OneOrMany,
    #[schemars(with = "String")]
    pub kind: ContractKind,
    pub role: Role,
    #[schemars(with = "String")]
    pub key: ValueExpr,
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    pub method: Option<ValueExpr>,
    /// A prefix taken from a decorator of the enclosing class: NestJS `@Controller("bookings")`.
    #[serde(default)]
    pub class: Option<Prefix>,
    /// A prefix taken from the call that made the decorator's receiver: FastAPI `APIRouter(prefix=…)`, Flask
    /// `Blueprint(url_prefix=…)`.
    #[serde(default)]
    pub receiver: Option<Prefix>,
    /// A prefix read at the decorator itself, joined after the class and receiver prefixes: with `$callee`,
    /// NestJS `@Query() bookings()` serves `graphql Query.bookings`.
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    pub prefix: Option<ValueExpr>,
}

/// A member chain that names a contract: `process.env.API_URL`, `prisma.booking.findMany()`,
/// `trpc.booking.get.useQuery()`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChainRule {
    /// Dotted roots a chain starts from: `process.env`, `import.meta.env`, `prisma`, `trpc`.
    pub root: OneOrMany,
    #[schemars(with = "String")]
    pub kind: ContractKind,
    pub role: Role,
    /// How many segments after the root make the key. Unset: every segment up to a terminal or the end of the
    /// chain.
    #[serde(default)]
    pub segments: Option<NonZeroU32>,
    /// Segments that end the key and are not part of it: `useQuery`, `query`, `mutate`.
    #[serde(default)]
    pub terminals: Vec<String>,
    /// A case change applied to the whole key: `upper-first` turns `prisma.booking` into `table Booking`.
    #[serde(default)]
    pub case: Option<Case>,
}

/// A document in another language inside code: a GraphQL query in a `gql` tagged template, SQL in
/// `cursor.execute("…")`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmbedRule {
    /// Template tags and callees: `gql`, `graphql`, `sql`, `*.execute`.
    pub callee: OneOrMany,
    /// The document's language, as the connector that parses it names it in `embeds`: `graphql`, `sql`.
    pub lang: String,
    /// For a call, where the document is; `$arg1` when unset. A tagged template is the document itself.
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    pub text: Option<ValueExpr>,
}

/// An assignment that names a contract: SQLAlchemy `__tablename__ = "bookings"`, Django `db_table = "bookings"`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssignRule {
    /// Names assigned to, matched like callees: `__tablename__`, `db_table`.
    pub target: OneOrMany,
    /// Base-class patterns, matched like callees, one of which the innermost enclosing class must list: `Base`,
    /// `*.Model`. Unset: the assignment counts wherever it is.
    #[serde(default)]
    pub class: Option<OneOrMany>,
    #[schemars(with = "String")]
    pub kind: ContractKind,
    pub role: Role,
    /// Where the key is. An assignment site gives the assigned value as `$arg1`.
    #[schemars(with = "String")]
    pub key: ValueExpr,
}

/// Where a key prefix comes from: the decorator or call named by `from`, and the value `key` reads there.
///
/// Every connector reads it the same way. For `class`, the innermost enclosing class must carry a decorator matching
/// `from`, or the rule does not apply. For `receiver`, the site's receiver must be a name bound in the same file to a
/// call matching `from` (`router = APIRouter(prefix="/x")`, `const app = express()`), or the rule does not apply;
/// a receiver imported from another file is left to the connector's `link` step, which sees both files. So
/// `receiver` is also how a rule stays off `cache.get("/x")`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Prefix {
    /// Decorator or callee patterns: `Controller`, `APIRouter`, `Blueprint`.
    pub from: OneOrMany,
    /// Unset when the maker carries no prefix and only gates the rule: `express.Router()`.
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    pub key: Option<ValueExpr>,
}

impl Prefix {
    /// The prefix at `maker`, the decorator or call that `from` matched. Empty when `key` is unset or yields nothing
    /// there, which includes a prefix held in a variable: connectors list that among their blind spots.
    pub fn read(&self, maker: &(impl Site + ?Sized)) -> String {
        self.key
            .as_ref()
            .and_then(|key| key.eval(maker))
            .unwrap_or_default()
    }
}

/// One pattern or several: `callee = "fetch"` or `callee = ["axios.*", "api.*"]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        match self {
            Self::One(one) => std::slice::from_ref(one).iter(),
            Self::Many(many) => many.iter(),
        }
        .map(String::as_str)
    }

    /// Whether any of the patterns matches the dotted `callee`, by [`callee_matches`].
    pub fn matches(&self, callee: &str) -> bool {
        self.iter().any(|pattern| callee_matches(pattern, callee))
    }
}

/// Whether a dotted callee such as `axios.get` matches a pattern such as `axios.*`. Segments compare one by one;
/// `*` matches exactly one segment.
pub fn callee_matches(pattern: &str, callee: &str) -> bool {
    let mut pattern = pattern.split('.');
    let mut callee = callee.split('.');
    loop {
        match (pattern.next(), callee.next()) {
            (None, None) => return true,
            (Some(want), Some(have)) if want == "*" || want == have => {}
            _ => return false,
        }
    }
}

/// A case change for chain keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Case {
    UpperFirst,
    LowerFirst,
    Upper,
    Lower,
}

impl Case {
    pub fn apply(self, text: &str) -> String {
        let mut chars = text.chars();
        match (self, chars.next()) {
            (Self::Upper, _) => text.to_uppercase(),
            (Self::Lower, _) => text.to_lowercase(),
            (Self::UpperFirst, Some(first)) => first.to_uppercase().chain(chars).collect(),
            (Self::LowerFirst, Some(first)) => first.to_lowercase().chain(chars).collect(),
            (Self::UpperFirst | Self::LowerFirst, None) => String::new(),
        }
    }
}

/// Where a value comes from at a call or decorator site: alternatives separated by `|`, tried left to right,
/// the first that yields text winning.
///
/// - `$callee`: the last segment of the callee, `get` in `axios.get(…)` and `Get` in `@Get(…)`.
/// - `$arg1`, `$arg2`, …: an argument, counting from 1, when it is a string literal or a template.
/// - `$arg2.method`: a property of an argument that is an object literal, or a dict literal in Python.
/// - `$member`: the name of the declaration a decorator or attribute sits on, `bookings` in `@Query() bookings()`.
/// - `$prefix`: a keyword argument, `prefix=…` in Python.
/// - Anything without `$` is literal text: `$arg2.method|GET` falls back to `GET`, and `$arg1|` to empty text.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ValueExpr(Vec<ValueSource>);

/// One alternative of a [`ValueExpr`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ValueSource {
    Callee,
    Member,
    Arg(NonZeroU32),
    ArgProp(NonZeroU32, String),
    Keyword(String),
    Literal(String),
}

/// Why a string is not a [`ValueExpr`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValueExprError {
    #[error("`{0}`: arguments count from 1, so `$arg1` is the first")]
    Position(String),
    #[error(
        "`{0}` is not a value: expected `$callee`, `$member`, `$argN`, `$argN.prop`, `$keyword` or literal text"
    )]
    Malformed(String),
}

/// A call or decorator site as a language connector reads it. Each connector implements it over its own syntax
/// tree, so a [`ValueExpr`] is evaluated the same way in every language.
pub trait Site {
    /// The last segment of the callee or of the decorator's name.
    fn callee_last(&self) -> Option<Cow<'_, str>>;
    /// The argument at `position`, counting from 1, as text: a string literal, or a template, f-string or
    /// concatenation with each computed part written as `${}`; a list, array or tuple literal gives its first
    /// element that is one of those. `None` for anything else.
    fn arg(&self, position: NonZeroU32) -> Option<Cow<'_, str>>;
    /// The property `prop` of the argument at `position` when that argument is an object or dict literal, as text
    /// the way [`Site::arg`] gives it.
    fn arg_prop(&self, position: NonZeroU32, prop: &str) -> Option<Cow<'_, str>>;
    /// The keyword argument `name`, in languages that have them.
    fn keyword(&self, name: &str) -> Option<Cow<'_, str>>;
    /// For a decorator or attribute, the name of the method, function, class or field it sits on.
    fn member(&self) -> Option<Cow<'_, str>> {
        None
    }
}

impl ValueExpr {
    /// The first alternative that yields text at `site`.
    pub fn eval(&self, site: &(impl Site + ?Sized)) -> Option<String> {
        self.0.iter().find_map(|source| match source {
            ValueSource::Callee => site.callee_last().map(Cow::into_owned),
            ValueSource::Member => site.member().map(Cow::into_owned),
            ValueSource::Arg(position) => site.arg(*position).map(Cow::into_owned),
            ValueSource::ArgProp(position, prop) => {
                site.arg_prop(*position, prop).map(Cow::into_owned)
            }
            ValueSource::Keyword(name) => site.keyword(name).map(Cow::into_owned),
            ValueSource::Literal(text) => Some(text.clone()),
        })
    }

    pub fn sources(&self) -> &[ValueSource] {
        &self.0
    }
}

impl ValueSource {
    fn parse(text: &str) -> Result<Self, ValueExprError> {
        let text = text.trim();
        let Some(name) = text.strip_prefix('$') else {
            return Ok(Self::Literal(text.to_owned()));
        };
        match name {
            "callee" => return Ok(Self::Callee),
            "member" => return Ok(Self::Member),
            _ => {}
        }
        if let Some(rest) = name.strip_prefix("arg") {
            let (position, prop) = match rest.split_once('.') {
                Some((position, prop)) => (position, Some(prop)),
                None => (rest, None),
            };
            if !position.is_empty() && position.bytes().all(|byte| byte.is_ascii_digit()) {
                let position: NonZeroU32 = position
                    .parse()
                    .map_err(|_| ValueExprError::Position(text.to_owned()))?;
                return match prop {
                    None => Ok(Self::Arg(position)),
                    Some(prop) if is_ident(prop) => Ok(Self::ArgProp(position, prop.to_owned())),
                    Some(_) => Err(ValueExprError::Malformed(text.to_owned())),
                };
            }
        }
        if is_ident(name) {
            Ok(Self::Keyword(name.to_owned()))
        } else {
            Err(ValueExprError::Malformed(text.to_owned()))
        }
    }
}

fn is_ident(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
        && chars.all(|rest| rest.is_alphanumeric() || rest == '_')
}

impl FromStr for ValueExpr {
    type Err = ValueExprError;

    fn from_str(text: &str) -> Result<Self, ValueExprError> {
        text.split('|')
            .map(ValueSource::parse)
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }
}

impl TryFrom<String> for ValueExpr {
    type Error = ValueExprError;

    fn try_from(text: String) -> Result<Self, ValueExprError> {
        text.parse()
    }
}

impl From<ValueExpr> for String {
    fn from(expr: ValueExpr) -> Self {
        expr.to_string()
    }
}

impl fmt::Display for ValueExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, source) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str("|")?;
            }
            match source {
                ValueSource::Callee => f.write_str("$callee")?,
                ValueSource::Member => f.write_str("$member")?,
                ValueSource::Arg(position) => write!(f, "$arg{position}")?,
                ValueSource::ArgProp(position, prop) => write!(f, "$arg{position}.{prop}")?,
                ValueSource::Keyword(name) => write!(f, "${name}")?,
                ValueSource::Literal(text) => f.write_str(text)?,
            }
        }
        Ok(())
    }
}

/// The contract a rule names at a site: `key` behind `prefix`, joined with `/` for HTTP and `.` otherwise, with
/// `method` for HTTP (`*` when unknown).
pub fn contract_key(
    kind: &ContractKind,
    prefix: Option<&str>,
    key: &str,
    method: Option<&str>,
) -> ContractKey {
    let joined = prefix.map_or(Cow::Borrowed(key), |prefix| join(kind, prefix, key));
    match kind {
        ContractKind::Http => ContractKey::http(method.unwrap_or("*"), &joined),
        kind => ContractKey::new(kind.clone(), &joined),
    }
}

/// `key` behind `prefix`, with `/` between them for HTTP and `.` otherwise; an empty side is left out.
fn join<'a>(kind: &ContractKind, prefix: &'a str, key: &'a str) -> Cow<'a, str> {
    match (prefix.is_empty(), key.is_empty()) {
        (true, _) => Cow::Borrowed(key),
        (false, true) => Cow::Borrowed(prefix),
        (false, false) => Cow::Owned(match kind {
            ContractKind::Http => format!("{prefix}/{key}"),
            _ => format!("{prefix}.{key}"),
        }),
    }
}

/// What a rule makes of a site whose callee, decorator, chain root or assignment target it matched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The site names this contract.
    Fact(ContractFact),
    /// The site names a contract whose key is computed at run time: a blind spot, which coverage counts.
    Blind(UnresolvedSite),
    /// The site is not a contract after all: `cache.get("user")` under an `*.get` rule, `app.listen(3000)` under
    /// `app.*`.
    Skip,
}

impl Outcome {
    pub fn fact(self) -> Option<ContractFact> {
        match self {
            Self::Fact(fact) => Some(fact),
            Self::Blind(_) | Self::Skip => None,
        }
    }
}

/// The verbs an HTTP method expression may yield; anything else means the site is not a route or a request.
const HTTP_METHODS: [&str; 11] = [
    "*", "ALL", "ANY", "DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT", "TRACE",
];

/// The parts every rule kind evaluates the same way.
#[derive(Clone, Copy)]
struct Spec<'r> {
    pack: &'r str,
    kind: &'r ContractKind,
    role: Role,
    key: &'r ValueExpr,
    method: Option<&'r ValueExpr>,
}

impl Spec<'_> {
    /// `what` names the site in a blind spot: the callee, decorator, root or target as written.
    fn apply(
        self,
        site: &(impl Site + ?Sized),
        prefix: Option<&str>,
        line: Line,
        what: &str,
    ) -> Outcome {
        let method = match (self.kind, self.method) {
            (ContractKind::Http, Some(expr)) => match expr.eval(site) {
                Some(method) if is_http_method(&method) => Some(method),
                Some(_) => return Outcome::Skip,
                None => None,
            },
            _ => None,
        };
        let Some(key) = self.key.eval(site) else {
            return blind(line, what);
        };
        let key = key.trim();
        match self.kind {
            ContractKind::Http if matches!(self.role, Role::Calls) && prefix.is_none() => {
                if !looks_like_url(key) {
                    return Outcome::Skip;
                }
                if is_computed(key) && normalize_http_path(key) == "/" {
                    return blind(line, what);
                }
            }
            ContractKind::Http => {}
            _ if key.is_empty() => return Outcome::Skip,
            _ if is_computed(key) => return blind(line, what),
            _ => {}
        }
        Outcome::Fact(ContractFact {
            key: contract_key(self.kind, prefix, key, method.as_deref()),
            role: self.role,
            line,
            via: self.pack.to_owned(),
        })
    }
}

fn is_http_method(method: &str) -> bool {
    let method = method.trim().to_ascii_uppercase();
    HTTP_METHODS.contains(&method.as_str())
}

/// Whether a request target names a URL or an absolute path rather than, say, a cache key.
fn looks_like_url(key: &str) -> bool {
    key.starts_with('/') || key.contains("://") || key.starts_with("${") || key.starts_with("{}")
}

fn is_computed(key: &str) -> bool {
    key.contains("${") || key.contains("{}")
}

fn blind(line: Line, what: &str) -> Outcome {
    Outcome::Blind(UnresolvedSite {
        line,
        reason: UnresolvedReason::NonLiteral,
        detail: what.to_owned(),
    })
}

impl CallRule {
    /// What this rule makes of a call whose callee matched [`CallRule::callee`]. `prefix` is what the connector
    /// read for [`CallRule::receiver`]; `callee` is the callee as written.
    pub fn apply(
        &self,
        pack: &str,
        site: &(impl Site + ?Sized),
        prefix: Option<&str>,
        line: Line,
        callee: &str,
    ) -> Outcome {
        Spec {
            pack,
            kind: &self.kind,
            role: self.role,
            key: &self.key,
            method: self.method.as_ref(),
        }
        .apply(site, prefix, line, callee)
    }
}

impl DecoratorRule {
    /// What this rule makes of a decorator, attribute or annotation that matched [`DecoratorRule::decorator`].
    /// `prefix` joins what the connector read for [`DecoratorRule::class`] and then [`DecoratorRule::receiver`];
    /// [`DecoratorRule::prefix`] is read here and joined after it.
    pub fn apply(
        &self,
        pack: &str,
        site: &(impl Site + ?Sized),
        prefix: Option<&str>,
        line: Line,
        decorator: &str,
    ) -> Outcome {
        let own = self.prefix.as_ref().and_then(|expr| expr.eval(site));
        let prefix = match (prefix, own.as_deref()) {
            (Some(outer), Some(inner)) => Some(join(&self.kind, outer, inner)),
            (outer, inner) => outer.or(inner).map(Cow::Borrowed),
        };
        Spec {
            pack,
            kind: &self.kind,
            role: self.role,
            key: &self.key,
            method: self.method.as_ref(),
        }
        .apply(site, prefix.as_deref(), line, decorator)
    }
}

impl AssignRule {
    /// What this rule makes of an assignment to a matched target; the site gives the assigned value as `$arg1`.
    pub fn apply(
        &self,
        pack: &str,
        site: &(impl Site + ?Sized),
        line: Line,
        target: &str,
    ) -> Outcome {
        Spec {
            pack,
            kind: &self.kind,
            role: self.role,
            key: &self.key,
            method: None,
        }
        .apply(site, None, line, target)
    }
}

impl ChainRule {
    /// What this rule makes of a member chain that starts at one of its roots. `segments` follow the root in
    /// order, `None` where a segment is computed (`process.env[name]`); `root` is the root as written.
    pub fn apply(&self, pack: &str, segments: &[Option<&str>], line: Line, root: &str) -> Outcome {
        let limit = self.segments.map_or(usize::MAX, |count| {
            usize::try_from(count.get()).unwrap_or(usize::MAX)
        });
        let mut parts: Vec<&str> = Vec::new();
        for segment in segments {
            match segment {
                None => return blind(line, root),
                Some(segment) if self.terminals.iter().any(|terminal| terminal == segment) => break,
                Some(_) if parts.len() == limit => break,
                Some(segment) => parts.push(segment),
            }
        }
        if parts.is_empty() {
            return Outcome::Skip;
        }
        let joined = parts.join(".");
        let key = self.case.map_or(joined.clone(), |case| case.apply(&joined));
        Outcome::Fact(ContractFact {
            key: contract_key(&self.kind, None, &key, None),
            role: self.role,
            line,
            via: pack.to_owned(),
        })
    }
}

impl EmbedRule {
    /// The document a call carries. A tagged template is the document itself and is read by the connector, not
    /// here. A document built at run time is a blind spot.
    pub fn apply(
        &self,
        pack: &str,
        site: &(impl Site + ?Sized),
        line: Line,
        callee: &str,
    ) -> Result<Embedded, UnresolvedSite> {
        let text = match &self.text {
            Some(expr) => expr.eval(site),
            None => site.arg(NonZeroU32::MIN).map(Cow::into_owned),
        };
        match text {
            Some(text) if !text.contains("${") => Ok(Embedded {
                lang: self.lang.clone(),
                line,
                text,
                via: pack.to_owned(),
            }),
            _ => Err(UnresolvedSite {
                line,
                reason: UnresolvedReason::NonLiteral,
                detail: callee.to_owned(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A call as a connector would hand it over: `None` arguments are not literals.
    struct Call<'a> {
        callee: &'a str,
        args: &'a [Option<&'a str>],
        keywords: &'a [(&'a str, &'a str)],
        member: Option<&'a str>,
    }

    impl Site for Call<'_> {
        fn callee_last(&self) -> Option<Cow<'_, str>> {
            self.callee.rsplit('.').next().map(Cow::Borrowed)
        }

        fn arg(&self, position: NonZeroU32) -> Option<Cow<'_, str>> {
            self.args
                .get(position.get() as usize - 1)
                .copied()
                .flatten()
                .map(Cow::Borrowed)
        }

        fn arg_prop(&self, _: NonZeroU32, _: &str) -> Option<Cow<'_, str>> {
            None
        }

        fn keyword(&self, name: &str) -> Option<Cow<'_, str>> {
            self.keywords
                .iter()
                .find(|(keyword, _)| *keyword == name)
                .map(|(_, value)| Cow::Borrowed(*value))
        }

        fn member(&self) -> Option<Cow<'_, str>> {
            self.member.map(Cow::Borrowed)
        }
    }

    fn call<'a>(callee: &'a str, args: &'a [Option<&'a str>]) -> Call<'a> {
        Call {
            callee,
            args,
            keywords: &[],
            member: None,
        }
    }

    #[test]
    fn resolver_decorators_name_the_field_they_sit_on() {
        let rule: DecoratorRule = serde_json::from_value(serde_json::json!({
            "decorator": ["Query", "Mutation"], "kind": "graphql", "role": "serves",
            "key": "$arg2.name|$member", "prefix": "$callee",
        }))
        .unwrap();
        let resolver = Call {
            member: Some("bookings"),
            ..call("Mutation", &[])
        };
        assert_eq!(
            key(rule.apply("nestjs-graphql", &resolver, None, 4, "Mutation")),
            "graphql Mutation.bookings"
        );
        assert_eq!(
            key(rule.apply("nestjs-graphql", &resolver, Some("Admin"), 4, "Mutation")),
            "graphql Admin.Mutation.bookings"
        );
        assert_eq!(expr("$member").to_string(), "$member");
    }

    fn rule(value: serde_json::Value) -> CallRule {
        serde_json::from_value(value).unwrap()
    }

    fn key(outcome: Outcome) -> String {
        outcome.fact().unwrap().key.to_string()
    }

    #[test]
    fn every_standard_http_verb_is_a_method() {
        let serves = rule(serde_json::json!({
            "callee": "app.*", "kind": "http", "role": "serves", "key": "$arg1", "method": "$callee",
        }));
        for verb in [
            "delete", "get", "head", "options", "patch", "post", "put", "trace",
        ] {
            let callee = format!("app.{verb}");
            let site = call(&callee, &[Some("/x")]);
            assert_eq!(
                key(serves.apply("express", &site, None, 1, &callee)),
                format!("http {} /x", verb.to_ascii_uppercase())
            );
        }
    }

    #[test]
    fn http_calls_skip_what_is_not_a_url_and_count_what_is_computed() {
        let get = rule(serde_json::json!({
            "callee": "*.get", "kind": "http", "role": "calls", "key": "$arg1", "method": "$callee",
        }));
        let apply = |site: &Call<'_>| get.apply("api", site, None, 7, site.callee);
        assert_eq!(apply(&call("cache.get", &[Some("user")])), Outcome::Skip);
        assert_eq!(
            key(apply(&call("api.get", &[Some("/bookings/${}")]))),
            "http GET /bookings/{}"
        );
        assert_eq!(
            key(apply(&call("api.get", &[Some("${}/bookings")]))),
            "http GET /bookings"
        );
        for computed in [None, Some("${}"), Some("https://${}")] {
            let Outcome::Blind(site) = apply(&call("api.get", &[computed])) else {
                panic!("{computed:?} is a blind spot")
            };
            assert_eq!(
                (site.line, site.reason, site.detail.as_str()),
                (7, UnresolvedReason::NonLiteral, "api.get")
            );
        }
        let any = rule(serde_json::json!({
            "callee": "app.*", "kind": "http", "role": "serves", "key": "$arg1", "method": "$callee",
        }));
        assert_eq!(
            any.apply(
                "express",
                &call("app.listen", &[None]),
                None,
                1,
                "app.listen"
            ),
            Outcome::Skip
        );
    }

    #[test]
    fn decorators_join_their_prefix_and_keep_an_empty_key() {
        let rule: DecoratorRule = serde_json::from_value(serde_json::json!({
            "decorator": "Get", "kind": "http", "role": "serves", "key": "$arg1|", "method": "GET",
        }))
        .unwrap();
        let get = |args: &[Option<&str>]| {
            key(rule.apply("nestjs", &call("Get", args), Some("bookings"), 3, "Get"))
        };
        assert_eq!(get(&[Some(":id")]), "http GET /bookings/{}");
        assert_eq!(get(&[]), "http GET /bookings");
    }

    #[test]
    fn chains_stop_at_terminals_and_change_case() {
        let chain =
            |value: serde_json::Value| -> ChainRule { serde_json::from_value(value).unwrap() };
        let prisma = chain(serde_json::json!({
            "root": "prisma", "kind": "table", "role": "calls", "segments": 1,
        }));
        assert_eq!(
            key(prisma.apply(
                "prisma",
                &[Some("bookingItem"), Some("findMany")],
                2,
                "prisma"
            )),
            "table bookingitem"
        );
        let models = chain(serde_json::json!({
            "root": "ctx.loaders", "kind": "graphql", "role": "calls", "segments": 1, "case": "upper-first",
        }));
        assert_eq!(
            key(models.apply(
                "dataloader",
                &[Some("booking"), Some("load")],
                2,
                "ctx.loaders"
            )),
            "graphql Booking"
        );
        let trpc = chain(serde_json::json!({
            "root": "trpc", "kind": "trpc", "role": "calls", "terminals": ["useQuery", "useUtils"],
        }));
        assert_eq!(
            key(trpc.apply(
                "trpc",
                &[Some("booking"), Some("get"), Some("useQuery")],
                2,
                "trpc"
            )),
            "trpc booking.get"
        );
        assert_eq!(
            trpc.apply("trpc", &[Some("useUtils")], 2, "trpc"),
            Outcome::Skip
        );
        let env =
            chain(serde_json::json!({ "root": "process.env", "kind": "env", "role": "calls" }));
        assert_eq!(
            key(env.apply("env", &[Some("API_URL")], 4, "process.env")),
            "env API_URL"
        );
        assert!(matches!(
            env.apply("env", &[None], 4, "process.env"),
            Outcome::Blind(_)
        ));
    }

    #[test]
    fn assignments_name_tables_and_embeds_carry_documents() {
        let tablename: AssignRule = serde_json::from_value(serde_json::json!({
            "target": "__tablename__", "class": "Base", "kind": "table", "role": "declares", "key": "$arg1",
        }))
        .unwrap();
        assert_eq!(
            key(tablename.apply(
                "sqlalchemy",
                &call("", &[Some("bookings")]),
                9,
                "__tablename__"
            )),
            "table bookings"
        );
        assert!(matches!(
            tablename.apply(
                "sqlalchemy",
                &call("", &[Some("${}_bookings")]),
                9,
                "__tablename__"
            ),
            Outcome::Blind(_)
        ));
        let sql: EmbedRule =
            serde_json::from_value(serde_json::json!({ "callee": "*.execute", "lang": "sql" }))
                .unwrap();
        let document = sql
            .apply(
                "dbapi",
                &call("cursor.execute", &[Some("SELECT 1 FROM bookings")]),
                5,
                "cursor.execute",
            )
            .unwrap();
        assert_eq!(
            (document.lang.as_str(), document.via.as_str()),
            ("sql", "dbapi")
        );
        assert!(
            sql.apply(
                "dbapi",
                &call("cursor.execute", &[None]),
                5,
                "cursor.execute"
            )
            .is_err()
        );
    }

    struct FakeSite;

    impl Site for FakeSite {
        fn callee_last(&self) -> Option<Cow<'_, str>> {
            Some("post".into())
        }

        fn arg(&self, position: NonZeroU32) -> Option<Cow<'_, str>> {
            (position.get() == 1).then(|| "/bookings/${}".into())
        }

        fn arg_prop(&self, position: NonZeroU32, prop: &str) -> Option<Cow<'_, str>> {
            (position.get() == 2 && prop == "method").then(|| "PUT".into())
        }

        fn keyword(&self, name: &str) -> Option<Cow<'_, str>> {
            (name == "prefix").then(|| "/api".into())
        }
    }

    fn expr(text: &str) -> ValueExpr {
        text.parse().unwrap()
    }

    #[test]
    fn value_exprs_round_trip_and_reject_nonsense() {
        for text in [
            "$arg1",
            "$arg2.method|GET",
            "$callee",
            "$url|$arg1",
            "$arg1|",
            "GET",
        ] {
            assert_eq!(expr(text).to_string(), text);
        }
        assert!(matches!(
            "$arg0".parse::<ValueExpr>(),
            Err(ValueExprError::Position(_))
        ));
        assert!(matches!(
            "$arg1.".parse::<ValueExpr>(),
            Err(ValueExprError::Malformed(_))
        ));
        assert!(matches!(
            "$1x".parse::<ValueExpr>(),
            Err(ValueExprError::Malformed(_))
        ));
    }

    #[test]
    fn eval_takes_the_first_alternative_that_yields() {
        assert_eq!(
            expr("$arg1").eval(&FakeSite).as_deref(),
            Some("/bookings/${}")
        );
        assert_eq!(
            expr("$arg2.method|GET").eval(&FakeSite).as_deref(),
            Some("PUT")
        );
        assert_eq!(
            expr("$arg3.method|GET").eval(&FakeSite).as_deref(),
            Some("GET")
        );
        assert_eq!(
            expr("$path|$prefix").eval(&FakeSite).as_deref(),
            Some("/api")
        );
        assert_eq!(expr("$callee").eval(&FakeSite).as_deref(), Some("post"));
        assert_eq!(expr("$arg4").eval(&FakeSite), None);
    }

    #[test]
    fn callee_patterns_match_segment_by_segment() {
        assert!(callee_matches("axios.*", "axios.get"));
        assert!(callee_matches("*.get", "router.get"));
        assert!(!callee_matches("axios.*", "axios"));
        assert!(!callee_matches("axios.*", "axios.defaults.get"));
        assert!(OneOrMany::Many(vec!["fetch".into(), "ky.*".into()]).matches("ky.post"));
    }

    #[test]
    fn keys_join_prefixes_by_kind() {
        let http = contract_key(&ContractKind::Http, Some("bookings"), ":id", Some("get"));
        assert_eq!(http.to_string(), "http GET /bookings/{}");
        assert_eq!(
            contract_key(&ContractKind::Http, Some("/api/"), "", None).to_string(),
            "http * /api"
        );
        assert_eq!(
            contract_key(&ContractKind::Trpc, Some("booking"), "get", None).to_string(),
            "trpc booking.get"
        );
        assert_eq!(
            contract_key(&ContractKind::Env, None, "API_URL", None).to_string(),
            "env API_URL"
        );
    }

    #[test]
    fn cases() {
        assert_eq!(Case::UpperFirst.apply("bookingItem"), "BookingItem");
        assert_eq!(Case::LowerFirst.apply("Booking"), "booking");
        assert_eq!(Case::Upper.apply("get"), "GET");
        assert_eq!(Case::UpperFirst.apply(""), "");
    }

    #[test]
    fn packs_deserialize_strictly() {
        let pack: Pack = serde_json::from_value(serde_json::json!({
            "name": "axios",
            "language": "ts",
            "detect": ["axios"],
            "calls": [{ "callee": "axios.*", "kind": "http", "role": "calls", "key": "$arg1", "method": "$callee" }],
        }))
        .unwrap();
        assert!(
            pack.applies_to(["react", "axios"].into_iter()) && !pack.applies_to(std::iter::empty())
        );
        let typo = serde_json::json!({ "name": "x", "language": "ts", "call": [] });
        assert!(serde_json::from_value::<Pack>(typo).is_err());
    }
}
