//! HTTP matching: when a call and a route are about the same contract.

use std::collections::BTreeSet;

use gsttt_core::ContractKey;
use gsttt_core::index::Freshness;
use gsttt_core::query::{Found, HttpMatcher, contract};
use gsttt_scenario::scenario;
use proptest::prelude::*;

use crate::fixture::Tree;

fn key(text: &str) -> ContractKey {
    ContractKey::parse(text).unwrap()
}

fn matcher(prefixes: &[&str]) -> HttpMatcher {
    let prefixes: Vec<String> = prefixes.iter().map(|prefix| (*prefix).to_owned()).collect();
    HttpMatcher::new(&prefixes)
}

#[scenario("queries", "A prefix is stripped before matching")]
#[test]
fn a_prefix_is_stripped_before_matching() {
    let call = key("http GET /api/bookings/me");
    let route = key("http GET /bookings/{}");

    let with_prefix = matcher(&["/api"]);
    assert!(with_prefix.meets(&call, &route));
    assert!(with_prefix.meets(&route, &call));
    assert!(!HttpMatcher::default().meets(&call, &route));
}

#[test]
fn prefixes_are_read_as_the_config_spells_them() {
    let call = key("http GET /api/v1/bookings");
    let route = key("http GET /bookings");
    for spelling in ["/api/v1", "api/v1", "/api/v1/", "api/v1/"] {
        assert!(matcher(&[spelling]).meets(&call, &route), "{spelling}");
    }
    assert!(!matcher(&["/api"]).meets(&call, &route));
    assert!(!matcher(&["/ap"]).meets(&key("http GET /api/bookings"), &route));
    assert!(!matcher(&["", "/"]).meets(&call, &route));
}

#[scenario("queries", "A prefix is removed from one path only")]
#[test]
fn a_prefix_is_removed_from_one_path_only() {
    let matcher = matcher(&["/api", "/v1"]);
    let api = key("http GET /api/users");
    let v1 = key("http GET /v1/users");
    let bare = key("http GET /users");

    assert!(!matcher.meets(&api, &v1));
    assert!(!matcher.meets(&v1, &api));
    assert!(matcher.meets(&api, &bare));
    assert!(matcher.meets(&v1, &bare));
}

#[scenario("queries", "A prefix alone is no path")]
#[test]
fn a_prefix_alone_is_no_path() {
    let matcher = matcher(&["/api"]);
    let prefix = key("http GET /api");
    let root = key("http GET /");

    assert!(!matcher.meets(&prefix, &root));
    assert!(!matcher.meets(&root, &prefix));
    assert!(matcher.meets(&prefix, &prefix));
}

#[scenario("queries", "Methods must agree")]
#[test]
fn methods_must_agree() {
    let matcher = HttpMatcher::default();
    let post = key("http POST /bookings");
    let get = key("http GET /bookings");
    let any = key("http * /bookings");

    assert!(!matcher.meets(&post, &get));
    assert!(matcher.meets(&any, &post));
    assert!(matcher.meets(&any, &get));
    assert!(matcher.meets(&get, &any));
}

#[scenario("queries", "Segment counts must agree")]
#[test]
fn segment_counts_must_agree() {
    let matcher = HttpMatcher::default();
    let route = key("http GET /bookings/{}");
    let longer = key("http GET /bookings/{}/items");

    assert!(!matcher.meets(&route, &longer));
    assert!(!matcher.meets(&longer, &route));
    assert!(!matcher.meets(&route, &key("http GET /bookings")));
}

#[test]
fn parameters_meet_literals_on_either_side() {
    let matcher = HttpMatcher::default();
    assert!(matcher.meets(&key("http GET /bookings/{}"), &key("http GET /bookings/me")));
    assert!(matcher.meets(&key("http GET /bookings/me"), &key("http GET /bookings/{}")));
    assert!(!matcher.meets(
        &key("http GET /bookings/me"),
        &key("http GET /bookings/you")
    ));
    assert!(matcher.meets(&key("http GET /{}/{}"), &key("http GET /a/b")));
}

#[test]
fn other_kinds_meet_by_equality() {
    let matcher = matcher(&["/api"]);
    assert!(matcher.meets(&key("table bookings"), &key("table bookings")));
    assert!(!matcher.meets(&key("table bookings"), &key("table booking")));
    assert!(!matcher.meets(&key("graphql Query.bookings"), &key("http GET /bookings")));
}

fn segment() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec!["api", "v1", "bookings", "me", "{}"])
}

fn http_key() -> impl Strategy<Value = ContractKey> {
    (
        prop::sample::select(vec!["GET", "POST", "*"]),
        prop::collection::vec(segment(), 0..5),
    )
        .prop_map(|(method, segments)| {
            ContractKey::http(method, &format!("/{}", segments.join("/")))
        })
}

/// Prefixes of the vocabulary the keys are made of, so that taking one off a path is something that happens, in the
/// spellings a config could use, with the two that are no prefix at all.
fn prefixes() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(
        prop::sample::select(vec![
            "/api",
            "/v1",
            "/api/v1",
            "/bookings",
            "api/v1/",
            "",
            "/",
        ])
        .prop_map(String::from),
        0..4,
    )
}

proptest! {
    // A prefix changes the answer in about one draw in two hundred, so the default 256 draws would often miss it.
    #![proptest_config(ProptestConfig::with_cases(1024))]

    #[scenario("queries", "Meeting is symmetric")]
    #[test]
    fn meeting_is_symmetric(a in http_key(), b in http_key(), prefixes in prefixes()) {
        let matcher = HttpMatcher::new(&prefixes);
        prop_assert_eq!(matcher.meets(&a, &b), matcher.meets(&b, &a));
    }

    #[test]
    fn a_key_meets_itself(a in http_key(), prefixes in prefixes()) {
        prop_assert!(HttpMatcher::new(&prefixes).meets(&a, &a));
    }

    #[test]
    fn prefixes_only_add_matches(a in http_key(), b in http_key(), prefixes in prefixes()) {
        if HttpMatcher::default().meets(&a, &b) {
            prop_assert!(HttpMatcher::new(&prefixes).meets(&a, &b));
        }
    }
}

proptest! {
    /// Looking a key up finds the sites of exactly the routes `HttpMatcher` says meet it, whichever way the index
    /// narrows the routes it compares.
    #[test]
    fn a_lookup_finds_exactly_the_routes_that_meet(
        asked in http_key(),
        routes in prop::collection::vec(http_key(), 0..12),
        prefixes in prefixes(),
    ) {
        let spelled: Vec<&str> = prefixes.iter().map(String::as_str).collect();
        let index = routes
            .iter()
            .enumerate()
            .fold(Tree::new().prefixes(&spelled), |tree, (position, route)| {
                tree.serves(&format!("route{position}.ts"), &route.to_string())
            })
            .build();
        let matcher = HttpMatcher::new(&prefixes);
        let expected: BTreeSet<String> = routes
            .iter()
            .filter(|route| matcher.meets(&asked, route))
            .map(ToString::to_string)
            .collect();

        match contract(&index, Freshness::Fresh, &asked.to_string()).found {
            Found::Key { serves, .. } => {
                let found: BTreeSet<String> = serves.into_iter().map(|site| site.key).collect();
                prop_assert_eq!(found, expected);
            }
            Found::Search { .. } => prop_assert!(expected.is_empty()),
        }
    }
}
