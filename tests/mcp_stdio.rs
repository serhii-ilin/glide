//! Subprocess tests for `glide --mcp` JSON-RPC over stdio (F-066 @mcp-stdio).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

mod common;

const RECV_TIMEOUT: Duration = Duration::from_secs(30);
const PROTOCOL_VERSION: &str = "2025-06-18";

struct McpClient {
    child: Child,
    stdin: std::process::ChildStdin,
    stdout_lines: Receiver<String>,
    next_id: AtomicU64,
    all_stdout: Arc<std::sync::Mutex<Vec<String>>>,
}

impl McpClient {
    fn spawn_with_args(extra: &[&str]) -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_glide"));
        cmd.arg("--mcp")
            .args(extra)
            .env("GLIDE_LOG", "warn")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = cmd.spawn().expect("spawn glide --mcp");

        let stdout = child.stdout.take().expect("child stdout");
        let stdin = child.stdin.take().expect("child stdin");
        let (tx, rx) = std::sync::mpsc::channel();
        let all_stdout = Arc::new(std::sync::Mutex::new(Vec::new()));
        let capture = Arc::clone(&all_stdout);
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        let _ = capture.lock().map(|mut v| v.push(line.clone()));
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            child,
            stdin,
            stdout_lines: rx,
            next_id: AtomicU64::new(1),
            all_stdout,
        }
    }

    fn spawn() -> Self {
        Self::spawn_with_args(&[])
    }

    fn send_raw(&mut self, value: &Value) {
        let line = serde_json::to_string(value).expect("serialize request");
        writeln!(self.stdin, "{line}").expect("write stdin");
        self.stdin.flush().expect("flush stdin");
    }

    fn recv_line(&self) -> Option<String> {
        match self.stdout_lines.recv_timeout(RECV_TIMEOUT) {
            Ok(line) => Some(line),
            Err(RecvTimeoutError::Timeout) => panic!("timed out waiting for MCP stdout"),
            Err(RecvTimeoutError::Disconnected) => None,
        }
    }

    fn assert_json_rpc_line(line: &str) -> Value {
        let msg: Value = serde_json::from_str(line).unwrap_or_else(|e| {
            panic!("stdout is not JSON ({e}): {line}");
        });
        assert_eq!(
            msg.get("jsonrpc").and_then(|j| j.as_str()),
            Some("2.0"),
            "line: {line}"
        );
        msg
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.send_raw(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        loop {
            let line = self
                .recv_line()
                .unwrap_or_else(|| panic!("EOF waiting for {method} response"));
            let msg = Self::assert_json_rpc_line(&line);
            if msg.get("id") == Some(&json!(id)) {
                return msg;
            }
        }
    }

    fn notify(&mut self, method: &str) {
        self.send_raw(&json!({ "jsonrpc": "2.0", "method": method }));
    }

    fn initialize(&mut self) {
        let resp = self.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "mcp_stdio_test", "version": "0.0.0" }
            }),
        );
        assert!(resp.get("result").is_some(), "initialize failed: {resp}");
        self.notify("notifications/initialized");
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        let resp = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        resp.get("result")
            .cloned()
            .unwrap_or_else(|| panic!("tools/call failed: {resp}"))
    }

    fn call_tool_message(&mut self, name: &str, arguments: Value) -> Value {
        self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )
    }

    fn list_tools(&mut self) -> Vec<Value> {
        let resp = self.request("tools/list", json!({}));
        resp.get("result")
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_else(|| panic!("tools/list failed: {resp}"))
    }

    fn stdout_lines(&self) -> Vec<String> {
        self.all_stdout
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default()
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    format!("file://{}", path.display())
}

fn missing_file_url() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join("no-such-mcp-page.html");
    format!("file://{}", path.display())
}

fn tool_text(result: &Value) -> String {
    result
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|blocks| blocks.first())
        .and_then(|b| b.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string()
}

