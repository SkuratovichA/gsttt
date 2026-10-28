//! `gsttt schema external`: the JSON Schema of the messages an external connector and gsttt exchange.

use gsttt_scenario::scenario;
use serde_json::Value;

use super::support::Sandbox;

#[scenario("cli", "schema external prints the protocol schema")]
#[test]
fn schema_external_prints_the_protocol_schema() {
    let sandbox = Sandbox::new();

    let ran = sandbox.run(&["schema", "external"]);

    assert_eq!(ran.code, Some(0), "{ran:?}");
    assert_eq!(ran.stderr, "", "{ran:?}");
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(printed, Value::from(gsttt_config::protocol_schema()));
    // A schema is read by people and checked into repositories: indented, and ending in a line break.
    assert!(ran.stdout.contains("\n  \""), "{}", ran.stdout);
    assert!(ran.stdout.ends_with("}\n"), "{}", ran.stdout);
}

/// The schema needs no repository and no config, so it is printed in the home directory too, which every command
/// about a repository refuses. It is JSON already, so `--json` changes nothing.
#[test]
fn schema_external_needs_no_root_and_prints_the_same_under_json() {
    let sandbox = Sandbox::new();
    let plain = sandbox.run(&["schema", "external"]);

    let at_home = sandbox.run_in(sandbox.home.path(), &["schema", "external"]);
    let json = sandbox.run(&["--json", "schema", "external"]);

    assert_eq!(plain.code, Some(0), "{plain:?}");
    assert_eq!(at_home.code, Some(0), "{at_home:?}");
    assert_eq!(at_home.stdout, plain.stdout);
    assert_eq!(json.code, Some(0), "{json:?}");
    assert_eq!(json.stdout, plain.stdout);
}
