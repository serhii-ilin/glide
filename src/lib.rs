//! Glide, a small desktop browser.
//!
//! The window, input, and rasterizer come from Blitz (the same engine
//! Cloudflare's Kitesurf embeds in WebAssembly). See the README for why the
//! Kitesurf crates themselves are not linked here.

mod app;
pub mod args;
pub mod chrome;
pub mod history;
pub mod html;
pub mod js;
pub mod keyboard;
pub mod location;
pub mod parse;
pub mod shot;

pub use app::run_window;
