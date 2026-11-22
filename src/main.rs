//! `glide` binary entry: parse CLI args and run the window, headless screenshot, or --dump-text.

use std::sync::Arc;

use glide::args::{RunMode, parse, usage};
use glide::control::session::{SessionOptions, spawn_session};
use glide::{headless, mcp, run_window, shot, tokio_runtime};

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    match parse(raw) {
        Ok(RunMode::Help) => {
            print!("{}", usage());
        }
        Ok(RunMode::Screenshot {
            url,
            output,
            js_enabled,
        }) => {
            let runtime = tokio_runtime();
            let _guard = runtime.enter();
            let render = if js_enabled {
                shot::render_url
            } else {
                shot::render_url_no_js
            };
            match render(&url, std::path::Path::new(&output)) {
                Ok(()) => println!("wrote {output}"),
                Err(err) => {
                    eprintln!("screenshot failed: {err}");
                    std::process::exit(1);
                }
            }
        }
        Ok(RunMode::Mcp { js_enabled, attach }) => {
            if attach.is_some() {
                eprintln!("glide --mcp --attach is not supported yet (planned F-069)");
                std::process::exit(2);
            }
            let runtime = tokio_runtime();
            let backend = spawn_session(SessionOptions { js_enabled }, runtime.handle().clone());
            let backend = match backend {
                Ok(b) => Arc::new(b) as Arc<dyn glide::control::ControlBackend>,
                Err(err) => {
                    eprintln!("{err}");
                    std::process::exit(1);
                }
            };
            if let Err(err) = runtime.block_on(mcp::serve_stdio(backend, js_enabled)) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        Ok(RunMode::DumpText { url, js_enabled }) => {
            let runtime = tokio_runtime();
            let _guard = runtime.enter();
            match headless::dump_text(&url, js_enabled) {
                Ok(text) => print!("{text}"),
                Err(err) => {
                    eprintln!("dump-text failed: {err}");
                    std::process::exit(1);
                }
            }
        }
        Ok(RunMode::Window {
            cli_url,
            js_enabled: _,
            cli_no_js,
        }) => {
            let runtime = tokio_runtime();
            let _guard = runtime.enter();
            let profile = glide::profile::ProfileStore::open();
            let launch = glide::prepare_window_launch(cli_url.as_deref(), cli_no_js, profile);
            if let Err(err) = run_window(launch) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        Err(err) => {
            eprintln!("{err}\n");
            print!("{}", usage());
            std::process::exit(2);
        }
    }
}
