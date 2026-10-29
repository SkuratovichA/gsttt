//! Arguments that do not fit a tool come back as results the model can read and act on, never as protocol errors.

use gsttt_scenario::scenario;
use rmcp::model::ErrorCode;
use rmcp::service::ServiceError;
use serde_json::json;

use crate::fake::Fake;
use crate::session::Session;

#[scenario("mcp", "A malformed argument is a tool error")]
#[tokio::test]
async fn a_malformed_argument_is_a_tool_error_that_names_it() {
    let source = Fake::small();
    let session = Session::start(source.clone()).await;

    // `Session::call` fails the test when the server answers with a JSON-RPC error.
    let reply = session.call("impact", json!({ "paths": 3 })).await;

    assert!(reply.is_error);
    assert_eq!(
        reply.text,
        "impact: `paths` must be a list of paths, for example [\"src/a.ts\"]"
    );
    assert_eq!(
        source.calls(),
        0,
        "the source is not asked about a call that cannot be read"
    );
}

#[scenario("mcp", "Conflicting arguments are a tool error")]
#[tokio::test]
async fn paths_together_with_changed_are_a_tool_error() {
    let source = Fake::small();
    let session = Session::start(source.clone()).await;

    let reply = session
        .call("impact", json!({ "paths": ["src/a.ts"], "changed": true }))
        .await;

    assert!(reply.is_error);
    assert!(
        reply.text.contains("pass either `paths` or `changed`"),
        "{}",
        reply.text
    );
    assert_eq!(source.calls(), 0);
}

#[scenario("mcp", "names with changed is a tool error")]
#[tokio::test]
async fn names_with_changed_are_a_tool_error_that_asks_for_paths() {
    let source = Fake::small();
    let session = Session::start(source.clone()).await;

    let reply = session
        .call("impact", json!({ "changed": true, "names": ["foo"] }))
        .await;

    assert!(reply.is_error);
    assert!(
        reply.text.starts_with("impact: `names` needs `paths`"),
        "{}",
        reply.text
    );
    assert_eq!(source.calls(), 0);
}

#[tokio::test]
async fn names_without_paths_say_the_same() {
    let session = Session::start(Fake::small()).await;

    let with_changed = session
        .call("impact", json!({ "changed": true, "names": ["foo"] }))
        .await;
    let alone = session.call("impact", json!({ "names": ["foo"] })).await;

    assert!(alone.is_error);
    assert_eq!(alone.text, with_changed.text);
}

#[tokio::test]
async fn impact_without_paths_or_changed_says_what_to_pass() {
    let session = Session::start(Fake::small()).await;

    let nothing = session.call("impact", json!({})).await;
    let empty = session.call("impact", json!({ "paths": [] })).await;

    assert!(nothing.is_error);
    assert!(
        nothing.text.starts_with(
            "impact: name the files to ask about in `paths`, or set `changed` to true"
        )
    );
    assert_eq!(nothing.text, empty.text);
}

#[tokio::test]
async fn base_without_changed_is_a_tool_error() {
    let session = Session::start(Fake::small()).await;

    let reply = session
        .call("impact", json!({ "paths": ["src/a.ts"], "base": "main" }))
        .await;

    assert!(reply.is_error);
    assert!(
        reply.text.starts_with("impact: `base` goes with `changed`"),
        "{}",
        reply.text
    );
}

#[tokio::test]
async fn an_argument_the_tool_does_not_take_is_named_with_the_ones_it_takes() {
    let session = Session::start(Fake::small()).await;

    let extra = session
        .call("dependents", json!({ "path": "lib/util.ts", "depth": 2 }))
        .await;
    let none_taken = session.call("index_status", json!({ "all": true })).await;

    assert!(extra.is_error);
    assert_eq!(
        extra.text,
        "dependents: `depth` is not an argument of this tool; it takes `path` and `all`"
    );
    assert!(none_taken.is_error);
    assert_eq!(
        none_taken.text,
        "index_status: `all` is not an argument of this tool; it takes no arguments"
    );
}

#[tokio::test]
async fn a_missing_required_argument_is_named_with_what_it_must_be() {
    let session = Session::start(Fake::small()).await;

    let with_empty_object = session.call("dependents", json!({})).await;
    let without_arguments = session.call_bare("dependents").await;
    let one_of_two = session.call("path", json!({ "from": "src/a.ts" })).await;

    assert!(with_empty_object.is_error);
    assert_eq!(
        with_empty_object.text,
        "dependents: `path` is required and must be a path from the repository root, for example \"src/a.ts\""
    );
    assert_eq!(without_arguments.text, with_empty_object.text);
    assert!(one_of_two.is_error);
    assert!(
        one_of_two.text.starts_with("path: `to` is required"),
        "{}",
        one_of_two.text
    );
}

#[tokio::test]
async fn a_value_of_the_wrong_type_names_its_argument_whichever_the_tool() {
    let session = Session::start(Fake::small()).await;

    let cases = [
        (
            "impact",
            json!({ "paths": ["src/a.ts", 3] }),
            "`paths` must be a list of paths",
        ),
        (
            "impact",
            json!({ "changed": "yes" }),
            "`changed` must be true or false",
        ),
        (
            "impact",
            json!({ "changed": true, "depth": -1 }),
            "`depth` must be a whole number",
        ),
        (
            "impact",
            json!({ "changed": true, "depth": 1.5 }),
            "`depth` must be a whole number",
        ),
        (
            "impact",
            json!({ "changed": true, "base": 7 }),
            "`base` must be a git revision",
        ),
        (
            "impact",
            json!({ "paths": ["src/a.ts"], "names": "getHash" }),
            "`names` must be a list of names",
        ),
        (
            "dependents",
            json!({ "path": ["lib/util.ts"] }),
            "`path` must be a path from the repository root",
        ),
        (
            "dependencies",
            json!({ "path": "lib/util.ts", "all": 1 }),
            "`all` must be true or false",
        ),
        (
            "path",
            json!({ "from": "src/a.ts", "to": null }),
            "`to` must be a path from the repository root",
        ),
        ("contract", json!({ "query": 5 }), "`query` must be text"),
        (
            "coverage",
            json!({ "all": "all" }),
            "`all` must be true or false",
        ),
    ];
    for (tool, arguments, expected) in cases {
        let reply = session.call(tool, arguments.clone()).await;

        assert!(reply.is_error, "{tool} {arguments}");
        assert!(
            reply.text.starts_with(&format!("{tool}: ")) && reply.text.contains(expected),
            "{tool} {arguments}: {}",
            reply.text
        );
    }
}

#[tokio::test]
async fn an_unknown_tool_is_the_protocol_error_for_invalid_params() {
    let session = Session::start(Fake::small()).await;

    let error = session
        .try_call("sources", json!({}))
        .await
        .expect_err("a tool the server does not offer is refused");

    let ServiceError::McpError(data) = error else {
        panic!("expected a JSON-RPC error, got {error:?}");
    };
    assert_eq!(data.code, ErrorCode::INVALID_PARAMS);
    assert_eq!(
        data.message,
        "unknown tool `sources`; the tools are impact, dependents, dependencies, path, contract, coverage and index_status"
    );
}
