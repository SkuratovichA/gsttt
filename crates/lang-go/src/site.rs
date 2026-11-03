//! A call as the rules of the packs see it.

use std::borrow::Cow;
use std::num::NonZeroU32;

use gsttt_core::fact::{Embedded, Line, UnresolvedSite};
use gsttt_core::rules::{CallRule, Outcome, Site, ValueSource, serves_http};
use tree_sitter::Node;

use crate::routers::Receiver;
use crate::syntax::{line_of, text_of};
use crate::value::Reader;

/// A callee as written, with the qualifier of an import replaced by the name its package is known by.
#[derive(Debug)]
pub(crate) struct Callee {
    pub(crate) written: String,
    /// The call goes into a package of this repository, which no rule of a pack describes.
    pub(crate) local: bool,
}

/// What the rules of the packs made of one call.
#[derive(Debug, Default)]
pub(crate) struct Read {
    pub(crate) outcomes: Vec<Outcome>,
    pub(crate) embedded: Vec<Embedded>,
    pub(crate) unresolved: Vec<UnresolvedSite>,
}

/// A call site over the syntax tree of Go.
struct GoSite<'r, 'a> {
    reader: Reader<'r, 'a>,
    arguments: &'r [Node<'a>],
    callee: &'r str,
    /// The method a route is limited to by `.Methods(…)`, which takes the place of the one its pattern names.
    method: Option<&'r str>,
}

impl GoSite<'_, '_> {
    fn text_at(&self, index: usize) -> Option<String> {
        self.reader.text(*self.arguments.get(index)?)
    }

    /// An argument that exists and is no text: the rule may not read it, and a call that passes one is a blind spot.
    fn unread_at(&self, index: usize) -> bool {
        self.arguments.get(index).is_some() && self.text_at(index).is_none()
    }

    /// An argument that is a name holding a value of a named type, such as the `ctx context.Context` of `h.Handle(ctx, rec)`: the type tells it is no string, so a rule that wants text there has no call to read.
    fn typed_at(&self, index: usize) -> bool {
        self.arguments.get(index).is_some_and(|argument| {
            argument.kind() == "identifier"
                && self
                    .reader
                    .scopes
                    .find(text_of(*argument, self.reader.cx.source))
                    .is_some_and(|binding| binding.holds.is_some())
        })
    }

    /// Whether the key of `rule` is read from an argument that is typed, so that the call is no site of the rule rather than a site it cannot read.
    fn key_is_typed(&self, rule: &CallRule) -> bool {
        rule.key.sources().iter().any(|source| match source {
            ValueSource::Arg(position) | ValueSource::ArgEach(position) => {
                usize::try_from(position.get().saturating_sub(1)).is_ok_and(|at| self.typed_at(at))
            }
            // A text that gives no path is no pattern, which is not a value the site fails to read.
            ValueSource::Keyword(name) if name.starts_with("pattern_") => {
                self.typed_at(0) || self.text_at(0).is_some()
            }
            _ => false,
        })
    }
}

impl Site for GoSite<'_, '_> {
    fn callee_last(&self) -> Option<Cow<'_, str>> {
        self.callee.rsplit('.').next().map(Cow::Borrowed)
    }

    fn arg(&self, position: NonZeroU32) -> Option<Cow<'_, str>> {
        let index = usize::try_from(position.get().saturating_sub(1)).ok()?;
        self.text_at(index).map(Cow::Owned)
    }

    fn arg_prop(&self, _position: NonZeroU32, _prop: &str) -> Option<Cow<'_, str>> {
        None
    }

    /// Go 1.22 patterns, `"GET /users/{id}"`: the method is the word before the first space when it is upper case, and the rest is the path.
    fn keyword(&self, name: &str) -> Option<Cow<'_, str>> {
        let pattern = self.text_at(0)?;
        let (method, path) = match pattern.split_once(' ') {
            Some((method, path))
                if !method.is_empty() && method.chars().all(|c| c.is_ascii_uppercase()) =>
            {
                (Some(method.to_owned()), path.trim_start().to_owned())
            }
            _ => (None, pattern),
        };
        match name {
            // `net/http` takes no pattern without a `/`: such a text is the verb of another router's `Handle("GET", "/x", h)`.
            "pattern_path" => path.contains('/').then_some(Cow::Owned(path)),
            "pattern_method" => self
                .method
                .map(Cow::Borrowed)
                .or_else(|| method.map(Cow::Owned)),
            _ => None,
        }
    }

    fn holds(&self, source: &ValueSource) -> bool {
        match source {
            ValueSource::Arg(position)
            | ValueSource::ArgEach(position)
            | ValueSource::ArgProp(position, _) => {
                usize::try_from(position.get().saturating_sub(1))
                    .is_ok_and(|index| self.unread_at(index))
            }
            ValueSource::Keyword(name) if name.starts_with("pattern_") => self.unread_at(0),
            ValueSource::Keyword(_)
            | ValueSource::Callee
            | ValueSource::Member
            | ValueSource::Literal(_) => false,
        }
    }
}

