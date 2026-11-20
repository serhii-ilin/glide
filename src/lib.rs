//! Glide, a small desktop browser.
//!
//! The window, input, and rasterizer come from Blitz (the same engine
//! Cloudflare's Kitesurf embeds in WebAssembly). See the README for why the
//! Kitesurf crates themselves are not linked here.

mod app;
pub mod args;
mod bookmarks;
pub mod chrome;
pub mod compat;
pub mod find;
pub mod headless;
pub mod history;
pub mod html;
pub mod js;
pub mod keyboard;
pub mod location;
pub mod nav;
pub mod net;
pub mod parse;
pub mod profile;
mod runtime;
pub mod shot;
pub mod zoom;

pub use app::{WindowLaunch, prepare_window_launch, run_window};
pub use runtime::tokio_runtime;
