//! Which HTTP contract keys meet. A call and the route it reaches are rarely spelled alike: the method may be a
//! wildcard, a gateway may put a prefix in front of the route, and parameters have no name in a key.

use crate::contract::{ContractKey, normalize_http_path};

/// Decides whether two contract keys meet, under the path prefixes a gateway or proxy puts in front of routes.
///
/// Two HTTP keys meet when their methods are equal or either is `*`, and their paths have the same number of
/// segments, each pair equal or `{}` on either side. A prefix may be taken off the start of one of the paths, never
/// of both: `/api/users` meets `/users`, but not `/v1/users`, which is another route behind another gateway. Taking
/// a prefix off is optional, so adding a prefix never breaks a match, and it must leave a path: `/api` is a prefix,
/// not a route, and does not meet `/`. Any other two keys meet when they are equal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HttpMatcher {
    /// The prefixes in the form key paths take: `/api/v1`, never `api/v1/`. The root is not a prefix.
    prefixes: Vec<String>,
}

impl HttpMatcher {
    /// A matcher for `prefixes` as a config spells them: `/api`, `api/`, `/api/v1`.
    pub fn new(prefixes: &[String]) -> Self {
        let mut prefixes: Vec<String> = prefixes
            .iter()
            .map(|prefix| normalize_http_path(prefix))
            .filter(|prefix| prefix != "/")
            .collect();
        prefixes.sort_unstable();
        prefixes.dedup();
        Self { prefixes }
    }

    /// Whether a site of `a` and a site of `b` are about the same contract. The relation is symmetric.
    pub fn meets(&self, a: &ContractKey, b: &ContractKey) -> bool {
        match (a.http_parts(), b.http_parts()) {
            (Some((method_a, path_a)), Some((method_b, path_b))) => {
                methods_meet(method_a, method_b) && self.paths_meet(path_a, path_b)
            }
            _ => a == b,
        }
    }

    /// How many segments `path` has as written and with each prefix taken off, which are the lengths at which it can
    /// meet another path.
    pub(super) fn lengths<'a>(&'a self, path: &'a str) -> impl Iterator<Item = usize> {
        std::iter::once(path)
            .chain(self.stripped(path))
            .map(|form| segments(form).count())
    }

    fn paths_meet(&self, a: &str, b: &str) -> bool {
        segments_meet(a, b)
            || self.stripped(a).any(|a| segments_meet(a, b))
            || self.stripped(b).any(|b| segments_meet(a, b))
    }

    /// `path` with each prefix taken off its start.
    fn stripped<'a>(&'a self, path: &'a str) -> impl Iterator<Item = &'a str> {
        self.prefixes
            .iter()
            .filter_map(move |prefix| strip_segments(path, prefix))
    }
}

fn methods_meet(a: &str, b: &str) -> bool {
    a == b || a == "*" || b == "*"
}

/// `path` without `prefix`, when `prefix` is made of whole leading segments of it and something is left. A path
/// always starts with `/`, so what is left starts with one exactly when it holds a segment.
fn strip_segments<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    path.strip_prefix(prefix)
        .filter(|rest| rest.starts_with('/'))
}

fn segments(path: &str) -> impl Iterator<Item = &str> {
    path.split('/').filter(|segment| !segment.is_empty())
}

fn segments_meet(a: &str, b: &str) -> bool {
    let mut a = segments(a);
    let mut b = segments(b);
    loop {
        match (a.next(), b.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) if x == y || x == "{}" || y == "{}" => {}
            _ => return false,
        }
    }
}
