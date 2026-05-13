use serde_json::json;

use crate::dispatch::dispatch_tool;
use crate::proto::{RpcRequest, RpcResponse};
use crate::state::McpState;
use crate::tools::tool_list;

/// Dispatch a JSON-RPC 2.0 request to the appropriate MCP handler.
///
/// Returns `None` for notifications (no `id`) — the transport must not send
/// a response in that case.  Returns `Some(RpcResponse)` for all requests.
pub fn handle_request(req: RpcRequest, state: &McpState) -> Option<RpcResponse> {
    let id = req.id.clone().unwrap_or(serde_json::Value::Null);

    // Notifications have no id — process but do not respond.
    let is_notification = req.id.is_none();

    let response = match req.method.as_str() {
        // ------------------------------------------------------------------ //
        // Lifecycle                                                            //
        // ------------------------------------------------------------------ //
        "initialize" => Some(RpcResponse::ok(
            id,
            json!({
                "protocolVersion": "2025-03-26",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "skycode-mcp", "version": "0.1.0" }
            }),
        )),

        // Client acknowledgement — no response required.
        "notifications/initialized" => None,

        // ------------------------------------------------------------------ //
        // Tool introspection                                                   //
        // ------------------------------------------------------------------ //
        "tools/list" => Some(RpcResponse::ok(id, json!({ "tools": tool_list() }))),

        // ------------------------------------------------------------------ //
        // Tool invocation                                                      //
        // ------------------------------------------------------------------ //
        "tools/call" => {
            let params = req.params.unwrap_or_default();
            let name = params["name"].as_str().unwrap_or("").to_string();
            let args = params["arguments"].clone();
            let result = dispatch_tool(&name, args, state);
            Some(RpcResponse::ok(id, result))
        }

        // ------------------------------------------------------------------ //
        // Unknown method                                                       //
        // ------------------------------------------------------------------ //
        other => {
            if is_notification {
                // Unknown notifications are silently ignored per JSON-RPC 2.0.
                None
            } else {
                Some(RpcResponse::err(
                    id,
                    -32601,
                    format!("Method not found: {other}"),
                ))
            }
        }
    };

    // Notifications must never produce a response even if the method matched.
    if is_notification {
        None
    } else {
        response
    }
}
