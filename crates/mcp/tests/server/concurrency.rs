//! Calls into the source: one at a time, on a blocking thread, and no reason for a call to take the server down.

use gsttt_core::{Freshness, query};
use gsttt_scenario::scenario;
use serde_json::json;

use crate::fake::Fake;
use crate::session::{Session, answered};

#[scenario("mcp", "Concurrent calls refresh one at a time")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_calls_refresh_one_at_a_time() {
    let source = Fake::small().slow_refresh();
    let session = Session::start(source.clone()).await;
    let alone = query::dependents(&source.index(), Freshness::Fresh, "lib/util.ts").to_string();

    let replies = session
        .call_together("dependents", &json!({ "path": "lib/util.ts" }), 8)
        .await;

    assert_eq!(replies.len(), 8, "all eight answer");
    for reply in &replies {
        assert!(!reply.is_error, "{}", reply.text);
        assert_eq!(
            reply.text, alone,
            "an answer does not depend on the calls beside it"
        );
    }
    assert_eq!(source.refreshes(), 8, "each call refreshed");
    assert_eq!(
        source.peak(),
        1,
        "the source never had two calls in progress"
    );
}

/// The runtime has a single thread, so a refresh that ran on it would stop the test and the server together.
#[tokio::test]
async fn a_refresh_in_progress_does_not_stop_the_server_answering() {
    let (source, mut hold) = Fake::small().holding_first_refresh();
    let session = Session::start(source.clone()).await;

    let refreshing = session.send("coverage", &json!({}));
    hold.entered().await;

    assert_eq!(
        session.tools().await.len(),
        7,
        "tools/list is answered at once"
    );
    let wrong = session.call("dependents", json!({})).await;
    assert!(
        wrong.is_error,
        "a call that is wrong from the start is told so at once"
    );
    assert!(wrong.text.contains("`path`"), "{}", wrong.text);
    assert_eq!(
        source.calls(),
        1,
        "only the refresh in progress has reached the source"
    );

    hold.release();
    let reply = answered(refreshing).await;
    assert!(!reply.is_error, "{}", reply.text);
}

#[tokio::test]
async fn a_source_that_panics_fails_that_call_and_not_the_server() {
    let session = Session::start(Fake::small().breaking_once()).await;

    let broken = session.call("coverage", json!({})).await;
    let next = session.call("coverage", json!({})).await;

    assert!(broken.is_error, "{}", broken.text);
    assert!(
        broken.text.starts_with("coverage: "),
        "the message names the tool: {}",
        broken.text
    );
    assert!(!next.is_error, "the next call is answered: {}", next.text);
}
