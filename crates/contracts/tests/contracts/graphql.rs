//! GraphQL schemas, operations, import comments and documents embedded in code.

use gsttt_contracts::GRAPHQL;
use gsttt_core::fact::ParseStatus;

use crate::support::{HOST, Run, module};

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

#[gsttt_scenario::scenario("contract-formats", "Embedded documents keep host lines")]
#[test]
fn embedded_documents_keep_host_lines() {
    let run = Run::new("graphql_embedded", &[module(&HOST), module(&GRAPHQL)]);
    assert_eq!(
        run.embedded("src/client.ts"),
        vec![(
            Some("graphql"),
            vec!["calls graphql Query.bookings @5".to_owned()]
        )]
    );
    insta::assert_json_snapshot!(run.snapshot());
}
