//! How a session ends.

use gsttt_scenario::scenario;

use crate::fake::Fake;
use crate::session::{PATIENCE, Session};

#[scenario("mcp", "The server stops when its input closes")]
#[tokio::test]
async fn the_server_stops_when_its_input_closes() {
    let session = Session::start(Fake::small()).await;

    let outcome = session.close().await;

    assert!(
        outcome.is_ok(),
        "serve returns without an error: {outcome:?}"
    );
}

#[tokio::test]
async fn a_client_that_hangs_up_before_it_says_hello_is_not_an_error() {
    let (client_end, server_end) = tokio::io::duplex(1 << 10);
    let (input, output) = tokio::io::split(server_end);
    drop(client_end);

    let outcome = tokio::time::timeout(PATIENCE, gsttt_mcp::serve_on(Fake::small(), input, output))
        .await
        .expect("serve returns once its input has closed");

    assert!(
        outcome.is_ok(),
        "nothing was asked, so nothing failed: {outcome:?}"
    );
}