fn is_tool_error(result: &Value) -> bool {
    result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn page_title(result: &Value) -> Option<String> {
    serde_json::from_str::<Value>(&tool_text(result))
        .ok()
        .and_then(|v| v.get("title").and_then(|t| t.as_str()).map(str::to_string))
}

#[test]
fn initialize_and_list_tools() {
    let mut client = McpClient::spawn();
    client.initialize();
    let tools = client.list_tools();
    let names: Vec<String> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .collect();
    for expected in [
        "glide_navigate",
        "glide_back",
        "glide_forward",
        "glide_reload",
        "glide_page_info",
        "glide_snapshot_text",
        "glide_dom",
        "glide_query",
        "glide_computed_style",
        "glide_screenshot",
        "glide_scroll",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "missing tool {expected}: {names:?}"
        );
    }
    for tool in &tools {
        assert!(
            tool.get("inputSchema").is_some(),
            "tool missing inputSchema: {tool}"
        );
    }
}

#[test]
fn navigate_then_snapshot_text_over_stdio() {
    let mut client = McpClient::spawn();
    client.initialize();
    let nav = client.call_tool(
        "glide_navigate",
        json!({ "url": fixture_url("mcp-page-a.html") }),
    );
    assert!(!is_tool_error(&nav));
    assert_eq!(page_title(&nav).as_deref(), Some("Page A"));
    let snap = client.call_tool("glide_snapshot_text", json!({}));
    let text = tool_text(&snap);
    assert!(text.contains("MCP Page A"));
}

#[test]
fn back_forward_over_stdio() {
    let mut client = McpClient::spawn();
    client.initialize();
    client.call_tool(
        "glide_navigate",
        json!({ "url": fixture_url("mcp-page-a.html") }),
    );
    client.call_tool(
        "glide_navigate",
        json!({ "url": fixture_url("mcp-page-b.html") }),
    );
    let back = client.call_tool("glide_back", json!({}));
    assert_eq!(page_title(&back).as_deref(), Some("Page A"));
    let fwd = client.call_tool("glide_forward", json!({}));
    assert_eq!(page_title(&fwd).as_deref(), Some("Page B"));
}

#[test]
fn reload_over_stdio() {
    let mut client = McpClient::spawn();
    client.initialize();
    client.call_tool(
        "glide_navigate",
        json!({ "url": fixture_url("mcp-page-a.html") }),
    );
    let reload = client.call_tool("glide_reload", json!({}));
    assert!(!is_tool_error(&reload));
    let dom = client.call_tool("glide_dom", json!({ "selector": "#run" }));
    let html = tool_text(&dom);
    assert!(html.contains("id=\"run\""));
}

#[test]
fn page_info_over_stdio() {
    let mut client = McpClient::spawn();
    client.initialize();
    client.call_tool(
        "glide_navigate",
        json!({ "url": fixture_url("mcp-page-a.html") }),
    );
    let info = client.call_tool("glide_page_info", json!({}));
    let page: Value = serde_json::from_str(&tool_text(&info)).expect("page json");
    assert_eq!(page.get("title").and_then(|t| t.as_str()), Some("Page A"));
    assert_eq!(page.get("status").and_then(|s| s.as_str()), Some("settled"));
    assert_eq!(page.get("js_enabled").and_then(|j| j.as_bool()), Some(true));
}

#[test]
fn dom_and_query_over_stdio() {
    let mut client = McpClient::spawn();
    client.initialize();
    client.call_tool(
        "glide_navigate",
        json!({ "url": fixture_url("mcp-page-a.html") }),
    );
    let dom = client.call_tool("glide_dom", json!({ "selector": "#box", "all": false }));
    let html = tool_text(&dom);
    assert!(html.contains("id=\"box\""));
    assert!(html.contains("Box"));
    let query = client.call_tool("glide_query", json!({ "selector": "#box" }));
    let elements: Vec<Value> = serde_json::from_str(&tool_text(&query)).expect("query json");
    let visible = elements.iter().any(|el| {
        el.get("visible").and_then(|v| v.as_bool()) == Some(true)
            && el
                .get("rect")
                .and_then(|r| r.get("width"))
                .and_then(|w| w.as_f64())
                .is_some_and(|w| w > 100.0 && w < 140.0)
    });
    assert!(visible, "expected visible #box: {elements:?}");
}

