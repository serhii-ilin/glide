//! Shared helpers for integration tests that need a loopback HTTP server.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

/// Items served per `/items?page=N` request by [`spawn_infinite_scroll_server`].
pub const ITEMS_PER_PAGE: usize = 5;

/// Serve `tests/fixtures/infinite-scroll/*.html` and `/items?page=N` (a JSON array of
/// `page-N-item-K` strings) on a loopback port. Returns the base URL, for example
/// `http://127.0.0.1:41234`. The accept thread lives until the test process exits.
pub fn spawn_infinite_scroll_server() -> String {
    spawn_infinite_scroll_server_with_delay(0)
}

/// Like [`spawn_infinite_scroll_server`], but `/items?page=N` answers after `items_delay_ms`
/// (requests are served one at a time, so a slow endpoint also serializes its callers).
pub fn spawn_infinite_scroll_server_with_delay(items_delay_ms: u64) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/infinite-scroll");
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 8192];
            let Ok(n) = stream.read(&mut buf) else {
                continue;
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let target = req
                .lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_string();
            if items_delay_ms > 0 && target.starts_with("/items?") {
                std::thread::sleep(std::time::Duration::from_millis(items_delay_ms));
            }
            let (status, ctype, body) = route(&dir, &target);
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn route(dir: &std::path::Path, target: &str) -> (&'static str, &'static str, String) {
    if let Some(query) = target.strip_prefix("/items?page=") {
        let page: usize = query.parse().unwrap_or(1);
        let items: Vec<String> = (1..=ITEMS_PER_PAGE)
            .map(|k| format!("\"page-{page}-item-{k}\""))
            .collect();
        return (
            "200 OK",
            "application/json",
            format!("[{}]", items.join(",")),
        );
    }
    let name = target.trim_start_matches('/');
    if matches!(name, "io.html" | "scroll-event.html")
        && let Ok(html) = std::fs::read_to_string(dir.join(name))
    {
        return ("200 OK", "text/html; charset=utf-8", html);
    }
    ("404 Not Found", "text/plain", String::new())
}
