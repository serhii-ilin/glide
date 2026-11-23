//! Page JavaScript via V8 (`v8` / rusty_v8).

mod apply;
mod batch;
mod bridge;
pub mod console_feed;
pub mod diagnostics;
mod form_controls;
mod host;
pub mod import_map;
mod modules;
mod paint_prune;
mod promise_console;
mod repl_eval;
pub mod scripts;
mod snapshot;
mod tap;
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
