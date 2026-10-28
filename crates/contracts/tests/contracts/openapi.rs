//! OpenAPI and Swagger documents, their references and their fragments.

use gsttt_contracts::OPENAPI;
use gsttt_core::connector::DynConnector;
use gsttt_core::fact::ParseStatus;
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use serde_json::{Value, json};

use crate::support::{Run, fixture, logged, module, with_options};

#[gsttt_scenario::scenario("contract-formats", "Operations are declared under the server path")]
#[test]
fn operations_are_declared_under_the_server_path() {
    let run = Run::new("openapi_servers", &[module(&OPENAPI)]);
    assert_eq!(run.status("openapi.yaml"), &ParseStatus::Ok);
    assert_eq!(
        run.sites("openapi.yaml"),
        vec![
            "declares http GET /v1/bookings/{} @9",
            "declares http DELETE /v1/bookings/{} @13",
        ]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario("contract-formats", "Operations are declared under every server path")]
#[test]
fn operations_are_declared_under_every_server_path() {
    let run = Run::new("openapi_many_servers", &[module(&OPENAPI)]);
    assert_eq!(run.status("api/openapi.yaml"), &ParseStatus::Ok);
    // The first two servers differ in their origin only, so they give one path.
    assert_eq!(
        run.sites("api/openapi.yaml"),
        vec![
            "declares http GET /v1/bookings @12",
            "declares http GET /v2/bookings @12",
            "declares http GET /v3/bookings @12",
        ]
    );
    assert_eq!(
        run.linked(),
        vec![
            "api/paths/carts.yaml: declares http GET /v1/carts @1",
            "api/paths/carts.yaml: declares http GET /v2/carts @1",
            "api/paths/carts.yaml: declares http GET /v3/carts @1",
        ]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[test]
fn swagger_two_documents_use_their_base_path() {
    let run = Run::new("openapi_swagger", &[module(&OPENAPI)]);
    assert_eq!(
        run.sites("swagger.json"),
        vec!["declares http GET /v2/bookings @6"]
    );
}

#[gsttt_scenario::scenario("contract-formats", "References are imports")]
#[test]
fn references_are_imports() {
    let run = Run::new("openapi_refs", &[module(&OPENAPI)]);
    let targets: Vec<String> = run
        .imports("api/openapi.yaml")
        .into_iter()
        .map(|(_, target)| target)
        .collect();
    assert_eq!(targets, vec!["api/schemas/booking.yaml"]);
    assert_eq!(run.project_of("api/schemas/booking.yaml"), "openapi");
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario("contract-formats", "Path items in fragments are declared")]
#[test]
fn path_items_in_fragments_are_declared() {
    let run = Run::new("openapi_fragments", &[module(&OPENAPI)]);
    assert_eq!(run.sites("api/openapi.yaml"), Vec::<String>::new());
    assert_eq!(
        run.linked(),
        vec![
            "api/paths/bookings.yaml: declares http GET /bookings @1",
            "api/paths/bookings.yaml: declares http POST /bookings @5",
        ]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[test]
fn the_files_option_claims_documents_with_other_names() {
    let without = Run::new("openapi_option", &[module(&OPENAPI)]);
    assert!(without.output.projects.is_empty());

    let options = json!({ "files": ["docs/*.yaml"] });
    let run = Run::new("openapi_option", &[with_options(&OPENAPI, options)]);
    assert_eq!(
        run.sites("docs/bookings-api.yaml"),
        vec!["declares http GET /bookings @7"]
    );
    assert_eq!(run.unclaimed(), vec!["config/app.yaml"]);
}

#[gsttt_scenario::scenario(
    "contract-formats",
    "A document that cannot be searched for references is logged"
)]
#[test]
fn a_document_that_cannot_be_searched_for_references_is_logged() {
    let repo = fixture("openapi_broken_fragment");
    let log = logged(|| {
        OPENAPI.open(&repo, &Value::Null, &[]).unwrap();
    });
    assert!(log.contains("paths/carts.yaml"), "{log}");
    assert!(!log.contains("paths/bookings.yaml"), "{log}");
    let run = Run::new("openapi_broken_fragment", &[module(&OPENAPI)]);
    assert_eq!(run.project_of("paths/bookings.yaml"), "openapi");
    assert_eq!(run.project_of("paths/carts.yaml"), "openapi");
}

/// The same for a document over the size the connector reads.
#[test]
fn a_document_over_the_size_the_connector_reads_is_logged() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("openapi_oversized");
    std::fs::create_dir_all(&dir).unwrap();
    // One byte over the 8 MiB the connector reads when it looks for the fragments a document refers to.
    std::fs::write(dir.join("openapi.yaml"), "#".repeat(8 * 1024 * 1024 + 1)).unwrap();
    let files = vec![RepoPath::normalize("openapi.yaml").unwrap()];
    let repo = RepoView::new(dir.to_str().unwrap(), files);
    let log = logged(|| {
        OPENAPI.open(&repo, &Value::Null, &[]).unwrap();
    });
    assert!(log.contains("openapi.yaml"), "{log}");
    assert!(log.contains("8388609"), "{log}");
}
