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
