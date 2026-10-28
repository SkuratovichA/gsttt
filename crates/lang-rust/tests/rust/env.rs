//! Environment variables, on the `env` fixture.

use gsttt_scenario::scenario;
use insta::assert_json_snapshot;

use crate::support::index;

#[scenario("rust", "Every way of reading the environment is seen")]
#[test]
fn every_way_of_reading_the_environment_is_seen() {
    let out = index("env");
    assert_eq!(
        out.contracts("src/main.rs"),
        [
            "6 calls env E",
            "11 calls env A",
            "12 calls env B",
            "13 calls env C",
            "14 calls env D",
        ]
    );
    assert_json_snapshot!(out.sites("src/main.rs"));
}
