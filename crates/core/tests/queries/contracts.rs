//! `contract`: the sites of one key by role, and a search over the keys.

use gsttt_core::contract::Role;
use gsttt_core::index::Freshness;
use gsttt_core::query::{ContractLookup, ContractLookupError, Found, Match, Site, contract};
use gsttt_core::testing::Tree;
use gsttt_scenario::scenario;

fn lookup(tree: Tree, query: &str) -> ContractLookup {
    contract(&tree.build(), Freshness::Fresh, query).unwrap()
}

/// Path, line and key of each site, which is what a reader of the answer sees.
fn places(sites: &[Site]) -> Vec<(&str, u32, &str)> {
    sites
        .iter()
        .map(|site| (site.path.as_str(), site.line, site.key.as_str()))
        .collect()
}

fn key_lookup(answer: ContractLookup) -> (String, Vec<Site>, Vec<Site>, Vec<Site>) {
    match answer.found {
        Found::Key {
            key,
            declares,
            serves,
            calls,
        } => (key, declares, serves, calls),
        Found::Search { .. } => panic!("expected the sites of a key, got a search"),
    }
}

fn matches(answer: ContractLookup) -> Vec<Match> {
    match answer.found {
        Found::Search { matches } => matches,
        Found::Key { key, .. } => panic!("expected a search, got the sites of {key}"),
    }
}

fn found(key: &str, declares: u32, serves: u32, calls: u32) -> Match {
    Match {
        key: key.to_owned(),
        declares,
        serves,
        calls,
    }
}

#[scenario("queries", "A route lists its callers")]
#[test]
fn a_route_lists_its_callers() {
    let tree = Tree::new()
        .serves("bookings.controller.ts", "http GET /bookings/{}")
        .calls("app.ts", "http GET /bookings/me");

    let answer = lookup(tree, "http GET /bookings/{}");

    let text = answer.to_string();
    let (key, declares, serves, calls) = key_lookup(answer);
    assert_eq!(key, "http GET /bookings/{}");
    assert_eq!(places(&declares), Vec::new());
    assert_eq!(
        places(&serves),
        [("bookings.controller.ts", 1, "http GET /bookings/{}")]
    );
    assert_eq!(places(&calls), [("app.ts", 1, "http GET /bookings/me")]);
    assert!(
        text.contains("serves (1):\nbookings.controller.ts:1  via nestjs\n"),
        "{text}"
    );
    assert!(
        text.contains("calls (1):\napp.ts:1  http GET /bookings/me  via fetch\n"),
        "{text}"
    );
}

#[scenario("queries", "Free text searches keys")]
#[test]
fn free_text_searches_keys() {
    let tree = Tree::new()
        .read_by("sql", "schema.sql")
        .declares("schema.sql", "table bookings")
        .read_by("graphql", "schema.graphql")
        .declares("schema.graphql", "graphql Query.bookings")
        .read_by("env", ".env")
        .declares(".env", "env API_URL");

    let answer = lookup(tree, "booking");

    let text = answer.to_string();
    assert_eq!(
        matches(answer),
        [
            found("graphql Query.bookings", 1, 0, 0),
            found("table bookings", 1, 0, 0)
        ]
    );
    assert!(!text.contains("API_URL"), "{text}");
    assert!(
        text.contains("graphql Query.bookings  declares 1, serves 0, calls 0\n"),
        "{text}"
    );
}

#[scenario("queries", "A blank query is an error")]
#[test]
fn a_blank_query_is_an_error_and_not_a_list_of_every_contract() {
    let index = Tree::new()
        .declares("schema.sql", "table bookings")
        .declares("schema.graphql", "graphql Query.bookings")
        .build();

    for blank in ["", "   ", "\t\n"] {
        let error = contract(&index, Freshness::Fresh, blank).unwrap_err();

        assert_eq!(error, ContractLookupError::Blank, "{blank:?}");
    }
}

#[test]
fn the_error_for_a_blank_query_says_what_to_ask_for() {
    assert_eq!(
        ContractLookupError::Blank.to_string(),
        "no contract was asked for: expected a key like `http GET /bookings/{}`, or part of one"
    );
}

#[test]
fn a_search_counts_the_sites_of_each_role() {
    let tree = Tree::new()
        .declares("schema.sql", "table bookings")
        .serves("repo.ts", "table bookings")
        .calls("a.ts", "table bookings")
        .site("b.ts", "table bookings", Role::Calls, 5)
        .site("b.ts", "table bookings", Role::Calls, 9);

    assert_eq!(
        matches(lookup(tree, "bookings")),
        [found("table bookings", 1, 1, 3)]
    );
}

#[test]
fn a_search_ignores_case() {
    let tree = || {
        Tree::new()
            .declares("schema.graphql", "graphql Query.Bookings")
            .declares("schema.sql", "table bookings")
    };

    assert_eq!(
        matches(lookup(tree(), "BOOKINGS")),
        matches(lookup(tree(), "bookings"))
    );
    assert_eq!(matches(lookup(tree(), "bookings")).len(), 2);
}

#[test]
fn a_key_is_found_in_any_spelling_that_canonicalises_to_it() {
    let tree = || {
        Tree::new()
            .serves("controller.ts", "http GET /bookings/{}")
            .calls("app.ts", "http GET /bookings/me")
    };

    let canonical = key_lookup(lookup(tree(), "http GET /bookings/{}"));
    let spelled = key_lookup(lookup(tree(), "  http get /bookings/:id "));

    assert_eq!(canonical, spelled);
}

#[test]
fn an_http_key_lists_every_key_that_meets_it_with_its_own_key() {
    let tree = Tree::new()
        .prefixes(&["/api"])
        .serves("controller.ts", "http GET /bookings/{}")
        .calls("one.ts", "http GET /api/bookings/me")
        .calls("two.ts", "http GET /bookings/{}")
        .calls("three.ts", "http POST /bookings/{}");

    let (_, _, serves, calls) = key_lookup(lookup(tree, "http GET /bookings/{}"));

    assert_eq!(
        places(&serves),
        [("controller.ts", 1, "http GET /bookings/{}")]
    );
    assert_eq!(
        places(&calls),
        [
            ("one.ts", 1, "http GET /api/bookings/me"),
            ("two.ts", 1, "http GET /bookings/{}")
        ]
    );
}

#[test]
fn a_key_nobody_has_falls_back_to_a_search_and_says_what_it_searched() {
    let tree = Tree::new()
        .declares("schema.sql", "table bookings")
        .declares("schema.sql", "table guests");

    let answer = lookup(tree, "table zebras");

    assert_eq!(matches(answer.clone()), Vec::new());
    assert!(
        answer
            .to_string()
            .starts_with("no contract key contains `table zebras` among 2 contracts\n"),
        "{answer}"
    );
}

#[test]
fn a_key_that_is_part_of_another_is_searched_not_looked_up() {
    let tree = Tree::new().declares("schema.sql", "table bookings");

    assert_eq!(
        matches(lookup(tree, "table book")),
        [found("table bookings", 1, 0, 0)]
    );
}

#[test]
fn sites_are_listed_by_path_and_line() {
    let tree = Tree::new()
        .site("b.ts", "queue emails", Role::Calls, 9)
        .site("a.ts", "queue emails", Role::Calls, 30)
        .site("a.ts", "queue emails", Role::Calls, 4);

    let (_, _, _, calls) = key_lookup(lookup(tree, "queue emails"));

    assert_eq!(
        places(&calls),
        [
            ("a.ts", 4, "queue emails"),
            ("a.ts", 30, "queue emails"),
            ("b.ts", 9, "queue emails")
        ]
    );
}
