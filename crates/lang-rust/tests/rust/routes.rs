//! Routes served by axum and actix, on the `axum` fixture.

use gsttt_scenario::scenario;
use insta::assert_json_snapshot;

use crate::support::index;

#[scenario("rust", "An axum route serves each of its methods")]
#[test]
fn an_axum_route_serves_each_of_its_methods() {
    let out = index("axum");
    assert_eq!(
        out.contracts("src/main.rs"),
        [
            "12 serves http DELETE /bookings/{}",
            "12 serves http GET /bookings/{}",
            "13 serves http POST /bookings",
            "14 serves http TRACE /probe",
            "15 serves http * /anything",
        ]
    );
    assert_json_snapshot!(out.sites("src/main.rs"));
}

#[test]
fn actix_attributes_resources_and_routes_serve_their_paths() {
    let out = index("actix");
    assert_json_snapshot!(out.sites("src/main.rs"));
}

#[scenario("rust", "An actix route attribute serves the TRACE method")]
#[test]
fn an_actix_route_attribute_serves_the_trace_method() {
    let out = index("actixtrace");
    assert_eq!(out.contracts("src/main.rs"), ["3 serves http TRACE /x"]);
}

#[scenario("rust", "A nested router in another file serves under the prefix")]
#[test]
fn a_nested_router_in_another_file_serves_under_the_prefix() {
    let out = index("nested");
    assert_eq!(
        out.linked("src/bookings.rs"),
        ["5 serves http GET /api/bookings/{}"]
    );
    // The output keeps the site the file declares, and the graph drops it, because the linked fact keys it again.
    assert_eq!(out.contracts("src/bookings.rs"), ["5 serves http GET /{}"]);
    assert_eq!(out.linked("src/main.rs"), Vec::<String>::new());
}

#[scenario("rust", "An actix scope prefixes a handler from another file")]
#[test]
fn an_actix_scope_prefixes_a_handler_from_another_file() {
    let out = index("actix");
    assert_eq!(
        out.linked("src/bookings.rs"),
        ["3 serves http GET /bookings/{}"]
    );
    // Everything else in the app sits at the root, where the sites already say so.
    assert_eq!(out.linked("src/main.rs"), Vec::<String>::new());
}

#[test]
fn routers_are_followed_through_levels_bindings_and_forwarders() {
    let out = index("routers");
    assert_eq!(
        out.linked("src/bookings.rs"),
        [
            "5 serves http GET /api/bookings/{}",
            "5 serves http GET /api/rooms/{}"
        ]
    );
    assert_eq!(
        out.linked("src/main.rs"),
        ["16 serves http GET /api/reports"]
    );
    assert_eq!(
        out.linked("src/admin.rs"),
        ["7 serves http GET /admin/v1/users"]
    );
    assert_json_snapshot!(out.json()["linked"]);
}

#[test]
fn routers_that_mount_each_other_are_visited_once_each() {
    let out = index("routers");
    assert_eq!(
        out.linked("src/rings.rs"),
        [
            "7 serves http GET /rings/ping",
            "11 serves http GET /rings/pong/pong"
        ]
    );
}

#[test]
fn a_prefix_that_is_not_text_is_a_blind_spot_and_places_nothing() {
    let out = index("routers");
    // `under(prefix)` nests the bookings router at a path nobody can read, so the only sites are the two that the
    // other mounts place.
    assert_json_snapshot!(out.sites("src/main.rs"));
    assert_eq!(out.linked("src/bookings.rs").len(), 2);
}

#[scenario("rust", "A nested router serves only under its prefix")]
#[test]
fn a_nested_router_serves_only_under_its_prefix() {
    let out = index("mounts");
    assert_eq!(out.graph("src/nested.rs"), ["7 serves http GET /api/x"]);
}

#[scenario("rust", "A merged router serves at the root")]
#[test]
fn a_merged_router_serves_at_the_root() {
    let out = index("mounts");
    assert_eq!(out.graph("src/merged.rs"), ["7 serves http GET /x"]);
}

#[scenario("rust", "A router both merged and nested serves under both")]
#[test]
fn a_router_both_merged_and_nested_serves_under_both() {
    let out = index("mounts");
    assert_eq!(
        out.graph("src/both.rs"),
        ["7 serves http GET /api/x", "7 serves http GET /x"]
    );
    // The root mount is reported too: it replaces the bare site with itself, or the nested mount would drop it.
    assert_json_snapshot!(out.json()["linked"]);
}
