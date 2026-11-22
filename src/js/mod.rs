//! Page JavaScript via V8 (`v8` / rusty_v8).

mod apply;
mod batch;
mod bridge;
pub mod console_feed;
pub mod diagnostics;
mod host;
mod paint_prune;
mod promise_console;
mod repl_eval;
pub mod scripts;
mod snapshot;
mod tap;
mod worker;

#[cfg(test)]
mod harness;

#[cfg(test)]
mod integration;

pub use host::JsHost;
pub(crate) use paint_prune::prune_stale_paint_children;
pub use tap::JsTap;
