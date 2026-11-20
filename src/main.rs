//! `glide` binary entry: parse CLI args and run the window or headless screenshot.

use glide::args::{RunMode, parse, usage};
use glide::{run_window, shot, tokio_runtime};

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
        Ok(RunMode::Window { url, js_enabled }) => {
            let runtime = tokio_runtime();
            let _guard = runtime.enter();
            if let Err(err) = run_window(url, js_enabled) {
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
