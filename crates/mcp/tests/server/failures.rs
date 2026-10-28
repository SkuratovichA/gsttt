//! What a call that cannot be answered comes back as: a result with `isError` set, never a protocol error.

use gsttt_scenario::scenario;
use serde_json::json;

use crate::fake::Fake;
use crate::session::Session;

#[scenario("mcp", "A path outside the root is a tool error")]
#[tokio::test]
async fn a_path_outside_the_root_is_a_tool_error() {
    let source = Fake::small();
    let session = Session::start(source.clone()).await;

    let reply = session
        .call("dependents", json!({ "path": "../elsewhere.ts" }))
        .await;

    assert!(reply.is_error);
    assert!(
        reply.text.contains("outside the repository"),
        "{}",
        reply.text
    );
    assert!(
        reply.text.contains("../elsewhere.ts"),
        "the text names the path: {}",
        reply.text
    );
    assert_eq!(
        source.refreshes(),
        0,
        "a path that cannot be read does not build an index"
    );
}

#[tokio::test]
async fn every_tool_that_takes_a_path_refuses_one_outside_the_root() {
    let source = Fake::small();
    let session = Session::start(source.clone()).await;
    let cases = [
        (
            "impact",
            json!({ "paths": ["src/a.ts", "../elsewhere.ts"] }),
        ),
        ("dependencies", json!({ "path": "../elsewhere.ts" })),
        (
            "path",
            json!({ "from": "src/a.ts", "to": "../elsewhere.ts" }),
        ),
        (
            "path",
            json!({ "from": "../elsewhere.ts", "to": "src/a.ts" }),
        ),
    ];

    for (tool, arguments) in cases {
        let reply = session.call(tool, arguments.clone()).await;

        assert!(reply.is_error, "{tool} {arguments}");
        assert!(
            reply.text.contains("outside the repository"),
            "{tool}: {}",
            reply.text
        );
    }
    assert_eq!(source.refreshes(), 0);
}

#[scenario("mcp", "A failed refresh is a tool error")]
#[tokio::test]
async fn a_failed_refresh_is_a_tool_error_that_holds_the_message() {
    let message = "the index could not be built: `.gsttt/config.toml` line 3 does not parse; fix it and ask again";
    let source = Fake::small().failing(message);
    let session = Session::start(source.clone()).await;

    let reply = session.call("coverage", json!({})).await;

    assert!(reply.is_error);
    assert_eq!(reply.text, message);
    assert_eq!(
        source.refreshes(),
        0,
        "the fake counts a refresh only when it succeeds"
    );
}

#[tokio::test]
async fn a_source_that_cannot_list_the_changed_files_is_a_tool_error() {
    let message = "`--changed` needs a git repository, and this directory is not in one";
    let source = Fake::small().failing(message);
    let session = Session::start(source.clone()).await;

    let reply = session.call("impact", json!({ "changed": true })).await;

    assert!(reply.is_error);
    assert_eq!(reply.text, message);
    assert_eq!(
        source.calls(),
        1,
        "the refresh is not tried once the files cannot be listed"
    );
}

#[tokio::test]
async fn a_source_that_cannot_read_the_stored_index_is_a_tool_error() {
    let message = "the stored index could not be read: it was written by a newer gsttt";
    let session = Session::start(Fake::small().failing(message)).await;

    let reply = session.call("index_status", json!({})).await;

    assert!(reply.is_error);
    assert_eq!(reply.text, message);
}

#[tokio::test]
async fn a_blank_contract_query_is_a_tool_error_that_says_what_to_ask() {
    let session = Session::start(Fake::small()).await;

    let reply = session.call("contract", json!({ "query": "  " })).await;

    assert!(reply.is_error);
    assert_eq!(
        reply.text,
        "contract: no contract was asked for: expected a key like `http GET /bookings/{}`, or part of one"
    );
}