#[test]
fn computed_style_over_stdio() {
    let mut client = McpClient::spawn();
    client.initialize();
    client.call_tool(
        "glide_navigate",
        json!({ "url": fixture_url("devtools-computed-sample.html") }),
    );
    let styles = client.call_tool("glide_computed_style", json!({ "selector": "#box" }));
    assert!(!is_tool_error(&styles));
    let map: serde_json::Map<String, Value> =
        serde_json::from_str(&tool_text(&styles)).expect("computed json");
    assert_eq!(map.get("display").and_then(|v| v.as_str()), Some("flex"));
    let missing = client.call_tool("glide_computed_style", json!({ "selector": "#missing" }));
    assert!(is_tool_error(&missing));
}

#[test]
fn dom_missing_selector_is_tool_error() {
    let mut client = McpClient::spawn();
    client.initialize();
    client.call_tool(
        "glide_navigate",
        json!({ "url": fixture_url("mcp-page-a.html") }),
    );
    let dom = client.call_tool("glide_dom", json!({ "selector": "#missing", "all": false }));
    assert!(is_tool_error(&dom));
    let msg = tool_text(&dom);
    assert!(
        msg.contains("#missing"),
        "error should name selector: {msg}"
    );
}

#[test]
fn missing_file_navigation_is_tool_error() {
    let mut client = McpClient::spawn();
    client.initialize();
    client.call_tool(
        "glide_navigate",
        json!({ "url": fixture_url("mcp-page-a.html") }),
    );
    let nav = client.call_tool("glide_navigate", json!({ "url": missing_file_url() }));
    assert!(is_tool_error(&nav));
    let info = client.call_tool("glide_page_info", json!({}));
    assert_eq!(page_title(&info).as_deref(), Some("Page A"));
}

#[test]
fn no_js_server_reads_static_text() {
    let mut client = McpClient::spawn_with_args(&["--no-js"]);
    client.initialize();
    client.call_tool(
        "glide_navigate",
        json!({ "url": fixture_url("mcp-page-a.html") }),
    );
    let snap = client.call_tool("glide_snapshot_text", json!({}));
    let text = tool_text(&snap);
    assert!(text.contains("MCP Page A"));
    assert!(!text.contains("script ran"));
}

#[test]
fn missing_required_argument_is_protocol_error() {
    let mut client = McpClient::spawn();
    client.initialize();
    let bad_method = client.request("no_such_method", json!({}));
    assert!(
        bad_method.get("error").is_some(),
        "expected JSON-RPC error for unknown method: {bad_method}"
    );
    let resp = client.call_tool_message("glide_navigate", json!({}));
    assert!(
        resp.get("error").is_some() || is_tool_error(resp.get("result").unwrap_or(&Value::Null)),
        "expected tool or protocol error for missing url: {resp}"
    );
    let tools = client.list_tools();
    assert!(
        tools
            .iter()
            .any(|t| t.get("name").and_then(|n| n.as_str()) == Some("glide_navigate"))
    );
}

#[test]
fn screenshot_returns_png_image_content() {
    let mut client = McpClient::spawn();
    client.initialize();
    client.call_tool(
        "glide_navigate",
        json!({ "url": fixture_url("mcp-page-a.html") }),
    );
    let shot = client.call_tool("glide_screenshot", json!({}));
    assert!(!is_tool_error(&shot));
    let block = shot
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .expect("image block");
    assert_eq!(block.get("type").and_then(|t| t.as_str()), Some("image"));
    assert_eq!(
        block.get("mimeType").and_then(|t| t.as_str()),
        Some("image/png")
    );
    let data = block
        .get("data")
        .and_then(|d| d.as_str())
        .expect("base64 data");
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .expect("base64 png");
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
    let reader = decoder.read_info().expect("png info");
    assert_eq!(reader.info().width, 1100);
    assert_eq!(reader.info().height, 760);
}

#[test]
fn every_stdout_line_is_json_rpc() {
    let mut client = McpClient::spawn();
    client.initialize();
    client.call_tool(
        "glide_navigate",
        json!({ "url": fixture_url("mcp-page-a.html") }),
    );
    client.call_tool("glide_page_info", json!({}));
    client.call_tool("glide_snapshot_text", json!({}));
    let _ping = client.request("ping", json!({}));
    for line in client.stdout_lines() {
        McpClient::assert_json_rpc_line(&line);
    }
}