/// Applies the call, embed and server rules of the packs in force to one call. `receiver` tells what the file shows of the value the method is called on, and `methods` are the verbs `.Methods(…)` limits the route to.
pub(crate) fn read_calls<'a>(
    reader: Reader<'_, 'a>,
    call: Node<'a>,
    arguments: &[Node<'a>],
    callee: &Callee,
    receiver: &dyn Fn() -> Receiver,
    methods: &[String],
) -> Read {
    let line = line_of(call);
    let cx = reader.cx;
    let mut site = GoSite {
        reader,
        arguments,
        callee: &callee.written,
        method: None,
    };
    let mut read = Read::default();
    let mut unread = false;
    let mut found: Option<Receiver> = None;
    for pack in cx.packs {
        for rule in &pack.calls {
            if !rule.callee.matches(&callee.written) {
                continue;
            }
            let serves = serves_http(&rule.kind, rule.role);
            // Every router verb takes a handler after the path, so a call with one argument is the request of a test client.
            if serves && arguments.len() < 2 {
                continue;
            }
            let gated = !rule.receiver_types.is_empty() || rule.receiver.is_some();
            let receiver = (gated || serves).then(|| &*found.get_or_insert_with(receiver));
            if gated && !receiver.is_some_and(|receiver| receiver.passes(rule)) {
                continue;
            }
            let behind = receiver
                .and_then(Receiver::prefix)
                .map_or(Ok(None), |reading| rule.behind(reading));
            let verbs: Vec<Option<&str>> = if serves && !methods.is_empty() {
                methods.iter().map(|method| Some(method.as_str())).collect()
            } else {
                vec![None]
            };
            for verb in verbs {
                site.method = verb;
                let outcome = match behind {
                    Ok(prefix) => rule.apply(
                        &pack.name,
                        pack.relative_without_base,
                        &site,
                        prefix,
                        line,
                        &callee.written,
                    ),
                    Err(_) => Outcome::blind(line, &callee.written),
                };
                if matches!(outcome, Outcome::Blind(_)) && site.key_is_typed(rule) {
                    continue;
                }
                // Several rules may read one call and each reports the same blind spot: it is one unread site.
                if matches!(outcome, Outcome::Blind(_)) {
                    if unread {
                        continue;
                    }
                    unread = true;
                }
                read.outcomes.push(outcome);
            }
        }
        for rule in &pack.embeds {
            if !rule.callee.matches(&callee.written) {
                continue;
            }
            if rule.text.is_none() && site.text_at(0).is_none() {
                continue;
            }
            match rule.apply(&pack.name, &site, line, &callee.written) {
                Ok(mut embedded) => {
                    embedded.line = first_text_line(&site, arguments).unwrap_or(line);
                    read.embedded.push(embedded);
                }
                Err(blind) => read.unresolved.push(blind),
            }
        }
    }
    // A call one rule reads is no blind spot because another cannot: `r.Handle(verb, "/x", h)` on a gin engine is unread only by the rule of `net/http`.
    if read
        .outcomes
        .iter()
        .any(|outcome| matches!(outcome, Outcome::Fact(_)))
    {
        read.outcomes
            .retain(|outcome| !matches!(outcome, Outcome::Blind(_)));
    }
    read
}

/// The document starts where the first argument that is text does, which is not the line of the call when the argument is on a line of its own.
fn first_text_line(site: &GoSite<'_, '_>, arguments: &[Node<'_>]) -> Option<Line> {
    arguments
        .iter()
        .enumerate()
        .find(|(index, _)| site.text_at(*index).is_some())
        .map(|(_, argument)| line_of(*argument))
}
