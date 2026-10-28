//! SQL and tables, on the `data` fixture.

use gsttt_scenario::scenario;
use insta::assert_json_snapshot;
use serde_json::json;

use crate::support::index;

#[scenario("rust", "Query macros embed SQL")]
#[test]
fn query_macros_embed_sql() {
    let out = index("data");
    let embedded = out.sites("crates/db/src/queries.rs")["embedded"].clone();
    assert!(
        embedded
            .as_array()
            .unwrap()
            .iter()
            .any(|document| document["line"] == 9
                && document["lang"] == "sql"
                && document["text"] == "SELECT id FROM bookings WHERE id = $1"),
        "{embedded}"
    );
    assert_json_snapshot!(out.sites("crates/db/src/queries.rs"));
}

#[scenario("rust", "Query files are includes")]
#[test]
fn query_files_are_includes() {
    let out = index("data");
    // The macro names the file from the manifest's directory, which is how the import is spelled.
    assert_eq!(
        out.to(
            "crates/db/src/queries.rs",
            "${CARGO_MANIFEST_DIR}/queries/find.sql"
        ),
        json!({ "file": "crates/db/queries/find.sql" })
    );
}

#[scenario("rust", "An entity declares its table")]
#[test]
fn an_entity_declares_its_table() {
    let out = index("data");
    assert_eq!(
        out.contracts("crates/db/src/entity.rs"),
        ["4 declares table bookings"]
    );
    assert_json_snapshot!(out.sites("crates/db/src/entity.rs"));
}
