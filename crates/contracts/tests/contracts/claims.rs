//! Which files each connector claims, and what a broken document does to the run.

use gsttt_contracts::{ENV, GRAPHQL, OPENAPI, PRISMA, PROTO, SQL};
use gsttt_core::connector::DynConnector;
use gsttt_core::fact::{ParseFailure, ParseStatus};
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use serde_json::{Value, json};

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
            ("sql", one_at_the_root.clone()),
            ("env", one_at_the_root),
        ]
    );
}

/// What the engine asks before it runs a connector nobody configured: whether the file list holds anything for it. The list alone answers, so one built in memory is enough.
#[test]
fn a_connector_is_detected_only_when_the_file_list_holds_a_file_it_claims() {
    let listing = |paths: &[&str]| {
        let files = paths
            .iter()
            .map(|path| RepoPath::try_from(*path).unwrap())
            .collect();
        RepoView::new(".", files)
    };
    let cases: [(&'static dyn DynConnector, &str, &str); 4] = [
        (&OPENAPI, "api/openapi.yaml", "config/app.yaml"),
        (&PROTO, "proto/booking.proto", "proto/booking.txt"),
        (&PRISMA, "prisma/schema.prisma", "prisma/schema.txt"),
        (&ENV, ".env.example", ".env.d.ts"),
    ];
    let mut wrong = Vec::new();
    for (connector, claimed, other) in cases {
        for (paths, expected) in [
            (vec![claimed, other], true),
            (vec![other], false),
            (vec![], false),
        ] {
            if connector.detect(&listing(&paths)) != expected {
                wrong.push(format!(
                    "{} with {paths:?}: expected {expected}",
                    connector.meta().name
                ));
            }
        }
    }
    // Code in any language can embed a GraphQL or an SQL document, so these two have something to read in any list.
    for connector in [&GRAPHQL as &dyn DynConnector, &SQL] {
        if !connector.detect(&listing(&[])) {
            wrong.push(format!(
                "{} with no file: expected true",
                connector.meta().name
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
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

/// The options a project is opened with are already part of the key of its facts, so no project adds a salt for them: a second hash of the same options would only be one more thing to keep in step.
#[test]
fn no_project_adds_a_salt_to_the_fact_key() {
    let repo = fixture("all_formats");
    let configured: [(&'static dyn DynConnector, Value); 2] = [
        (&OPENAPI, json!({ "files": ["config/*.yaml"] })),
        (&SQL, json!({ "dialect": "mysql" })),
    ];
    let salts: Vec<u64> = connectors()
        .map(|connector| (connector, Value::Null))
        .into_iter()
        .chain(configured)
        .flat_map(|(connector, options)| connector.open(&repo, &options, &[]).unwrap())
        .map(|project| project.salt())
        .collect();
    assert!(salts.len() >= 8, "{salts:?}");
    assert!(salts.iter().all(|salt| *salt == 0), "{salts:?}");
}
