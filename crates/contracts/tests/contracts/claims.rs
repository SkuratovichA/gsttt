//! Which files each connector claims, and what a broken document does to the run.

use gsttt_core::fact::{ParseFailure, ParseStatus};
use serde_json::Value;

use crate::support::{Run, all_modules, connectors, fixture};

#[gsttt_scenario::scenario(
    "contract-formats",
    "A connector opens a project only for files it claims"
)]
#[test]
fn a_connector_opens_a_project_only_for_files_it_claims() {
    let repo = fixture("claims_graphql_env");
    let opened: Vec<(&str, Vec<String>)> = connectors()
        .iter()
        .map(|connector| {
            let projects = connector.open(&repo, &Value::Null, &[]).unwrap();
            let roots = projects
                .iter()
                .map(|project| project.root().to_owned())
                .collect();
            (connector.meta().name, roots)
        })
        .collect();
    let one_at_the_root = vec![String::new()];
    assert_eq!(
        opened,
        vec![
            ("openapi", vec![]),
            ("graphql", one_at_the_root.clone()),
            ("proto", vec![]),
            ("prisma", vec![]),
            ("sql", vec![]),
            ("env", one_at_the_root),
        ]
    );
}

#[gsttt_scenario::scenario("contract-formats", "Each format claims its files")]
#[test]
fn each_format_claims_its_files() {
    let run = Run::new("all_formats", &all_modules());
    let owners: Vec<(&str, &str)> = [
        "api/openapi.yaml",
        "api/paths/bookings.yaml",
        "schema.graphql",
        "proto/booking/v1/booking.proto",
        "prisma/schema.prisma",
        "migrations/001_init.sql",
        ".env.example",
    ]
    .into_iter()
    .map(|path| (path, run.project_of(path)))
    .collect();
    assert_eq!(
        owners,
        vec![
            ("api/openapi.yaml", "openapi"),
            ("api/paths/bookings.yaml", "openapi"),
            ("schema.graphql", "graphql"),
            ("proto/booking/v1/booking.proto", "proto"),
            ("prisma/schema.prisma", "prisma"),
            ("migrations/001_init.sql", "sql"),
            (".env.example", "env"),
        ]
    );
    assert!(
        run.output
            .projects
            .iter()
            .all(|project| project.root.is_empty())
    );
    assert_eq!(run.unclaimed(), vec!["config/app.yaml"]);
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario("contract-formats", "A broken document fails alone")]
#[test]
fn a_broken_document_fails_alone() {
    let run = Run::new("broken_yaml", &all_modules());
    let status = run.status("api/openapi.yaml");
    assert!(
        matches!(
            status,
            ParseStatus::Failed(ParseFailure::Syntax { line: 12, .. })
        ),
        "{status:?}"
    );
    assert_eq!(run.status("schema.graphql"), &ParseStatus::Ok);
    assert_eq!(
        run.sites("schema.graphql"),
        vec!["declares graphql Query.bookings @2"]
    );
    assert_eq!(
        run.sites(".env.example"),
        vec!["declares env DATABASE_URL @1"]
    );
    insta::assert_json_snapshot!(run.snapshot());
}
