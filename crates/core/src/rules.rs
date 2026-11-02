//! Rule packs: how a framework spells a contract in code, written down as data.
//!
//! A pack tells one language connector which calls, decorators, member chains, imported names and embedded documents name a contract, and where the key is: `fetch("/bookings")` calls `http GET /bookings`, `@Get(":id")` inside `@Controller("bookings")` serves `http GET /bookings/{}`, `process.env.API_URL` calls `env API_URL`. Packs are TOML files: the built-in ones live next to the connector they are for (`crates/lang-ts/packs/`), a user's own in `.gsttt/packs/`. They arrive here through serde. A language connector finds the sites a rule names with [`callee_matches`] and hands each one to the rule's `apply`, which makes every decision after that: where the key is, whether the site is a contract at all, and whether it is a blind spot. So a rule means the same thing in TypeScript, Python and Rust.
//!
//! Callees are dotted member paths in every language. `a?.b` is read as `a.b`, a subscript with a string literal (`process.env["X"]`) is a segment, and a Rust path is written with dots: `env.var` matches `env::var(…)`.

use std::borrow::Cow;
use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::contract::{
    ContractKey, ContractKind, PARAMETER, Role, normalize_http_path, path_after_origin,
};
use crate::fact::{ContractFact, Embedded, Line, UnresolvedSite};

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
    /// Packages whose imported names each name a contract, for a name the file uses as a value: the enums of `@prisma/client`, and no model type, which is a type. Only the TypeScript connector reads them.
    // Left out when empty, unlike the lists above, so that a pack written before this rule existed serialises as it did: the hash of the packs that keys the cache and the pinned packs of the other languages do not move.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<ImportRule>,
}

impl Pack {
    /// Whether a project whose manifest lists `dependencies` gets this pack. Python names arrive normalised (PEP 503: lower case, runs of `-_.` as `-`).
    pub fn applies_to<'a>(&self, mut dependencies: impl Iterator<Item = &'a str>) -> bool {
        self.detect.is_empty()
            || dependencies.any(|dependency| self.detect.iter().any(|name| name == dependency))
    }

    /// The kind and the role of every contract the rules of this pack name, once for each role a rule gives: a chain that is also named when it is written (`written`) lists both. The `[[imports]]` rules are left out, since only the TypeScript connector reads them, and it adds them; a connector adds the contracts it finds without a rule, as Rust does for an embedded document.
    pub fn contracts(&self) -> impl Iterator<Item = (&ContractKind, Role)> {
        let chains = self.chains.iter().flat_map(|rule| {
            rule.roles(Access::ReadWrite)
                .into_iter()
                .flatten()
                .map(move |role| (&rule.kind, role))
        });
        self.calls
            .iter()
            .map(|rule| (&rule.kind, rule.role))
            .chain(self.decorators.iter().map(|rule| (&rule.kind, rule.role)))
            .chain(chains)
            .chain(self.assigns.iter().map(|rule| (&rule.kind, rule.role)))
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
    /// For HTTP, where the method is: `GET`, `$callee`, `$arg2.method|GET`. Unset: any method (`*`), as is a method the site holds but cannot read, such as `{ method: verb }` for `$arg2.method`.
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    pub method: Option<ValueExpr>,
    /// A prefix taken from the call that made the receiver: `router = APIRouter(prefix="/x")` before `router.get("/y")`.
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
    /// A prefix taken from the call that made the decorator's receiver: FastAPI `APIRouter(prefix=…)`, Flask `Blueprint(url_prefix=…)`.
    #[serde(default)]
    pub receiver: Option<Prefix>,
    /// A prefix read at the decorator itself, joined after the class and receiver prefixes: with `$callee`, NestJS `@Query() bookings()` serves `graphql Query.bookings`.
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    pub prefix: Option<ValueExpr>,
}

/// A member chain that names a contract: `process.env.API_URL`, `prisma.booking.findMany()`, `trpc.booking.get.useQuery()`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChainRule {
    /// Dotted roots a chain starts from: `process.env`, `import.meta.env`, `prisma`, `trpc`.
    pub root: OneOrMany,
    /// Types that make a name a root too: a variable, a parameter or a class property declared with one of them, or made with `new` of one, starts a chain of this rule, and so does `this.<name>` for a constructor parameter property. A type counts when it is written as one of these, as a member of a union or an intersection, as the first type argument of `Pick`, `Omit`, `Partial`, `Required`, `Readonly` or `NonNullable`, through a `type` alias of the same file, or as a class or an interface of the same file that extends one: `PrismaClient`, `Prisma.TransactionClient`. A name the file also binds elsewhere to anything but `this` or a root type starts no chain, and a type the file declares under the name of a root type is that local type. Only the TypeScript connector reads them.
    // Left out when empty, for the reason given at `Pack::imports`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub root_types: Vec<String>,
    #[schemars(with = "String")]
    pub kind: ContractKind,
    /// The role of a site that reads the chain: `process.env.API_URL` in an expression.
    pub role: Role,
    /// The role of a site that writes the chain: `process.env.API_URL = "x"`. Unset: a write names nothing, so `prisma.user.findMany = vi.fn()` is no query. A site that both reads and writes the chain, as in `const url = (process.env.API_URL ??= "x")`, names the key with both roles.
    // Left out when unset, for the reason given at `Pack::imports`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written: Option<Role>,
    /// How many segments after the root make the key. Unset: every segment up to a terminal or the end of the chain.
    #[serde(default)]
    pub segments: Option<NonZeroU32>,
    /// Segments that end the key and are not part of it: `useQuery`, `query`, `mutate`.
    #[serde(default)]
    pub terminals: Vec<String>,
    /// A case change applied to the whole key: `upper-first` turns `prisma.booking` into `table Booking`.
    #[serde(default)]
    pub case: Option<Case>,
}

