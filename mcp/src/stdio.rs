use std::io::{BufRead, Write};

use crate::proto::{RpcRequest, RpcResponse};
use crate::state::McpState;

/// Run the MCP server in stdio transport mode (line-delimited JSON-RPC 2.0).
///
/// This is the mode expected by Claude Desktop and Cursor.  One JSON object
/// per line on stdin → zero or one JSON objects per line on stdout.
pub fn run_stdio(state: McpState) -> Result<(), Box<dyn std::error::Error>> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());

    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: RpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                // Malformed JSON → send parse-error response and continue.
                let err =
                    RpcResponse::err(serde_json::Value::Null, -32700, format!("Parse error: {e}"));
                writeln!(out, "{}", serde_json::to_string(&err)?)?;
                out.flush()?;
                continue;
            }
        };

        if let Some(resp) = crate::handler::handle_request(req, &state) {
            writeln!(out, "{}", serde_json::to_string(&resp)?)?;
            out.flush()?;
        }
    }

    Ok(())
}
