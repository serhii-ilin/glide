//! MCP stdio server (`glide --mcp`): exposes headless session control as MCP tools.

mod tools;

use std::sync::Arc;

use rmcp::ServiceExt;

pub use tools::GlideMcp;

/// Start the MCP server on stdin/stdout with the given control backend.
pub async fn serve_stdio(
    backend: Arc<dyn crate::control::ControlBackend>,
    js_enabled: bool,
) -> Result<(), String> {
    crate::logging::info(
        "mcp.server",
        "started",
        &[
            ("backend", "headless".into()),
            ("js_enabled", js_enabled.to_string()),
        ],
    );
    let server = GlideMcp::new(backend);
    let running = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| format!("mcp serve failed: {e}"))?;
    running
        .waiting()
        .await
        .map(|_| ())
        .map_err(|e| format!("mcp server exited: {e}"))
}

/// Fields for an `mcp.server` tool-call debug record (tested by unit tests).
pub fn tool_call_log_fields(method: &str, status: &str) -> Vec<(&'static str, String)> {
    vec![("method", method.into()), ("status", status.into())]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logging::{Level, format_record};

    #[test]
    fn tool_call_log_record_shape() {
        let fields = tool_call_log_fields("glide_navigate", "ok");
        let line = format_record(
            Level::Debug,
            "mcp.server",
            "tool_call",
            &fields,
            Level::Debug,
        )
        .expect("debug line");
        assert!(line.contains("target=\"mcp.server\""));
        assert!(line.contains("method=\"glide_navigate\""));
        assert!(line.contains("status=\"ok\""));
    }
}
