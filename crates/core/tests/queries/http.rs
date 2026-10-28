//! HTTP matching: when a call and a route are about the same contract.

use gsttt_core::ContractKey;
use gsttt_core::query::HttpMatcher;
use gsttt_scenario::scenario;
use proptest::prelude::*;

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

/// Prefixes as a config could spell them, including shapes that are no sensible prefix at all.
fn prefixes() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec("[a-z/{}]{0,12}", 0..4)
}

proptest! {
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
