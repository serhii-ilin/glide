//! Agent-control request/result model and backends that execute those requests.
//!
//! Headless sessions (`session`) implement [`ControlBackend`] for in-process and MCP clients.

pub mod page_ops;
pub mod protocol;
pub mod session;

use std::time::Duration;

use protocol::{ControlRequest, ControlResult};

/// Executes control requests (headless session, or live window in a later feature).
pub trait ControlBackend: Send + Sync {
    fn call(&self, request: ControlRequest, timeout: Duration) -> Result<ControlResult, String>;
}
