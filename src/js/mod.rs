//! Page JavaScript via V8 (`v8` / rusty_v8).

mod apply;
mod batch;
mod bridge;
pub mod diagnostics;
mod host;
pub mod scripts;
mod snapshot;
mod tap;
mod worker;

#[cfg(test)]
mod harness;

#[cfg(test)]
mod integration;

pub use host::JsHost;
pub use tap::JsTap;