/// Names imported from a package that each name a contract: `import { BookingStatus } from '@prisma/client'` names the table `bookingstatus` once the file uses `BookingStatus` as a value, and a type, a re-export or a name the file never uses names nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportRule {
    /// Package specifiers, as written in the import: `@prisma/client`. A name imported from one of them that the file uses as a value is the key, at the line of the import. A namespace import, a default import and a side-effect import name nothing.
    pub from: OneOrMany,
    #[schemars(with = "String")]
    pub kind: ContractKind,
    pub role: Role,
    /// Exported names that name nothing, because the package exports more than contracts: `Prisma`, `PrismaClient`.
    #[serde(default)]
    pub except: Vec<String>,
    /// A case change applied to the exported name before it becomes a key.
    #[serde(default)]
    pub case: Option<Case>,
}

/// A document in another language inside code: a GraphQL query in a `gql` tagged template, SQL in `cursor.execute("…")`.
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
    /// Base-class patterns, matched like callees, one of which the innermost enclosing class must list: `Base`, `*.Model`. Unset: the assignment counts wherever it is.
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
/// Every connector reads it the same way. For `class`, the innermost enclosing class must carry a decorator matching `from`, or the rule does not apply. The TypeScript connector keeps the http serves route of a class without that decorator for its link phase, which serves it from the decorated classes that extend that class. For `receiver`, the site's receiver must be a name bound in the same file to a call matching `from` (`router = APIRouter(prefix="/x")`, `const app = express()`), or the rule does not apply; a receiver imported from another file is left to the connector's `link` step, which sees both files. So `receiver` is also how a rule stays off `cache.get("/x")`.
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
    /// What `key` reads at `maker`, the decorator or call that `from` matched: [`Reading::Absent`] when `key` is unset. A connector that would serve a route at a wrong path behind a prefix it cannot read treats [`Reading::Unread`] as a blind spot. The base of a client it may read as empty, since the origin is often an environment variable and the path after it is still right.
    pub fn reading(&self, maker: &(impl Site + ?Sized)) -> Reading {
        self.key
            .as_ref()
            .map_or(Reading::Absent, |key| key.read(maker))
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

/// Whether a dotted callee such as `axios.get` matches a pattern such as `axios.*`. Segments compare one by one; `*` matches exactly one segment.
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

/// Where a value comes from at a call or decorator site: alternatives separated by `|`, tried left to right, the first that yields text winning.
///
/// - `$callee`: the last segment of the callee, `get` in `axios.get(…)` and `Get` in `@Get(…)`.
/// - `$arg1`, `$arg2`, …: an argument, counting from 1, when it is a string literal or a template.
/// - `$arg2.method`: a property of an argument that is an object literal, or a dict literal in Python.
/// - `$member`: the name of the declaration a decorator or attribute sits on, `bookings` in `@Query() bookings()`.
/// - `$prefix`: a keyword argument, `prefix=…` in Python.
/// - Anything without `$` is literal text standing in for a value the site does not carry, never for one it carries and cannot read as text: `$arg2.method|GET` falls back to `GET` for `f(url)` and `f(url, {})` and reads nothing for `f(url, options)` and `f(url, { method: verb })`, and `$arg1|` reads no argument as empty text and `f(PATH)` as nothing.
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

/// A call or decorator site as a language connector reads it. Each connector implements it over its own syntax tree, so a [`ValueExpr`] is evaluated the same way in every language.
pub trait Site {
    /// The last segment of the callee or of the decorator's name.
    fn callee_last(&self) -> Option<Cow<'_, str>>;
    /// The argument at `position`, counting from 1, as text: a string literal, or a template, f-string or concatenation with each computed part written as `${}`; a list, array or tuple literal gives its first element that is one of those. `None` for anything else.
    fn arg(&self, position: NonZeroU32) -> Option<Cow<'_, str>>;
    /// The property `prop` of the argument at `position` when that argument is an object or dict literal, as text the way [`Site::arg`] gives it.
    fn arg_prop(&self, position: NonZeroU32, prop: &str) -> Option<Cow<'_, str>>;
    /// The keyword argument `name`, in languages that have them.
    fn keyword(&self, name: &str) -> Option<Cow<'_, str>>;
    /// For a decorator or attribute, the name of the method, function, class or field it sits on.
    fn member(&self) -> Option<Cow<'_, str>> {
        None
    }
    /// Whether the site carries a value at `source` that it cannot read as text, as opposed to carrying none there.
    ///
    /// - [`ValueSource::Arg`]: an argument at that position that is neither text nor an object or dict literal, which is read through its properties.
    /// - [`ValueSource::ArgProp`]: an object or dict literal at that position whose property is there and is not text, or an argument that is neither text nor such a literal, whose properties nothing can see. A literal without the property holds nothing there.
    /// - [`ValueSource::Keyword`]: a keyword argument that is there and is not text.
    /// - [`ValueSource::Callee`], [`ValueSource::Member`] and [`ValueSource::Literal`] hold nothing.
    fn holds(&self, source: &ValueSource) -> bool;
}

/// What a [`ValueExpr`] reads at a site. `None` from [`ValueExpr::eval`] covers both of the last two, and only [`Reading::Unread`] means the site has a value that the connector could not read.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Reading {
    /// The text of the first alternative that yields one.
    Text(String),
    /// The site carries no value for any alternative: `@Controller()` has no prefix.
    #[default]
    Absent,
    /// The site carries a value that is not text here and no alternative gives text: `@Controller(PREFIX)`, `APIRouter(prefix=settings.PREFIX)`.
    Unread,
}

impl Reading {
    /// Whether the site carries no value, which is what a cached reading leaves out.
    pub fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }

    /// The text, when there is some.
    pub fn text(self) -> Option<String> {
        match self {
            Self::Text(text) => Some(text),
            Self::Absent | Self::Unread => None,
        }
    }

    /// This reading as a prefix that is put in front of a path: the text, empty when the site carries none, and `None` when it carries one that is not text, because a path joined to nothing is not the path of the contract.
    pub fn as_prefix(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Absent => Some(""),
            Self::Unread => None,
        }
    }
}

