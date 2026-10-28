//! `index_status`: how the stored index stands, without a refresh.

use gsttt_core::Freshness;
use gsttt_core::query::{self, Answer, Entries};
use gsttt_scenario::scenario;
use serde_json::json;

use crate::fake::Fake;
use crate::session::Session;

#[scenario("mcp", "index_status does not refresh")]
#[tokio::test]
async fn index_status_reads_the_stored_index_and_does_not_refresh() {
    let source = Fake::small();
    let session = Session::start(source.clone()).await;

    let reply = session.call("index_status", json!({})).await;

    assert!(!reply.is_error, "{}", reply.text);
    assert_eq!(source.stored_reads(), 1);
    assert_eq!(source.refreshes(), 0);
    let stored = source.stored_index().expect("the fake holds an index");
    assert_eq!(
        reply.text,
        query::status(&stored.index, stored.freshness).text(Entries::Capped)
    );
    assert!(
        reply.text.starts_with("built by gsttt 0.0.0 at a1b2c3d"),
        "{}",
        reply.text
    );
}

#[tokio::test]
async fn index_status_says_how_far_the_tree_has_moved_on() {
    let source = Fake::small().stored_stale(1, 2, 3);
    let session = Session::start(source.clone()).await;

    let reply = session.call_bare("index_status").await;

    let stale = Freshness::Stale {
        added: 1,
        removed: 2,
        changed: 3,
    };
    assert_eq!(
        reply.text,
        query::status(&source.index(), stale).text(Entries::Capped)
    );
    let fresh = query::status(&source.index(), Freshness::Fresh).text(Entries::Capped);
    assert_ne!(reply.text, fresh);
    assert_eq!(source.refreshes(), 0);
}

#[scenario("mcp", "index_status without an index answers in text")]
#[tokio::test]
async fn index_status_without_an_index_says_the_first_question_builds_one() {
    let source = Fake::small().without_stored_index();
    let session = Session::start(source.clone()).await;

    let reply = session.call("index_status", json!({})).await;

    assert!(!reply.is_error, "{}", reply.text);
    assert!(
        reply.text.contains("There is no index yet"),
        "{}",
        reply.text
    );
    assert!(
        reply.text.contains("the first question builds one"),
        "{}",
        reply.text
    );
    assert_eq!(
        source.refreshes(),
        0,
        "asking after the index does not build it"
    );
}
