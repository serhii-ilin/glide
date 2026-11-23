//! Glide, a small desktop browser.
//!
//! The window, input, and rasterizer come from Blitz (the same engine
//! Cloudflare's Kitesurf embeds in WebAssembly). See the README for why the
//! Kitesurf crates themselves are not linked here.

pub mod about;
mod app;
mod app_icon;
pub mod args;
mod bookmarks;
pub mod chrome;
pub mod compat;
pub mod control;
pub mod cookies;
pub mod custom_elements;
pub mod devtools;
pub mod find;
pub mod headless;
pub mod history;
pub mod html;
mod http;
pub mod js;
pub mod keyboard;
pub mod location;
pub mod logging;
pub mod mcp;
pub mod menu;
#[cfg(target_os = "macos")]
mod native_menu;
pub mod nav;
pub mod nav_request;
pub mod net;
pub mod network_feed;
pub mod parse;
pub mod profile;
pub mod remote_socket;
mod runtime;
pub mod shot;
pub mod tabs;
pub mod zoom;

pub use app::{WindowLaunch, prepare_window_launch, run_window};
pub use runtime::tokio_runtime;
