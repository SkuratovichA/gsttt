//! Rules that apply to a method only on a receiver the file shows to be the right kind, on the `gates` fixture.

use gsttt_scenario::scenario;

use crate::support::index;

#[scenario("rust", "A route on another receiver is no route")]
#[test]
fn a_route_on_another_receiver_is_no_route() {
    let found = index("gates").contracts("src/lib.rs");
    assert!(
        found
            .iter()
            .any(|site| site.ends_with("serves http GET /health")),
        "{found:?}"
    );
    assert!(
        !found.iter().any(|site| site.contains("not-a-route")),
        "{found:?}"
    );
}

#[scenario("rust", "A client builder makes a client")]
#[test]
fn a_client_builder_makes_a_client() {
    let found = index("gates").contracts("src/lib.rs");
    assert!(
        found
            .iter()
            .any(|site| site.contains("calls http GET /items")),
        "{found:?}"
    );
}

#[scenario("rust", "A function a use brought in is matched by its full path")]
#[test]
fn a_function_a_use_brought_in_is_matched_by_its_full_path() {
    let found = index("gates").contracts("src/imported.rs");
    assert!(
        found
            .iter()
            .any(|site| site.ends_with("calls http GET /imported")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|site| site.ends_with("calls env IMPORTED")),
        "{found:?}"
    );
}
