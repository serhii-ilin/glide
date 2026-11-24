//! Page JavaScript via V8 (`v8` / rusty_v8).

mod apply;
mod batch;
mod bridge;
pub mod console_feed;
pub mod diagnostics;
mod fetch_body;
mod form_controls;
mod form_data;
mod history_rpc;
mod host;
pub mod import_map;
mod modules;
mod paint_prune;
mod promise_console;
mod repl_eval;
pub mod scripts;
mod snapshot;
pub mod storage_events;
mod tap;
pub mod viewport;
pub mod websocket;
mod worker;

#[cfg(test)]
mod harness;

#[cfg(test)]
mod integration;

pub use host::JsHost;
pub(crate) use host::page_subdoc_hover_is_none;
pub(crate) use paint_prune::prune_stale_paint_children;
pub use repl_eval::json_eval_source;
pub use tap::JsTap;
pub use worker::EvalReply;
