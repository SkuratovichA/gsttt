//! The handler of the protocol: what the server says on `initialize`, which tools it lists, and how it takes a call.
//!
//! It is written by hand and not with the `#[tool]` router, because the router answers arguments that do not
//! deserialize with a JSON-RPC error, while the specification of the protocol calls those tool execution errors, so
//! that the model can correct the call.
//!
//! The one exception is an `arguments` that is not an object. rmcp reads the parameters of a call before it asks the handler, so such a call never reaches `call_tool`, and `on_custom_request` answers it with the protocol error for invalid parameters.

use std::sync::Arc;

use gsttt_core::text::join_and;
use rmcp::model::{
    CallToolRequestMethod, CallToolRequestParams, CallToolResponse, CallToolResult, ConstString,
    ContentBlock, CustomRequest, CustomResult, ErrorCode, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig,
};
use rmcp::service::{MaybeSendFuture, RequestContext};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};

use crate::arguments::Call;
use crate::ask;
use crate::error::ToolError;
use crate::gate::{Gate, Stopped};
use crate::source::IndexSource;
use crate::tools::{self, ToolName};

/// What the client is told on `initialize` about when to call each tool. The specification test pins the words.
const INSTRUCTIONS: &str = "gsttt answers what a change affects, from an index of imports and contracts (HTTP routes, \
GraphQL operations, gRPC methods, tables, queues, env variables) across the repository. Call impact before changing a \
file that others may use, or with changed=true before finishing a task; dependents before renaming or deleting; path \
to learn how two files are connected; contract to find who serves and who calls an endpoint. Every answer ends with \
what it covered and how fresh it is; when coverage names a gap, say so instead of assuming nothing is affected.";

/// The server: it answers `initialize` and `tools/list` from itself, and every other question through the source.
pub(crate) struct Server<S> {
    source: Arc<S>,
    /// Gives each tool call its turn at the source.
    gate: Gate,
}

impl<S: IndexSource> Server<S> {
    pub(crate) fn new(source: S) -> Self {
        Self {
            source: Arc::new(source),
            gate: Gate::default(),
        }
    }
}

impl<S: IndexSource> Server<S> {
    /// The entry of `tools/list`, which needs nothing from the source.
    fn listing() -> Result<ListToolsResult, McpError> {
        let tools = tools::listing().map_err(|reason| {
            McpError::internal_error(
                format!("the input schema of a tool could not be built: {reason}"),
                None,
            )
        })?;
        Ok(ListToolsResult::with_all_items(tools))
    }

    /// Answers a call to one of the tools. A tool the server does not offer is a protocol error; everything that goes
    /// wrong in a known tool is a result with `isError` set, so that the model reads it and corrects the call.
    async fn answer(&self, request: CallToolRequestParams) -> Result<CallToolResponse, McpError> {
        let Some(tool) = ToolName::from_name(&request.name) else {
            return Err(unknown_tool(&request.name));
        };
        tracing::debug!(%tool, "tool call");
        // The arguments are read before the call waits for its turn, so that a call that is wrong from the start is
        // told so at once and not after the refresh in front of it.
        let reply = match Call::read(tool, request.arguments) {
            Ok(call) => self.call_source(tool, call).await,
            Err(error) => Err(error),
        };
        let result = match reply {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Err(error) => {
                tracing::debug!(%tool, %error, "tool call failed");
                CallToolResult::error(vec![ContentBlock::text(error.to_string())])
            }
        };
        Ok(CallToolResponse::Complete(result))
    }

    /// Puts `call` to the source: on a blocking thread, and with no other call in progress. The turn is the whole tool
    /// call, so the paths it reads and the refresh that follows are not split by another call.
    async fn call_source(&self, tool: ToolName, call: Call) -> Result<String, ToolError> {
        let source = Arc::clone(&self.source);
        match self.gate.run(move || ask::run(source.as_ref(), call)).await {
            Ok(reply) => reply,
            Err(Stopped) => Err(ToolError::Stopped { tool }),
        }
    }
}

impl<S: IndexSource> ServerHandler for Server<S> {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("gsttt", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + MaybeSendFuture + '_ {
        std::future::ready(Self::listing())
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResponse, McpError>> + MaybeSendFuture + '_ {
        self.answer(request)
    }

    /// Takes a request that did not read as any request the server knows, which includes a `tools/call` whose
    /// `arguments` is not an object: rmcp reads the parameters before the handler is asked.
    fn on_custom_request(
        &self,
        request: CustomRequest,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CustomResult, McpError>> + MaybeSendFuture + '_ {
        std::future::ready(Err(unreadable(request.method)))
    }
}

/// The protocol error for a request that did not read. rmcp's own answer is `method not found` with the method's
/// name, which, for a `tools/call` with malformed parameters, tells the client that the server has no tools at all.
fn unreadable(method: String) -> McpError {
    if method == CallToolRequestMethod::VALUE {
        McpError::invalid_params(
            "`tools/call` takes `name`, a string, and `arguments`, an object that may be left out",
            None,
        )
    } else {
        McpError::new(ErrorCode::METHOD_NOT_FOUND, method, None)
    }
}

/// The protocol error for a tool the server does not offer, which the specification gives the code of invalid params.
fn unknown_tool(name: &str) -> McpError {
    let offered: Vec<String> = ToolName::ALL
        .into_iter()
        .map(|tool| tool.name().to_owned())
        .collect();
    McpError::invalid_params(
        format!(
            "unknown tool `{name}`; the tools are {}",
            join_and(&offered)
        ),
        None,
    )
}