/// Whether a rule of this kind and role finds the routes an HTTP server serves.
pub fn serves_http(kind: &ContractKind, role: Role) -> bool {
    *kind == ContractKind::Http && role == Role::Serves
}

impl From<ValueSource> for ValueExpr {
    /// The expression that reads one source, such as the keyword a connector names itself rather than a pack: `ValueSource::Keyword("prefix".into())` is `$prefix`.
    fn from(source: ValueSource) -> Self {
        Self(vec![source])
    }
}

impl ValueExpr {
    /// What this expression reads at `site`: the first alternative that yields text, tried left to right. A literal counts only when the site holds no value at an alternative before it, so that it stands in for a value that is absent and never for one that is there and unread. Without text, the reading is [`Reading::Unread`] when the site held a value at some alternative and [`Reading::Absent`] when it held none.
    pub fn read(&self, site: &(impl Site + ?Sized)) -> Reading {
        let mut held = false;
        for source in &self.0 {
            let text = match source {
                ValueSource::Callee => site.callee_last(),
                ValueSource::Member => site.member(),
                ValueSource::Arg(position) => site.arg(*position),
                ValueSource::ArgProp(position, prop) => site.arg_prop(*position, prop),
                ValueSource::Keyword(name) => site.keyword(name),
                ValueSource::Literal(text) => (!held).then_some(Cow::Borrowed(text.as_str())),
            };
            if let Some(text) = text {
                return Reading::Text(text.into_owned());
            }
            held = held || site.holds(source);
        }
        if held {
            Reading::Unread
        } else {
            Reading::Absent
        }
    }

    /// The text [`ValueExpr::read`] finds at `site`, for a caller that does not tell an absent value from an unread one.
    pub fn eval(&self, site: &(impl Site + ?Sized)) -> Option<String> {
        self.read(site).text()
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

/// The contract a rule names at a site: `key` behind `prefix`, joined with `/` for HTTP and `.` otherwise, with `method` for HTTP (`*` when unknown).
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
    /// The site is not a contract after all: `cache.get("user")` under an `*.get` rule, `app.listen(3000)` under `app.*`.
    Skip,
}

impl Outcome {
    pub fn fact(self) -> Option<ContractFact> {
        match self {
            Self::Fact(fact) => Some(fact),
            Self::Blind(_) | Self::Skip => None,
        }
    }

    /// A blind spot at `line`; `what` names the site as written: the callee, decorator, root or target.
    pub fn blind(line: Line, what: &str) -> Self {
        Self::Blind(UnresolvedSite::non_literal(line, what))
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
            return Outcome::blind(line, what);
        };
        let key = key.trim();
        match self.kind {
            ContractKind::Http if matches!(self.role, Role::Calls) => {
                if prefix.is_none() && !looks_like_url(key) {
                    return Outcome::Skip;
                }
                // A target of placeholders alone, the root included, would meet every served route of its length, whatever base the receiver puts before it.
                let target =
                    prefix.map_or(Cow::Borrowed(key), |prefix| join(self.kind, prefix, key));
                if is_computed(&target) && !has_literal_segment(&normalize_http_path(&target)) {
                    return Outcome::blind(line, what);
                }
            }
            ContractKind::Http => {}
            _ if key.is_empty() => return Outcome::Skip,
            _ if is_computed(key) => return Outcome::blind(line, what),
            // A key that is no variable name, such as `database.host`, is a path into a loaded configuration.
            ContractKind::Env if !is_ident(key) => return Outcome::Skip,
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
    key.starts_with('/')
        || path_after_origin(key).is_some()
        || key.starts_with("${")
        || key.starts_with("{}")
}

fn is_computed(key: &str) -> bool {
    key.contains("${") || key.contains("{}")
}

/// Whether a normalised HTTP path keeps a segment that is not the `{}` parameter.
fn has_literal_segment(path: &str) -> bool {
    path.split('/')
        .any(|segment| !segment.is_empty() && segment != PARAMETER)
}

/// The outcome of a rule that gives no prefix and no method: `key` is a contract of `kind`.
fn named(kind: &ContractKind, role: Role, key: &str, line: Line, pack: &str) -> Outcome {
    Outcome::Fact(ContractFact {
        key: contract_key(kind, None, key, None),
        role,
        line,
        via: pack.to_owned(),
    })
}

impl CallRule {
    /// The prefix a call of this rule is applied behind when the prefix of its [`CallRule::receiver`] reads as `reading`: empty when the receiver gives none. `None` for one that is not text under a rule that serves HTTP routes, because a route would be served at a path that is not its own and the call is a blind spot. Under any other rule it stands for no prefix: the base of a client is still the origin, and the path after it is right.
    pub fn behind<'r>(&self, reading: &'r Reading) -> Option<&'r str> {
        match (reading.as_prefix(), serves_http(&self.kind, self.role)) {
            (Some(prefix), _) => Some(prefix),
            (None, true) => None,
            (None, false) => Some(""),
        }
    }

