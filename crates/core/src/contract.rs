//! Contracts: the seams between files that imports do not show. An HTTP route joins a controller and the
//! frontend that calls it; a table joins a schema, its migrations and every query; an env var joins
//! `.env.example` and the code that reads it.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The family a contract belongs to. The text form is the first word of a key: `http`, `graphql`, …
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(from = "String", into = "String")]
#[rkyv(derive(PartialEq, Eq, PartialOrd, Ord, Hash))]
pub enum ContractKind {
    /// `http GET /bookings/{}`: method and normalised path.
    Http,
    /// `graphql Query.tenant`: root type and field.
    Graphql,
    /// `grpc booking.v1.BookingService/GetBooking`: package, service and method.
    Grpc,
    /// `trpc booking.get`: the procedure path.
    Trpc,
    /// `table booking`: a table or an ORM model. SQL folds unquoted names and ORMs map them loosely, so table
    /// keys are lower case: Prisma's `model Booking`, `prisma.booking` and `FROM Booking` meet.
    Table,
    /// `queue booking-created`: a topic, queue or event name.
    Queue,
    /// `env DATABASE_URL`. Keys are upper case: pydantic settings read `database_url` from `DATABASE_URL`, and no
    /// project tells two variables apart by case alone.
    Env,
    /// A kind a user rule or an external connector introduced.
    Other(String),
}

impl ContractKind {
    pub fn parse(text: &str) -> Self {
        match text {
            "http" => Self::Http,
            "graphql" => Self::Graphql,
            "grpc" => Self::Grpc,
            "trpc" => Self::Trpc,
            "table" => Self::Table,
            "queue" => Self::Queue,
            "env" => Self::Env,
            other => Self::Other(other.to_owned()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Http => "http",
            Self::Graphql => "graphql",
            Self::Grpc => "grpc",
            Self::Trpc => "trpc",
            Self::Table => "table",
            Self::Queue => "queue",
            Self::Env => "env",
            Self::Other(other) => other,
        }
    }
}

impl From<String> for ContractKind {
    fn from(text: String) -> Self {
        Self::parse(&text)
    }
}

impl From<ContractKind> for String {
    fn from(kind: ContractKind) -> Self {
        kind.as_str().to_owned()
    }
}

impl fmt::Display for ContractKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a site does with a contract.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "lowercase")]
#[rkyv(derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug))]
pub enum Role {
    /// A schema or spec defines it: an OpenAPI document, a `.proto`, a GraphQL SDL, a Prisma model, a migration,
    /// `.env.example`.
    Declares,
    /// Code implements it: a controller, a resolver, a gRPC server.
    Serves,
    /// Code uses it: a fetch, a generated client, a query, an env read.
    Calls,
}

impl Role {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Declares => "declares",
            Self::Serves => "serves",
            Self::Calls => "calls",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A contract's identity, in canonical form: two sites that name the same contract produce equal keys.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
#[rkyv(derive(PartialEq, Eq, PartialOrd, Ord, Hash))]
pub struct ContractKey {
    kind: ContractKind,
    key: String,
}

/// Why a string is not a contract key.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContractKeyError {
    #[error("`{0}` is not a contract key: expected `<kind> <key>`, like `http GET /bookings/{{}}`")]
    Malformed(String),
}

impl ContractKey {
    /// A key of `kind`, put in that kind's canonical form. HTTP keys are `<METHOD> <path>`, with the method in
    /// upper case (`*` when unknown) and the path through [`normalize_http_path`]; table keys are lower case and env
    /// keys upper case; other keys are trimmed.
    pub fn new(kind: ContractKind, key: &str) -> Self {
        let key = key.trim();
        match kind {
            ContractKind::Http => match key.split_once(char::is_whitespace) {
                Some((method, path)) => Self::http(method, path),
                None => Self::http("*", key),
            },
            ContractKind::Table => Self {
                kind,
                key: key.to_lowercase(),
            },
            ContractKind::Env => Self {
                kind,
                key: key.to_uppercase(),
            },
            kind => Self {
                kind,
                key: key.to_owned(),
            },
        }
    }

