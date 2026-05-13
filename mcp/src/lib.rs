//! SkyCodeOS MCP server — MCP 2025-03-26 protocol implementation.
//!
//! Exposes all SkyCodeOS tools (diff approval, apply, memory search, …) as
//! MCP tools callable from Claude Desktop, Cursor, and SkaiRPG.
//!
//! Two transports:
//! - **stdio** (`run_stdio`) — for Claude Desktop / Cursor local integration.
//! - **SSE/HTTP** (`run_sse`)  — POST `/mcp` over LAN / Tailscale.

pub mod dispatch;
pub mod handler;
pub mod proto;
pub mod sse;
pub mod state;
pub mod stdio;
pub mod tools;
