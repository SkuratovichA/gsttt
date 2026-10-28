//! Requests made with reqwest, on the `reqwest` fixture.

use gsttt_scenario::scenario;
use insta::assert_json_snapshot;

use crate::support::index;

#[scenario("rust", "A client held in a field calls the URL")]
#[test]
fn a_client_held_in_a_field_calls_the_url() {
    let out = index("reqwest");
    let contracts = out.contracts("src/api.rs");
    assert!(contracts.contains(&"11 calls http GET /bookings/{}".to_owned()));
    assert_json_snapshot!(out.sites("src/api.rs"));
}

#[scenario("rust", "A get on another receiver is no request")]
#[test]
fn a_get_on_another_receiver_is_no_request() {
    let out = index("reqwest");
    let file = out.sites("src/api.rs");
    let at_line_19 = |list: &str| {
        file[list]
            .as_array()
            .unwrap()
            .iter()
            .filter(|site| site["line"] == 19)
            .count()
    };
    assert_eq!(at_line_19("contracts"), 0);
    assert_eq!(at_line_19("unresolved"), 0);
}

#[scenario("rust", "Built URLs keep their literal parts and bare URLs are unread")]
#[test]
fn built_urls_keep_their_literal_parts_and_bare_urls_are_unread() {
    let out = index("reqwest");
    assert_eq!(
        out.contracts("src/client.rs"),
        ["12 calls http DELETE /bookings/{}"]
    );
    let unresolved = out.sites("src/client.rs")["unresolved"].clone();
    assert_eq!(unresolved.as_array().unwrap().len(), 1);
    assert_eq!(unresolved[0]["line"], 13);
    assert_json_snapshot!(out.sites("src/client.rs"));
}

#[test]
fn a_builder_chain_and_a_request_method_are_clients() {
    let out = index("reqwest");
    assert_json_snapshot!(out.sites("src/builder.rs"));
}
