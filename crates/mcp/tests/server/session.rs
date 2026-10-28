//! A server and a real MCP client, joined by an in-memory pipe, so that the tests speak the protocol the way an agent
//! does and see what an agent would see.

use std::sync::Arc;
use std::time::Duration;

use gsttt_mcp::{IndexSource, ServeError, serve_on};
use rmcp::model::{CallToolRequestParams, CallToolResult, ServerPeerInfo, Tool, object};
use rmcp::service::{RunningService, ServiceError};
use rmcp::{RoleClient, ServiceExt};
use serde_json::Value;
use tokio::task::JoinHandle;

/// How long a test waits for the server to do something it should do at once, before it calls the server stuck.
pub(crate) const PATIENCE: Duration = Duration::from_secs(20);

/// What a tool call came back with when it came back as a result and not as a protocol error.
#[derive(Debug)]
pub(crate) struct Reply {
    /// Whether the result has `isError` set.
    pub(crate) is_error: bool,
    /// The text of the one block the result holds.
    pub(crate) text: String,
}

impl Reply {
    fn of(result: &CallToolResult) -> Self {
        let [block] = result.content.as_slice() else {
            panic!("a result holds one content block, this one holds {result:?}");
        };
        let text = block
            .as_text()
            .unwrap_or_else(|| panic!("the block is text, this one is {block:?}"));
        Self {
            is_error: result.is_error == Some(true),
            text: text.text.clone(),
        }
    }
}

/// The answer to a call that was sent, once it has come.
pub(crate) async fn answered(call: JoinHandle<Reply>) -> Reply {
    tokio::time::timeout(PATIENCE, call)
        .await
        .expect("the server answers the call")
        .expect("the call was answered, not abandoned")
}

/// A server running over the source it was given, and a client that has said hello to it.
pub(crate) struct Session {
    client: RunningService<RoleClient, ()>,
    /// The task that runs `serve_on`, which ends with what `serve` would return.
    server: JoinHandle<Result<(), ServeError>>,
}

impl Session {
    /// Starts a server over `source` and opens the session with `initialize`.
    pub(crate) async fn start(source: impl IndexSource) -> Self {
        let (client_end, server_end) = tokio::io::duplex(1 << 16);
        let (input, output) = tokio::io::split(server_end);
        let server = tokio::spawn(serve_on(source, input, output));
        let client = tokio::time::timeout(PATIENCE, ().serve(client_end))
            .await
            .expect("the server answers initialize")
            .expect("the session opens");
        Self { client, server }
    }

    /// Closes the client's end, which is the server's input, and returns what `serve` returned once it had.
    pub(crate) async fn close(self) -> Result<(), ServeError> {
        self.client.cancel().await.expect("the client closes");
        tokio::time::timeout(PATIENCE, self.server)
            .await
            .expect("serve returns once its input has closed")
            .expect("the task that runs the server was not cut short")
    }

    /// What the server said on `initialize`.
    pub(crate) fn server_info(&self) -> Arc<ServerPeerInfo> {
        self.client
            .peer_info()
            .expect("the server answered initialize")
    }

    /// The tools the server lists.
    pub(crate) async fn tools(&self) -> Vec<Tool> {
        self.client
            .list_all_tools()
            .await
            .expect("the server lists its tools")
    }

    /// Calls `tool` with `arguments`, which is a JSON object, and returns what the server answered.
    pub(crate) async fn try_call(
        &self,
        tool: &str,
        arguments: Value,
    ) -> Result<CallToolResult, ServiceError> {
        let params = CallToolRequestParams::new(tool.to_owned()).with_arguments(object(arguments));
        tokio::time::timeout(PATIENCE, self.client.call_tool(params))
            .await
            .expect("the server answers the call")
    }

    /// Sends a call of `tool` and returns at once with the task that waits for its answer, so that the test can do
    /// something else while the server works on it.
    pub(crate) fn send(&self, tool: &str, arguments: &Value) -> JoinHandle<Reply> {
        let peer = self.client.peer().clone();
        let tool = tool.to_owned();
        let params =
            CallToolRequestParams::new(tool.clone()).with_arguments(object(arguments.clone()));
        tokio::spawn(async move {
            match peer.call_tool(params).await {
                Ok(result) => Reply::of(&result),
                Err(error) => panic!("`{tool}` was answered with a protocol error: {error}"),
            }
        })
    }

    /// Sends `count` calls of `tool` with the same `arguments` without waiting for any answer in between, and
    /// returns the answers.
    pub(crate) async fn call_together(
        &self,
        tool: &str,
        arguments: &Value,
        count: usize,
    ) -> Vec<Reply> {
        let sent: Vec<_> = (0..count).map(|_| self.send(tool, arguments)).collect();
        let mut replies = Vec::with_capacity(count);
        for call in sent {
            replies.push(answered(call).await);
        }
        replies
    }

    /// Calls `tool` with no `arguments` member at all, and returns the result.
    pub(crate) async fn call_bare(&self, tool: &str) -> Reply {
        let params = CallToolRequestParams::new(tool.to_owned());
        let result = tokio::time::timeout(PATIENCE, self.client.call_tool(params))
            .await
            .expect("the server answers the call");
        match result {
            Ok(result) => Reply::of(&result),
            Err(error) => panic!("`{tool}` was answered with a protocol error: {error}"),
        }
    }

    /// Calls `tool` with `arguments` and returns the result, which fails the test when the server answered with a
    /// protocol error instead.
    pub(crate) async fn call(&self, tool: &str, arguments: Value) -> Reply {
        match self.try_call(tool, arguments).await {
            Ok(result) => Reply::of(&result),
            Err(error) => panic!("`{tool}` was answered with a protocol error: {error}"),
        }
    }
}
