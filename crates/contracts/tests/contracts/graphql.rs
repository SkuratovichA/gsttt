//! GraphQL schemas, operations, import comments and documents embedded in code.

use gsttt_contracts::GRAPHQL;
use gsttt_core::fact::ParseStatus;

use crate::support::{HOST, Run, module, read_embedded};

#[gsttt_scenario::scenario("contract-formats", "Schema fields are declared")]
#[test]
fn schema_fields_are_declared() {
    let run = Run::new("graphql_schema", &[module(&GRAPHQL)]);
    assert_eq!(run.status("schema.graphql"), &ParseStatus::Ok);
    assert_eq!(
        run.sites("schema.graphql"),
        vec![
            "declares graphql Query.bookings @6",
            "declares graphql Mutation.cancel @14",
        ]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario("contract-formats", "Operations call their root fields")]
#[test]
fn operations_call_their_root_fields() {
    let run = Run::new("graphql_operations", &[module(&GRAPHQL)]);
    assert_eq!(run.status("operations.graphql"), &ParseStatus::Ok);
    assert_eq!(
        run.sites("operations.graphql"),
        vec![
            "calls graphql Query.bookings @2",
            "calls graphql Query.me @5",
            "calls graphql Mutation.cancel @11",
            // The shorthand `{ me }` is a query.
            "calls graphql Query.me @16",
            // `query WithFragment { ...F }` reaches the field of the fragment in the same document.
            "calls graphql Query.me @20",
        ]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario("contract-formats", "Import comments are imports relative to the file")]
#[test]
fn import_comments_are_imports_relative_to_the_file() {
    let run = Run::new("graphql_imports", &[module(&GRAPHQL)]);
    assert_eq!(
        run.imports("api/schema.graphql"),
        vec![(
            "./types/booking.graphql".to_owned(),
            "api/types/booking.graphql".to_owned()
        )]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario(
    "contract-formats",
    "Embedded documents report lines within the document"
)]
#[test]
fn embedded_documents_report_lines_within_the_document() {
    // The document starts on line 5 of its host file; the engine, not the connector, moves its lines there.
    assert_eq!(
        read_embedded(
            &GRAPHQL,
            "graphql_embedded",
            "graphql",
            5,
            "query { bookings { id } }"
        ),
        vec!["calls graphql Query.bookings @1"]
    );
    assert_eq!(
        read_embedded(
            &GRAPHQL,
            "graphql_embedded",
            "graphql",
            5,
            "query Bookings {\n  me { id }\n  bookings { id }\n}"
        ),
        vec![
            "calls graphql Query.me @2",
            "calls graphql Query.bookings @3"
        ]
    );
}

/// The pipeline hands the host's document to the connector and keeps what it reads. The snapshot shows the lines as the engine places them, so it changes when the engine moves them to the host file.
#[test]
fn embedded_documents_are_read_when_a_host_carries_them() {
    let run = Run::new("graphql_embedded", &[module(&HOST), module(&GRAPHQL)]);
    assert_eq!(
        run.embedded("src/client.ts"),
        vec![(
            Some("graphql"),
            vec!["calls graphql Query.bookings".to_owned()]
        )]
    );
    insta::assert_json_snapshot!(run.snapshot());
}