#[test]
fn stdin_eof_exits_cleanly() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_glide"))
        .arg("--mcp")
        .env("GLIDE_LOG", "warn")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        let init = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "eof-test", "version": "0" }
            }
        });
        writeln!(stdin, "{}", serde_json::to_string(&init).expect("json")).expect("write init");
        let initialized = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        writeln!(
            stdin,
            "{}",
            serde_json::to_string(&initialized).expect("json")
        )
        .expect("write initialized");
        stdin.flush().expect("flush");
    }
    drop(child.stdin.take());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            assert_eq!(status.code(), Some(0), "stdin EOF should exit cleanly");
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("glide --mcp did not exit within 10s after stdin EOF");
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn attach_is_not_supported_yet() {
    let output = Command::new(env!("CARGO_BIN_EXE_glide"))
        .args(["--mcp", "--attach"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run attach");
    assert_eq!(output.status.code(), Some(2), "attach must exit 2");
    let err = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        err.contains("glide --mcp --attach is not supported yet"),
        "unexpected output (code={:?}): {err:?}",
        output.status.code()
    );
}

fn scroll_info(result: &Value) -> Value {
    serde_json::from_str(&tool_text(result)).expect("scroll result JSON")
}

// Source: CSSOM View Module (scroll offsets); product decision (infinite scroll over MCP).
#[test]
fn scroll_then_snapshot_text_loads_more_items_over_stdio() {
    let base = common::spawn_infinite_scroll_server();
    let mut client = McpClient::spawn();
    client.initialize();
    let nav = client.call_tool(
        "glide_navigate",
        json!({ "url": format!("{base}/io.html") }),
    );
    assert!(!is_tool_error(&nav));
    let before = tool_text(&client.call_tool("glide_snapshot_text", json!({})));
    assert!(before.contains("page-1-item-1"), "{before}");
    assert!(!before.contains("page-2-item-1"), "{before}");
    let scrolled = client.call_tool("glide_scroll", json!({ "dy": 1000000 }));
    assert!(!is_tool_error(&scrolled), "{scrolled}");
    let info = scroll_info(&scrolled);
    assert!(info["y"].as_f64().unwrap_or(0.0) > 0.0, "{info}");
    assert_eq!(info["viewport_height"], 760);
    let after = tool_text(&client.call_tool("glide_snapshot_text", json!({})));
    assert!(after.contains("page-2-item-1"), "{after}");
}

#[test]
fn scroll_waits_for_slow_fetch_and_reports_settled_over_stdio() {
    let base = common::spawn_infinite_scroll_server_with_delay(700);
    let mut client = McpClient::spawn();
    client.initialize();
    client.call_tool(
        "glide_navigate",
        json!({ "url": format!("{base}/scroll-event.html") }),
    );
    let scrolled = client.call_tool("glide_scroll", json!({ "dy": 1000000 }));
    assert!(!is_tool_error(&scrolled), "{scrolled}");
    assert_eq!(scroll_info(&scrolled)["settled"], true);
    let after = tool_text(&client.call_tool("glide_snapshot_text", json!({})));
    assert!(after.contains("page-2-item-1"), "{after}");
}

#[test]
fn scroll_to_missing_selector_is_tool_error_over_stdio() {
    let mut client = McpClient::spawn();
    client.initialize();
    client.call_tool(
        "glide_navigate",
        json!({ "url": fixture_url("scroll-colors.html") }),
    );
    let result = client.call_tool("glide_scroll", json!({ "to_selector": "#nope" }));
    assert!(is_tool_error(&result), "{result}");
}

#[test]
fn full_page_screenshot_is_taller_than_viewport_over_stdio() {
    let mut client = McpClient::spawn();
    client.initialize();
    client.call_tool(
        "glide_navigate",
        json!({ "url": fixture_url("scroll-colors.html") }),
    );
    let shot = client.call_tool("glide_screenshot", json!({ "full_page": true }));
    assert!(!is_tool_error(&shot));
    let data = shot["content"][0]["data"].as_str().expect("base64 data");
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .expect("base64 png");
    let reader = png::Decoder::new(std::io::Cursor::new(&bytes))
        .read_info()
        .expect("png info");
    assert_eq!(reader.info().width, 1100);
    assert!(reader.info().height >= 2050, "{}", reader.info().height);
}
