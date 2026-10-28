//! What the server says before it touches the repository.

use gsttt_scenario::scenario;

use crate::fake::Fake;
use crate::session::Session;

#[scenario("mcp", "initialize and tools/list do not touch the repository")]
#[tokio::test]
async fn initialize_and_tools_list_do_not_call_the_source() {
    let source = Fake::small().failing("the repository cannot be indexed here");
    let session = Session::start(source.clone()).await;

    let tools = session.tools().await;

    assert_eq!(tools.len(), 7);
    assert_eq!(source.calls(), 0);
}

#[scenario("mcp", "initialize carries instructions")]
#[tokio::test]
async fn initialize_carries_instructions() {
    let session = Session::start(Fake::small()).await;

    let info = session.server_info();
    let instructions = info
        .instructions
        .as_deref()
        .expect("the server gives instructions");

    assert!(instructions.contains("Call impact before changing a file that others may use"));
    assert!(instructions.contains("with changed=true before finishing a task"));
    assert!(
        instructions
            .contains("when coverage names a gap, say so instead of assuming nothing is affected")
    );
    // The words are part of the contract with the model, so a change to them is read before it is accepted.
    insta::assert_snapshot!(instructions);
}

#[tokio::test]
async fn the_server_names_itself_and_offers_tools() {
    let session = Session::start(Fake::small()).await;

    let info = session.server_info();

    let identity = info.server_info.as_ref().expect("the server names itself");
    assert_eq!(identity.name, "gsttt");
    assert_eq!(identity.version, gsttt_core::VERSION);
    assert!(info.capabilities.tools.is_some());
}
