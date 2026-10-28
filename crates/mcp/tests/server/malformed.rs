//! Requests that are not what the protocol gives a call, which the typed client cannot make and so are written to the
//! wire as lines of JSON.

use gsttt_scenario::scenario;
use rmcp::model::{ErrorCode, ErrorData};
use serde_json::{Value, json};

use crate::fake::Fake;
use crate::session::Wire;

#[scenario("mcp", "Arguments that are not an object are refused")]
#[tokio::test]
async fn arguments_that_are_not_an_object_are_the_protocol_error_for_invalid_params() {
    let source = Fake::small();
    let mut wire = Wire::start(source.clone()).await;

    for arguments in [json!([]), json!("x"), json!(3), json!(true)] {
        let response = wire.call("coverage", &arguments).await;

        assert!(response.get("result").is_none(), "{arguments}: {response}");
        let error: ErrorData =
            serde_json::from_value(response["error"].clone()).unwrap_or_else(|error| {
                panic!("{arguments}: no JSON-RPC error in {response}: {error}")
            });
        assert_eq!(
            error.code,
            ErrorCode::INVALID_PARAMS,
            "{arguments}: {response}"
        );
        assert!(
            error.message.contains("arguments"),
            "{arguments}: {response}"
        );
    }
    assert_eq!(
        source.calls(),
        0,
        "a call that cannot be read is not put to the source"
    );
    // The server is still serving: the call after the refused ones is answered as usual.
    let response = wire.call("coverage", &json!({})).await;
    assert_ne!(
        response["result"]["isError"],
        Value::Bool(true),
        "{response}"
    );
    assert!(
        response["result"]["content"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("coverage: ts read 2 of 2 files")),
        "{response}"
    );
    assert_eq!(source.calls(), 1);
}

#[tokio::test]
async fn a_method_the_server_does_not_have_is_still_method_not_found() {
    let mut wire = Wire::start(Fake::small()).await;

    let response = wire.exchange("sources/list", &json!({})).await;

    let error: ErrorData = serde_json::from_value(response["error"].clone())
        .unwrap_or_else(|error| panic!("no JSON-RPC error in {response}: {error}"));
    assert_eq!(error.code, ErrorCode::METHOD_NOT_FOUND, "{response}");
    assert_eq!(error.message, "sources/list");
}