    /// `http <METHOD> <path>`. A method that names no single verb (`ALL`, `ANY`, empty) becomes `*`, which
    /// matches every method.
    pub fn http(method: &str, path: &str) -> Self {
        let method = method.trim().to_ascii_uppercase();
        let method = match method.as_str() {
            "" | "ALL" | "ANY" => "*".to_owned(),
            _ => method,
        };
        Self {
            kind: ContractKind::Http,
            key: format!("{method} {}", normalize_http_path(path)),
        }
    }

    /// Reads the text form, `<kind> <key>`.
    pub fn parse(text: &str) -> Result<Self, ContractKeyError> {
        let text = text.trim();
        match text.split_once(char::is_whitespace) {
            Some((kind, key)) if !key.trim().is_empty() => {
                Ok(Self::new(ContractKind::parse(kind), key))
            }
            _ => Err(ContractKeyError::Malformed(text.to_owned())),
        }
    }

    pub fn kind(&self) -> &ContractKind {
        &self.kind
    }

    /// The key without its kind: `GET /bookings/{}`.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// For HTTP keys, the method and the path.
    pub fn http_parts(&self) -> Option<(&str, &str)> {
        match self.kind {
            ContractKind::Http => self.key.split_once(' '),
            _ => None,
        }
    }
}

impl TryFrom<String> for ContractKey {
    type Error = ContractKeyError;

    fn try_from(text: String) -> Result<Self, ContractKeyError> {
        Self::parse(&text)
    }
}

impl From<ContractKey> for String {
    fn from(key: ContractKey) -> Self {
        key.to_string()
    }
}

impl fmt::Display for ContractKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.kind, self.key)
    }
}

/// What follows the scheme and host of `url`: `https://api.example.com/v1/x` gives `/v1/x`, `http://h` gives the empty string and `http://h?next=/a` gives `?next=/a`, because a host ends at the first `/`, `?` or `#`. `None` when `url` has no origin.
///
/// An origin is a scheme, `://` and a host at the start of the text. A scheme is a letter followed by letters, digits, `+`, `-` or `.` (RFC 3986 section 3.1). It may also be written as placeholders, because the connectors write a computed part as `${}` and OpenAPI writes a server variable as `{name}`: it may start with `$` or `{` and hold `$`, `{` and `}` as well. It never holds `/`, so the `://` of a query string or of a deeper path segment, as in `/a?next=https://b/c`, is no origin.
///
/// A protocol that holds its own colon, as in the template `${location.protocol}//${location.host}/api/x`, leaves `${}//${}/api/x`, which is an origin too: a placeholder, `//` and a host that holds a placeholder. `${}//api/x` is not, because that is a base URL with a trailing slash.
pub fn path_after_origin(url: &str) -> Option<&str> {
    if let Some((scheme, rest)) = url.split_once("://")
        && is_scheme(scheme)
    {
        return Some(split_host(rest).1);
    }
    let (scheme, rest) = url.split_once("//")?;
    let (host, path) = split_host(rest);
    (is_scheme(scheme) && is_placeholder(scheme) && is_placeholder(host)).then_some(path)
}

/// Whether `text` can be the scheme of a URL, see [`path_after_origin`].
fn is_scheme(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || matches!(first, '$' | '{'))
        && characters.all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '+' | '-' | '.' | '$' | '{' | '}')
        })
}

/// Splits the text that follows `//` into the host, which ends at the first `/`, `?` or `#`, and the rest.
fn split_host(rest: &str) -> (&str, &str) {
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    rest.split_at_checked(end).unwrap_or_default()
}

/// Puts an HTTP path in the form servers and clients are matched on.
///
/// - A scheme and host go: `https://api.example.com/v1/x` becomes `/v1/x`, and so does `${}://${}/v1/x` with a scheme and host written as placeholders. Only text that starts with an origin has a host, see [`path_after_origin`]: in `/a?next=https://b/c` the path `/a` stays, and the host of `http://h?next=/a` ends at the `?`, so that path is `/`.
/// - The query string and fragment go.
/// - Parameter segments become `{}`: `:id`, `{id}`, `{id:int}`, `<id>`, `<int:id>`, any segment that holds a `{` followed later by a `}` or a `<` followed later by a `>`, such as the custom methods `{id}:cancel` and `<id>:cancel`, and any segment holding a `${…}` or `{}` placeholder from a template string or a concatenation.
/// - A placeholder before the first `/` is a base URL and goes: `${API}/bookings` becomes `/bookings`.
/// - Repeated and trailing slashes go, and so does the whitespace around a segment; the result starts with `/`.
pub fn normalize_http_path(raw: &str) -> String {
    let raw = raw.trim();
    let without_host = path_after_origin(raw).unwrap_or(raw);
    let path = without_host.split(['?', '#']).next().unwrap_or("");
    let mut segments = path.split('/').map(str::trim).peekable();
    if !path.starts_with('/') && segments.peek().is_some_and(|first| is_placeholder(first)) {
        segments.next();
    }
    let mut out = String::with_capacity(path.len() + 1);
    for segment in segments.filter(|segment| !segment.is_empty()) {
        out.push('/');
        out.push_str(if is_param(segment) { "{}" } else { segment });
    }
    if out.is_empty() {
        out.push('/');
    }
    out
}

