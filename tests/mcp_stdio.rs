//! Subprocess tests for `glide --mcp` JSON-RPC over stdio (F-066 @mcp-stdio).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

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
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_glide"))
            .arg("--mcp")
            .env("GLIDE_LOG", "warn")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn glide --mcp");

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
            let msg: Value = serde_json::from_str(&line).unwrap_or_else(|e| {
                panic!("stdout is not JSON ({e}): {line}");
            });
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
        "glide_screenshot",
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
    let page_json = tool_text(&nav);
    let title = serde_json::from_str::<Value>(&page_json)
        .ok()
        .and_then(|v| v.get("title").and_then(|t| t.as_str()).map(str::to_string));
    assert_eq!(title.as_deref(), Some("Page A"));
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
    let back_title = serde_json::from_str::<Value>(&tool_text(&back))
        .ok()
        .and_then(|v| v.get("title").and_then(|t| t.as_str()).map(str::to_string));
    assert_eq!(back_title.as_deref(), Some("Page A"));
    let fwd = client.call_tool("glide_forward", json!({}));
    let fwd_title = serde_json::from_str::<Value>(&tool_text(&fwd))
        .ok()
        .and_then(|v| v.get("title").and_then(|t| t.as_str()).map(str::to_string));
    assert_eq!(fwd_title.as_deref(), Some("Page B"));
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
    for line in client.stdout_lines() {
        let msg: Value = serde_json::from_str(&line).unwrap_or_else(|_| panic!("not JSON: {line}"));
        assert_eq!(
            msg.get("jsonrpc").and_then(|j| j.as_str()),
            Some("2.0"),
            "line: {line}"
        );
    }
}

#[test]
fn attach_is_not_supported_yet() {
    let output = Command::new(env!("CARGO_BIN_EXE_glide"))
        .args(["--mcp", "--attach"])
        .output()
        .expect("run attach");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("glide --mcp --attach is not supported yet"));
}
