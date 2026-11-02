//! Environment variables, on the `env` fixture.

use gsttt_config::read_pack;
use gsttt_scenario::scenario;
use insta::assert_json_snapshot;

use crate::support::{index, index_with};

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

#[scenario("rust", "Setting or removing a variable serves it")]
#[test]
fn setting_or_removing_a_variable_serves_it() {
    let out = index("env-writes");
    assert_eq!(
        out.contracts("src/main.rs"),
        [
            "4 serves env A",
            "5 serves env B",
            "6 serves env C",
            "7 serves env D",
            "9 serves env E",
        ]
    );
    assert_json_snapshot!(out.sites("src/main.rs"));
}

#[scenario("rust", "A chain rule with a written role serves what is assigned")]
#[test]
fn a_chain_rule_with_a_written_role_serves_what_is_assigned() {
    let pack = read_pack(
        "settings.toml",
        r#"name = "settings"
language = "rust"

[[chains]]
root = "settings"
kind = "env"
role = "calls"
written = "serves"
segments = 1
"#,
    )
    .unwrap();
    let out = index_with("chain-writes", vec![pack]);
    assert_eq!(
        out.contracts("src/main.rs"),
        [
            "2 serves env PORT",
            "3 serves env RETRIES",
            "4 calls env PORT",
        ]
    );
    assert_json_snapshot!(out.sites("src/main.rs"));
}