fn is_placeholder(segment: &str) -> bool {
    segment.contains("${") || segment.contains("{}")
}

fn is_param(segment: &str) -> bool {
    // A `{…}` or `<…>` anywhere in a segment makes it a parameter here and not in `is_placeholder`, which also decides that a placeholder before the first `/` is a base URL that goes, while `{tenant}/x` keeps its first segment as a parameter.
    is_placeholder(segment)
        || segment.starts_with(':')
        || holds_pair(segment, '{', '}')
        || holds_pair(segment, '<', '>')
}

/// Whether `segment` holds `open` and, later, `close`.
fn holds_pair(segment: &str, open: char, close: char) -> bool {
    segment
        .split_once(open)
        .is_some_and(|(_, after)| after.contains(close))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[gsttt_scenario::scenario("indexing", "Paths from every framework meet")]
    #[test]
    fn http_paths_from_every_framework_meet() {
        let spellings = [
            "/bookings/:id",
            "/bookings/{id}",
            "/bookings/{id:int}",
            "/bookings/<int:id>",
            "bookings/${id}/",
            "${API_URL}/bookings/${booking.id}?expand=1",
            "https://api.example.com/bookings/{}#top",
            "//bookings//{}",
        ];
        for spelling in spellings {
            assert_eq!(normalize_http_path(spelling), "/bookings/{}", "{spelling}");
        }
        assert_eq!(normalize_http_path(""), "/");
        // Whitespace that the cut of a query string or the end of the path leaves at the end of a segment goes with it.
        assert_eq!(normalize_http_path("a #"), "/a");
        assert_eq!(normalize_http_path("/a /?x"), "/a");
        assert_eq!(normalize_http_path("/${tenant}/x"), "/{}/x");
        assert_eq!(normalize_http_path("{tenant}/x"), "/{}/x");
        // A bracket that is never closed, or that closes before it opens, makes no parameter.
        assert_eq!(normalize_http_path("/a/}x{"), "/a/}x{");
        assert_eq!(normalize_http_path("/a/b{c"), "/a/b{c");
        assert_eq!(normalize_http_path("/a/b<c"), "/a/b<c");
        assert_eq!(normalize_http_path("/a/>b<"), "/a/>b<");
    }

    #[gsttt_scenario::scenario(
        "contract-formats",
        "A parameter with a custom method is still a parameter"
    )]
    #[test]
    fn a_parameter_with_a_custom_method_is_still_a_parameter() {
        for path in [
            "/v1/things/{id}:cancel",
            "/v1/things/<id>:cancel",
            "/v1/things/<int:id>:cancel",
        ] {
            assert_eq!(normalize_http_path(path), "/v1/things/{}", "{path}");
        }
    }

    #[test]
    fn an_origin_is_a_scheme_at_the_start_of_the_text() {
        assert_eq!(
            path_after_origin("https://api.example.com/v1/x"),
            Some("/v1/x")
        );
        assert_eq!(path_after_origin("http://h"), Some(""));
        assert_eq!(path_after_origin("ftp://h/a/b"), Some("/a/b"));
        assert_eq!(path_after_origin("git+ssh://h/a"), Some("/a"));
        assert_eq!(path_after_origin("a.b-c://h"), Some(""));
        assert_eq!(path_after_origin("/v1/x"), None);
        // A scheme starts with a letter, and a text with nothing before the `://` has none.
        assert_eq!(path_after_origin("1x://h/a"), None);
        assert_eq!(path_after_origin("://h/x"), None);
    }

    #[gsttt_scenario::scenario(
        "contract-formats",
        "A scheme and host written as placeholders are an origin"
    )]
    #[test]
    fn a_scheme_and_host_written_as_placeholders_are_an_origin() {
        for (url, after_origin, path) in [
            ("${}://${}/bookings/{}", "/bookings/{}", "/bookings/{}"),
            ("{}://{}/bookings/{}", "/bookings/{}", "/bookings/{}"),
            ("http${}://h/bookings/{}", "/bookings/{}", "/bookings/{}"),
            ("{scheme}://{host}:{port}/v1", "/v1", "/v1"),
            ("${}://${}", "", "/"),
            // The protocol of `${location.protocol}//${location.host}/api/x` holds its own colon.
            ("${}//${}/api/x", "/api/x", "/api/x"),
        ] {
            assert_eq!(path_after_origin(url), Some(after_origin), "{url}");
            assert_eq!(normalize_http_path(url), path, "{url}");
        }
        // A placeholder, `//` and a literal is a base URL with a trailing slash, and a base URL and a parameter are no origin.
        assert_eq!(path_after_origin("${}//api/x"), None);
        assert_eq!(normalize_http_path("${}//api/x"), "/api/x");
        assert_eq!(path_after_origin("${}/${}/x"), None);
        assert_eq!(normalize_http_path("${}/${}/x"), "/{}/x");
    }

    #[gsttt_scenario::scenario("contract-formats", "A URL in a query string is no origin")]
    #[test]
    fn a_url_in_a_query_string_is_no_origin() {
        assert_eq!(path_after_origin("/a?next=https://b/c"), None);
        assert_eq!(path_after_origin("a?next=https://b/c"), None);
        assert_eq!(normalize_http_path("/a?next=https://b/c"), "/a");
    }

    #[gsttt_scenario::scenario("contract-formats", "The host ends at a query string")]
    #[test]
    fn the_host_ends_at_a_query_string_or_a_fragment() {
        assert_eq!(path_after_origin("http://h?next=/a"), Some("?next=/a"));
        assert_eq!(path_after_origin("http://h#/a"), Some("#/a"));
        assert_eq!(normalize_http_path("http://h?next=/a"), "/");
        assert_eq!(normalize_http_path("http://h#/a"), "/");
    }

    #[test]
    fn keys_round_trip_through_text() {
        let key = ContractKey::parse("http get /bookings/:id").unwrap();
        assert_eq!(key.to_string(), "http GET /bookings/{}");
        assert_eq!(key.http_parts(), Some(("GET", "/bookings/{}")));
        assert_eq!(ContractKey::parse(&key.to_string()).unwrap(), key);
        assert_eq!(
            ContractKey::parse("http /x").unwrap().to_string(),
            "http * /x"
        );
        assert_eq!(ContractKey::http("all", "/x").to_string(), "http * /x");
        assert_eq!(
            ContractKey::parse("graphql  Query.tenant ").unwrap().key(),
            "Query.tenant"
        );
        assert_eq!(
            ContractKey::new(ContractKind::Table, "BookingItem"),
            ContractKey::parse("table bookingitem").unwrap()
        );
        assert_eq!(
            ContractKey::new(ContractKind::Env, "database_url"),
            ContractKey::parse("env DATABASE_URL").unwrap()
        );
        assert!(ContractKey::parse("http").is_err());
    }

    /// Text that reaches the edges of a path, with its separators, brackets, placeholders and a space, and any text at all.
    fn texts() -> impl Strategy<Value = String> {
        prop_oneof!["[a-z/:{}$<>?#.+ -]{0,24}", ".{0,40}"]
    }

    proptest! {
        #[test]
        fn a_normalised_path_starts_with_a_slash_and_normalises_to_itself(raw in texts()) {
            let path = normalize_http_path(&raw);
            prop_assert!(path.starts_with('/'), "{path:?}");
            prop_assert!(!path.contains(['?', '#']), "{path:?}");
            prop_assert_eq!(normalize_http_path(&path), path);
        }

        #[test]
        fn what_follows_an_origin_is_the_end_of_the_text(raw in texts()) {
            if let Some(rest) = path_after_origin(&raw) {
                prop_assert!(raw.ends_with(rest), "{rest:?}");
            }
        }
    }
}