    /// What this rule makes of a call whose callee matched [`CallRule::callee`]. `prefix` is what the connector read for [`CallRule::receiver`]; `callee` is the callee as written.
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
    /// What this rule makes of a decorator, attribute or annotation that matched [`DecoratorRule::decorator`]. `prefix` joins what the connector read for [`DecoratorRule::class`] and then [`DecoratorRule::receiver`]; [`DecoratorRule::prefix`] is read here and joined after it, and a value there that the connector cannot read as text makes the decorator a blind spot.
    pub fn apply(
        &self,
        pack: &str,
        site: &(impl Site + ?Sized),
        prefix: Option<&str>,
        line: Line,
        decorator: &str,
    ) -> Outcome {
        let own = match self.prefix.as_ref().map(|expr| expr.read(site)) {
            Some(Reading::Text(own)) => Some(own),
            None | Some(Reading::Absent) => None,
            // Joining nothing would serve the contract at a path that is not its own.
            Some(Reading::Unread) => return Outcome::blind(line, decorator),
        };
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

/// How a site touches a member chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// The value of the chain is used: `process.env.API_URL` in an expression.
    Read,
    /// A value is stored in the chain and its old value is not used: `process.env.API_URL = "x"`, `del os.environ["API_URL"]`, and a compound assignment whose value is dropped, `process.env.API_URL ??= "x";`.
    Write,
    /// A value is stored in the chain and the expression also gives a value that depends on the old one, as a compound assignment or an update whose value is used does: `const url = (process.env.API_URL ??= "x")`. A plain `=` used as a value only stores, so it is a [`Access::Write`].
    ReadWrite,
}

impl ChainRule {
    /// What this rule makes of a member chain that starts at one of its roots, touched as `access` says. `segments` follow the root in order, `None` where a segment is computed (`process.env[name]`); `root` is the root as written.
    ///
    /// A read names the key with [`ChainRule::role`], a write with [`ChainRule::written`] and a read-write with both, so it gives up to two outcomes. A rule with no role for the access gives [`Outcome::Skip`] alone, and a computed segment gives one blind spot whatever the access. A chain the rule's `segments` limit cut short, `process.env.X.foo`, is stored into the value of the variable, so a rule with a written role reads it whatever the access.
    pub fn apply(
        &self,
        pack: &str,
        access: Access,
        segments: &[Option<&str>],
        line: Line,
        root: &str,
    ) -> impl Iterator<Item = Outcome> + use<> {
        let outcomes = if self.roles(access) == [None, None] {
            [Some(Outcome::Skip), None]
        } else {
            match self.key(segments) {
                Err(Computed) => [Some(Outcome::blind(line, root)), None],
                Ok(None) => [Some(Outcome::Skip), None],
                Ok(Some((key, cut))) => {
                    // A member below the variable, `process.env.X.foo = 1`, is stored in its value and sets nothing, so the rule that can tell a write reads it; a rule with no written role has nothing to read it as, and a write to a member of its chain names nothing.
                    let access = if cut && self.written.is_some() {
                        Access::Read
                    } else {
                        access
                    };
                    self.roles(access)
                        .map(|role| role.map(|role| named(&self.kind, role, &key, line, pack)))
                }
            }
        };
        outcomes.into_iter().flatten()
    }

    /// The roles this rule names a chain touched as `access` says, once each: a role that is both read and written is one.
    fn roles(&self, access: Access) -> [Option<Role>; 2] {
        let (read, written) = match access {
            Access::Read => (Some(self.role), None),
            Access::Write => (None, self.written),
            Access::ReadWrite => (Some(self.role), self.written),
        };
        [read, written.filter(|role| Some(*role) != read)]
    }

    /// The key the chain makes and whether the rule's `segments` limit cut the chain short: `Ok(None)` when the segments hold none, `Err` when one of them is computed.
    fn key(&self, segments: &[Option<&str>]) -> Result<Option<(String, bool)>, Computed> {
        let limit = self.segments.map_or(usize::MAX, |count| {
            usize::try_from(count.get()).unwrap_or(usize::MAX)
        });
        let mut parts: Vec<&str> = Vec::new();
        let mut cut = false;
        for segment in segments {
            match segment {
                None => return Err(Computed),
                Some(segment) if self.terminals.iter().any(|terminal| terminal == segment) => break,
                Some(_) if parts.len() == limit => {
                    cut = true;
                    break;
                }
                Some(segment) => parts.push(segment),
            }
        }
        if parts.is_empty() {
            return Ok(None);
        }
        let joined = parts.join(".");
        let key = self.case.map_or(joined.clone(), |case| case.apply(&joined));
        Ok(Some((key, cut)))
    }
}

/// A segment of a chain is computed, so the key cannot be read.
struct Computed;

impl ImportRule {
    /// Whether the rule reads the names imported from the package `spec`.
    pub fn reads(&self, spec: &str) -> bool {
        self.from.iter().any(|from| from == spec)
    }

    /// What this rule makes of `name`, which the file takes from a package it reads and uses as a value: the exported name, not the local one. A name the rule excepts names nothing.
    pub fn apply(&self, pack: &str, name: &str, line: Line) -> Outcome {
        if name.is_empty() || self.except.iter().any(|except| except == name) {
            return Outcome::Skip;
        }
        let key = self
            .case
            .map_or_else(|| name.to_owned(), |case| case.apply(name));
        named(&self.kind, self.role, &key, line, pack)
    }
}

impl EmbedRule {
    /// The document a call carries. A tagged template is the document itself and is read by the connector, not here. A document built at run time is a blind spot.
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
            _ => Err(UnresolvedSite::non_literal(line, callee)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fact::UnresolvedReason;

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

        fn holds(&self, source: &ValueSource) -> bool {
            match source {
                ValueSource::Arg(position) => {
                    self.args.get(position.get() as usize - 1) == Some(&None)
                }
                _ => false,
            }
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

    #[gsttt_scenario::scenario(
        "contract-formats",
        "A computed target with a literal segment is a fact"
    )]
    #[test]
    fn a_computed_http_call_with_a_literal_segment_is_a_fact() {
        let get = rule(serde_json::json!({
            "callee": "*.get", "kind": "http", "role": "calls", "key": "$arg1", "method": "$callee",
        }));
        let site = call("api.get", &[Some("${}/users/${}")]);
        assert_eq!(
            key(get.apply("api", &site, None, 7, site.callee)),
            "http GET /users/{}"
        );
    }

    #[gsttt_scenario::scenario(
        "contract-formats",
        "A target whose path is only parameters is a blind spot"
    )]
    #[test]
    fn a_computed_http_call_of_parameters_alone_is_a_blind_spot() {
        let get = rule(serde_json::json!({
            "callee": "*.get", "kind": "http", "role": "calls", "key": "$arg1", "method": "$callee",
        }));
        for target in ["${}/${}", "${}${}/${}", "https://${}/${}/${}"] {
            let args = [Some(target)];
            let site = call("api.get", &args);
            assert!(
                matches!(
                    get.apply("api", &site, None, 7, site.callee),
                    Outcome::Blind(_)
                ),
                "{target} is a blind spot"
            );
        }
    }

    #[test]
    fn an_env_key_that_is_no_variable_name_is_skipped() {
        let get = rule(serde_json::json!({
            "callee": "*.get", "kind": "env", "role": "calls", "key": "$arg1",
        }));
        let apply =
            |name: &str| get.apply("cfg", &call("cfg.get", &[Some(name)]), None, 3, "cfg.get");
        assert_eq!(key(apply("API_URL")), "env API_URL");
        for path in ["database.host", "a-b", "A B"] {
            assert_eq!(apply(path), Outcome::Skip, "{path}");
        }
        assert!(matches!(apply("${}"), Outcome::Blind(_)));
    }

    #[test]
    fn a_call_on_a_receiver_reads_its_target_behind_the_base() {
        let get = rule(serde_json::json!({
            "callee": "*.get", "kind": "http", "role": "calls", "key": "$arg1", "method": "$callee",
        }));
        let apply = |prefix: Option<&str>, target: &str| {
            let args = [Some(target)];
            let site = call("api.get", &args);
            get.apply("api", &site, prefix, 7, site.callee)
        };
        assert!(matches!(apply(Some(""), "${}${}"), Outcome::Blind(_)));
        assert!(matches!(apply(Some("${}"), "${}"), Outcome::Blind(_)));
        assert_eq!(key(apply(Some("/api"), "${}")), "http GET /api/{}");
        assert_eq!(key(apply(Some(""), "users")), "http GET /users");
        assert_eq!(apply(None, "users"), Outcome::Skip);
    }

    #[gsttt_scenario::scenario("contract-formats", "A computed origin alone is a blind spot")]
    #[test]
    fn http_calls_skip_what_is_not_a_url_and_count_what_is_computed() {
        let get = rule(serde_json::json!({
            "callee": "*.get", "kind": "http", "role": "calls", "key": "$arg1", "method": "$callee",
        }));
        let apply = |site: &Call<'_>| get.apply("api", site, None, 7, site.callee);
        assert_eq!(apply(&call("cache.get", &[Some("user")])), Outcome::Skip);
        // A `://` inside a query string is no origin, so this is a cache key and no URL.
        assert_eq!(
            apply(&call("cache.get", &[Some("cache?next=https://b")])),
            Outcome::Skip
        );
        assert_eq!(
            key(apply(&call("api.get", &[Some("/bookings/${}")]))),
            "http GET /bookings/{}"
        );
        assert_eq!(
            key(apply(&call("api.get", &[Some("${}/bookings")]))),
            "http GET /bookings"
        );
        for computed in [
            None,
            Some("${}"),
            Some("https://${}"),
            Some("${}://${}"),
            Some("${}//${}"),
        ] {
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

    /// The outcomes of a read of a chain, which are one.
    fn read(
        rule: &ChainRule,
        pack: &str,
        segments: &[Option<&str>],
        line: Line,
        root: &str,
    ) -> Outcome {
        let mut outcomes = rule.apply(pack, Access::Read, segments, line, root);
        let outcome = outcomes.next().unwrap();
        assert_eq!(outcomes.next(), None, "a read of a chain is one outcome");
        outcome
    }

    #[test]
    fn chains_stop_at_terminals_and_change_case() {
        let chain =
            |value: serde_json::Value| -> ChainRule { serde_json::from_value(value).unwrap() };
        let prisma = chain(serde_json::json!({
            "root": "prisma", "kind": "table", "role": "calls", "segments": 1,
        }));
        assert_eq!(
            key(read(
                &prisma,
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
            key(read(
                &models,
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
            key(read(
                &trpc,
                "trpc",
                &[Some("booking"), Some("get"), Some("useQuery")],
                2,
                "trpc"
            )),
            "trpc booking.get"
        );
        assert_eq!(
            read(&trpc, "trpc", &[Some("useUtils")], 2, "trpc"),
            Outcome::Skip
        );
        let env =
            chain(serde_json::json!({ "root": "process.env", "kind": "env", "role": "calls" }));
        assert_eq!(
            key(read(&env, "env", &[Some("API_URL")], 4, "process.env")),
            "env API_URL"
        );
        assert!(matches!(
            read(&env, "env", &[None], 4, "process.env"),
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

    #[test]
    fn imports_name_the_exported_name_unless_the_rule_excepts_it() {
        let prisma: ImportRule = serde_json::from_value(serde_json::json!({
            "from": "@prisma/client", "kind": "table", "role": "calls",
            "except": ["Prisma", "PrismaClient"],
        }))
        .unwrap();
        assert!(prisma.reads("@prisma/client"));
        assert!(!prisma.reads("@prisma/client/edge") && !prisma.reads("prisma"));
        assert_eq!(
            key(prisma.apply("prisma", "BookingItem", 3)),
            "table bookingitem"
        );
        assert_eq!(prisma.apply("prisma", "Prisma", 3), Outcome::Skip);
        assert_eq!(prisma.apply("prisma", "", 3), Outcome::Skip);

        let models: ImportRule = serde_json::from_value(serde_json::json!({
            "from": ["models", "@org/models"], "kind": "graphql", "role": "calls", "case": "upper-first",
        }))
        .unwrap();
        assert!(models.reads("@org/models"));
        assert_eq!(key(models.apply("orm", "booking", 1)), "graphql Booking");

        let typo =
            serde_json::json!({ "from": "x", "kind": "table", "role": "calls", "exept": [] });
        assert!(serde_json::from_value::<ImportRule>(typo).is_err());
    }

    #[test]
    fn a_chain_rule_may_list_root_types() {
        let chain =
            |value: serde_json::Value| -> ChainRule { serde_json::from_value(value).unwrap() };
        let typed = chain(serde_json::json!({
            "root": "tx", "root_types": ["PrismaClient", "Prisma.TransactionClient"],
            "kind": "table", "role": "calls",
        }));
        assert_eq!(
            typed.root_types,
            ["PrismaClient", "Prisma.TransactionClient"]
        );
        let plain = chain(serde_json::json!({ "root": "tx", "kind": "table", "role": "calls" }));
        assert_eq!(plain.root_types, Vec::<String>::new());
    }

    /// What `rule` makes of `segments` touched as `access`, as the roles and keys of its facts and the number of unread sites.
    fn touch(
        rule: &ChainRule,
        access: Access,
        segments: &[Option<&str>],
    ) -> (Vec<String>, usize, usize) {
        let mut facts = Vec::new();
        let (mut blind, mut skipped) = (0, 0);
        for outcome in rule.apply("env", access, segments, 4, "process.env") {
            match outcome {
                Outcome::Fact(fact) => facts.push(format!("{} {}", fact.role, fact.key)),
                Outcome::Blind(_) => blind += 1,
                Outcome::Skip => skipped += 1,
            }
        }
        (facts, blind, skipped)
    }

    fn env_chain(written: Option<&str>) -> ChainRule {
        let mut value = serde_json::json!({
            "root": "process.env", "kind": "env", "role": "calls", "segments": 1,
        });
        if let Some(role) = written {
            value["written"] = role.into();
        }
        serde_json::from_value(value).unwrap()
    }

    #[gsttt_scenario::scenario("indexing", "A read, a write and a read-write of one chain")]
    #[test]
    fn a_chain_is_named_by_the_role_of_the_access() {
        let rule = env_chain(Some("serves"));
        let name = [Some("API_URL")];
        assert_eq!(
            touch(&rule, Access::Read, &name),
            (vec!["calls env API_URL".to_owned()], 0, 0)
        );
        assert_eq!(
            touch(&rule, Access::Write, &name),
            (vec!["serves env API_URL".to_owned()], 0, 0)
        );
        assert_eq!(
            touch(&rule, Access::ReadWrite, &name),
            (
                vec![
                    "calls env API_URL".to_owned(),
                    "serves env API_URL".to_owned()
                ],
                0,
                0
            )
        );
    }

    #[gsttt_scenario::scenario(
        "indexing",
        "A write under a rule with no written role names nothing"
    )]
    #[test]
    fn a_write_under_a_rule_with_no_written_role_names_nothing() {
        let rule = env_chain(None);
        for segments in [&[Some("API_URL")][..], &[None][..], &[][..]] {
            assert_eq!(
                touch(&rule, Access::Write, segments),
                (Vec::new(), 0, 1),
                "{segments:?}"
            );
        }
        assert_eq!(
            touch(&rule, Access::ReadWrite, &[Some("API_URL")]),
            (vec!["calls env API_URL".to_owned()], 0, 0),
            "a read-write still reads"
        );
        let packed = serde_json::to_value(&rule).unwrap();
        assert!(
            packed.get("written").is_none(),
            "a rule without it serialises as before, so the hash of the packs does not move"
        );
    }

    #[gsttt_scenario::scenario(
        "indexing",
        "A computed segment is one unread site whatever the access"
    )]
    #[test]
    fn a_computed_segment_is_one_unread_site_whatever_the_access() {
        let rule = env_chain(Some("serves"));
        for access in [Access::Read, Access::Write, Access::ReadWrite] {
            assert_eq!(
                touch(&rule, access, &[None]),
                (Vec::new(), 1, 0),
                "{access:?}"
            );
        }
    }

    #[test]
    fn a_member_below_the_variable_is_a_read_not_a_write() {
        let rule = env_chain(Some("serves"));
        let below = [Some("API_URL"), Some("foo")];
        for access in [Access::Read, Access::Write, Access::ReadWrite] {
            assert_eq!(
                touch(&rule, access, &below),
                (vec!["calls env API_URL".to_owned()], 0, 0),
                "{access:?}"
            );
        }
    }

    #[test]
    fn a_member_below_a_chain_with_no_written_role_names_nothing_when_written() {
        let rule = env_chain(None);
        let below = [Some("user"), Some("foo")];
        assert_eq!(touch(&rule, Access::Write, &below), (Vec::new(), 0, 1));
        assert_eq!(
            touch(&rule, Access::Read, &below),
            (vec!["calls env USER".to_owned()], 0, 0)
        );
    }

    #[test]
    fn a_terminal_is_no_cut() {
        let mut rule = env_chain(Some("serves"));
        rule.terminals = vec!["foo".to_owned()];
        assert_eq!(
            touch(&rule, Access::Write, &[Some("API_URL"), Some("foo")]),
            (vec!["serves env API_URL".to_owned()], 0, 0)
        );
    }

    #[test]
    fn a_role_that_is_read_and_written_is_one_fact_and_one_listed_role() {
        let mut rule = env_chain(Some("calls"));
        assert_eq!(
            touch(&rule, Access::ReadWrite, &[Some("API_URL")]),
            (vec!["calls env API_URL".to_owned()], 0, 0)
        );
        rule.written = Some(Role::Calls);
        let pack = Pack {
            name: "env".to_owned(),
            language: "ts".to_owned(),
            detect: Vec::new(),
            calls: Vec::new(),
            decorators: Vec::new(),
            chains: vec![rule],
            embeds: Vec::new(),
            assigns: Vec::new(),
            imports: Vec::new(),
        };
        assert_eq!(pack.contracts().count(), 1);
    }

    #[test]
    fn a_pack_lists_the_role_of_a_write_beside_the_role_of_a_read() {
        let pack: Pack = serde_json::from_value(serde_json::json!({
            "name": "env", "language": "ts",
            "chains": [{ "root": "process.env", "kind": "env", "role": "calls", "written": "serves" }],
            "calls": [{ "callee": "vi.stubEnv", "kind": "env", "role": "serves", "key": "$arg1" }],
        }))
        .unwrap();
        let listed: Vec<(String, Role)> = pack
            .contracts()
            .map(|(kind, role)| (kind.to_string(), role))
            .collect();
        assert_eq!(
            listed,
            [
                ("env".to_owned(), Role::Serves),
                ("env".to_owned(), Role::Calls),
                ("env".to_owned(), Role::Serves)
            ]
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

        fn holds(&self, _: &ValueSource) -> bool {
            false
        }
    }

    /// What a [`Probe`] carries at one place.
    #[derive(Clone, Copy)]
    enum Slot {
        Text(&'static str),
        Absent,
        Unread,
    }

    impl Slot {
        fn text(self) -> Option<Cow<'static, str>> {
            match self {
                Self::Text(text) => Some(Cow::Borrowed(text)),
                Self::Absent | Self::Unread => None,
            }
        }

        fn unread(self) -> bool {
            matches!(self, Self::Unread)
        }
    }

    /// A site whose first argument, the property `p` of its second and its keyword `k` are each text, absent or unread, on a callee `post` and a member `bookings`.
    struct Probe {
        arg: Slot,
        prop: Slot,
        keyword: Slot,
    }

    impl Site for Probe {
        fn callee_last(&self) -> Option<Cow<'_, str>> {
            Some("post".into())
        }

        fn arg(&self, _: NonZeroU32) -> Option<Cow<'_, str>> {
            self.arg.text()
        }

        fn arg_prop(&self, _: NonZeroU32, _: &str) -> Option<Cow<'_, str>> {
            self.prop.text()
        }

        fn keyword(&self, _: &str) -> Option<Cow<'_, str>> {
            self.keyword.text()
        }

        fn member(&self) -> Option<Cow<'_, str>> {
            Some("bookings".into())
        }

        fn holds(&self, source: &ValueSource) -> bool {
            match source {
                ValueSource::Arg(_) => self.arg.unread(),
                ValueSource::ArgProp(..) => self.prop.unread(),
                ValueSource::Keyword(_) => self.keyword.unread(),
                ValueSource::Callee | ValueSource::Member | ValueSource::Literal(_) => false,
            }
        }
    }

    const SLOTS: [Slot; 3] = [Slot::Text("t"), Slot::Absent, Slot::Unread];

    fn probe(arg: Slot, prop: Slot, keyword: Slot) -> Probe {
        Probe { arg, prop, keyword }
    }

    fn unread_arg() -> Probe {
        probe(Slot::Unread, Slot::Absent, Slot::Absent)
    }

    fn expr(text: &str) -> ValueExpr {
        text.parse().unwrap()
    }

    #[test]
    fn a_literal_stands_in_for_an_absent_argument_not_an_unreadable_one() {
        assert_eq!(expr("$arg1|").eval(&call("Get", &[])), Some(String::new()));
        assert_eq!(expr("$arg1|").eval(&call("Get", &[None])), None);
        assert_eq!(
            expr("$arg1|").eval(&call("Get", &[Some("/x")])),
            Some("/x".to_string())
        );
        let member = Call {
            member: Some("bookings"),
            ..call("Query", &[None])
        };
        assert_eq!(
            expr("$arg1|$member").eval(&member),
            Some("bookings".to_string())
        );
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

    #[gsttt_scenario::scenario(
        "contract-formats",
        "A literal stands in only for a value the site does not carry"
    )]
    #[test]
    fn a_literal_stands_in_only_for_a_value_the_site_does_not_carry() {
        let request = rule(serde_json::json!({
            "callee": "*.post", "kind": "http", "role": "calls", "key": "$arg1", "method": "$arg2.method|GET",
        }));
        let apply = |method: Slot| {
            let site = probe(Slot::Text("/x"), method, Slot::Absent);
            key(request.apply("api", &site, None, 7, "api.post"))
        };
        assert_eq!(apply(Slot::Absent), "http GET /x");
        assert_eq!(apply(Slot::Unread), "http * /x");
        assert_eq!(apply(Slot::Text("PUT")), "http PUT /x");
    }

    #[gsttt_scenario::scenario(
        "contract-formats",
        "A decorator whose own prefix is not text is unread"
    )]
    #[test]
    fn a_decorator_whose_own_prefix_is_not_text_is_unread() {
        let rule: DecoratorRule = serde_json::from_value(serde_json::json!({
            "decorator": "Controller", "kind": "http", "role": "serves",
            "key": "$arg1", "method": "GET", "prefix": "$arg2.prefix",
        }))
        .unwrap();
        let apply = |prefix: Slot| {
            let site = probe(Slot::Text("/x"), prefix, Slot::Absent);
            rule.apply("nestjs", &site, Some("/api"), 4, "Controller")
        };
        let Outcome::Blind(site) = apply(Slot::Unread) else {
            panic!("a prefix that is not text is a blind spot")
        };
        assert_eq!(site.line, 4);
        assert_eq!(site.detail, "Controller");
        assert_eq!(key(apply(Slot::Absent)), "http GET /api/x");
        assert_eq!(key(apply(Slot::Text("v1"))), "http GET /api/v1/x");
    }

    #[test]
    fn a_reading_is_a_prefix_only_when_it_is_written() {
        assert_eq!(Reading::Text("/api".into()).as_prefix(), Some("/api"));
        assert_eq!(Reading::Absent.as_prefix(), Some(""));
        assert_eq!(Reading::Unread.as_prefix(), None);
    }

    #[test]
    fn a_call_is_blind_behind_a_prefix_that_is_not_text_only_when_it_serves_http_routes() {
        let behind = |kind: &str, role: &str, reading: Reading| {
            let rule = rule(serde_json::json!({
                "callee": "x.*", "kind": kind, "role": role, "key": "$arg1",
            }));
            rule.behind(&reading).map(str::to_owned)
        };
        let unread = |kind, role| behind(kind, role, Reading::Unread);
        assert_eq!(unread("http", "serves"), None);
        assert_eq!(unread("http", "calls"), Some(String::new()));
        assert_eq!(unread("graphql", "serves"), Some(String::new()));
        assert_eq!(unread("graphql", "calls"), Some(String::new()));
        for (kind, role) in [("http", "serves"), ("http", "calls"), ("graphql", "serves")] {
            assert_eq!(
                behind(kind, role, Reading::Text("/api".into())),
                Some("/api".into())
            );
            assert_eq!(behind(kind, role, Reading::Absent), Some(String::new()));
        }
    }

    #[test]
    fn only_an_http_rule_that_serves_finds_the_routes_of_a_server() {
        assert!(serves_http(&ContractKind::Http, Role::Serves));
        assert!(!serves_http(&ContractKind::Http, Role::Calls));
        assert!(!serves_http(&ContractKind::Http, Role::Declares));
        assert!(!serves_http(&ContractKind::Graphql, Role::Serves));
    }

    #[test]
    fn read_tells_a_value_the_site_lacks_from_one_it_cannot_read() {
        let (text, absent, unread) = (Slot::Text("t"), Slot::Absent, Slot::Unread);
        let cases = [
            (
                "$callee",
                probe(unread, unread, unread),
                Reading::Text("post".into()),
            ),
            (
                "$member",
                probe(unread, unread, unread),
                Reading::Text("bookings".into()),
            ),
            (
                "GET",
                probe(unread, unread, unread),
                Reading::Text("GET".into()),
            ),
            (
                "$arg1",
                probe(text, absent, absent),
                Reading::Text("t".into()),
            ),
            ("$arg1", probe(absent, text, text), Reading::Absent),
            ("$arg1", probe(unread, text, text), Reading::Unread),
            (
                "$arg2.p",
                probe(absent, text, absent),
                Reading::Text("t".into()),
            ),
            ("$arg2.p", probe(text, absent, text), Reading::Absent),
            ("$arg2.p", probe(text, unread, text), Reading::Unread),
            ("$k", probe(absent, absent, text), Reading::Text("t".into())),
            ("$k", probe(text, text, absent), Reading::Absent),
            ("$k", probe(text, text, unread), Reading::Unread),
            (
                "$arg1|GET",
                probe(absent, absent, absent),
                Reading::Text("GET".into()),
            ),
            ("$arg1|GET", probe(unread, absent, absent), Reading::Unread),
            (
                "$arg1|GET",
                probe(text, absent, absent),
                Reading::Text("t".into()),
            ),
            (
                "$arg2.p|$k|GET",
                probe(absent, unread, text),
                Reading::Text("t".into()),
            ),
            (
                "$arg2.p|$k|GET",
                probe(absent, unread, absent),
                Reading::Unread,
            ),
            (
                "$arg2.p|$k|GET",
                probe(absent, absent, unread),
                Reading::Unread,
            ),
            (
                "$arg2.p|$k|GET",
                probe(absent, absent, absent),
                Reading::Text("GET".into()),
            ),
            ("$arg1|$k", probe(absent, absent, absent), Reading::Absent),
            ("$arg1|$k", probe(unread, absent, absent), Reading::Unread),
            (
                "$arg1|",
                probe(absent, absent, absent),
                Reading::Text(String::new()),
            ),
            ("$arg1|", probe(unread, absent, absent), Reading::Unread),
        ];
        for (text, site, expected) in cases {
            assert_eq!(expr(text).read(&site), expected, "{text}");
        }
    }

    #[test]
    fn eval_is_the_text_of_the_reading_and_nothing_else() {
        let exprs = [
            "$callee",
            "$member",
            "GET",
            "$arg1",
            "$arg2.p",
            "$k",
            "$arg1|GET",
            "$arg2.p|$k|GET",
            "$arg1|$k",
            "$arg1|",
        ];
        for text in exprs {
            let expr = expr(text);
            for arg in SLOTS {
                for prop in SLOTS {
                    for keyword in SLOTS {
                        let site = probe(arg, prop, keyword);
                        let expected = match expr.read(&site) {
                            Reading::Text(text) => Some(text),
                            Reading::Absent | Reading::Unread => None,
                        };
                        assert_eq!(expr.eval(&site), expected, "{text}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_prefix_that_is_not_text_is_unread_and_one_that_is_not_there_is_absent() {
        let prefix: Prefix = serde_json::from_value(serde_json::json!({
            "from": "APIRouter", "key": "$prefix",
        }))
        .unwrap();
        let unkeyed: Prefix =
            serde_json::from_value(serde_json::json!({ "from": "APIRouter" })).unwrap();
        let read = |arg, keyword| probe(arg, Slot::Absent, keyword);
        assert_eq!(
            prefix.reading(&read(Slot::Absent, Slot::Text("/v1"))),
            Reading::Text("/v1".into())
        );
        assert_eq!(
            prefix.reading(&read(Slot::Absent, Slot::Absent)),
            Reading::Absent
        );
        assert_eq!(
            prefix.reading(&read(Slot::Absent, Slot::Unread)),
            Reading::Unread
        );
        assert_eq!(unkeyed.reading(&unread_arg()), Reading::Absent);
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
