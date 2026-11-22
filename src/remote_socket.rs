//! Local NDJSON request/response socket used for opt-in remote control of a live window.
//!
//! [`RemoteServer::bind`] listens on a user-only Unix socket under the profile directory and
//! answers one JSON object per newline-delimited line; [`RemoteClient`] is the matching client
//! used by `glide --mcp --attach`. Payloads are opaque [`serde_json::Value`]s: this module owns
//! the transport (framing, limits, permissions, shutdown), not the meaning of a request.

use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const SOCKET_FILE: &str = "remote-control.sock";

/// Longest accepted line, excluding the trailing newline.
pub const MAX_LINE_BYTES: usize = 1 << 20;

pub type Handler =
    Arc<dyn Fn(serde_json::Value) -> Result<serde_json::Value, String> + Send + Sync>;

/// `<profile_dir>/run/remote-control.sock`.
pub fn socket_path(profile_dir: &Path) -> PathBuf {
    profile_dir.join("run").join(SOCKET_FILE)
}

/// One newline-terminated request frame: `{"v":1,"id":..,"request":..}`.
pub fn encode_request(id: u64, request: &serde_json::Value) -> String {
    format!("{{\"v\":1,\"id\":{id},\"request\":{request}}}\n")
}

/// Decode one request line. The error carries the id when it was parseable.
pub fn parse_request_line(line: &str) -> Result<(u64, serde_json::Value), (Option<u64>, String)> {
    let value: serde_json::Value = serde_json::from_str(line.trim_end_matches(['\n', '\r']))
        .map_err(|err| (None, format!("bad request: {err}")))?;
    let Some(object) = value.as_object() else {
        return Err((None, "bad request: expected a JSON object".to_string()));
    };
    let Some(id) = object.get("id").and_then(serde_json::Value::as_u64) else {
        return Err((None, "bad request: missing or invalid id".to_string()));
    };
    match object.get("v") {
        Some(serde_json::Value::Number(version)) if version.as_u64() == Some(1) => {}
        Some(_) => return Err((Some(id), "unsupported protocol version".to_string())),
        None => return Err((Some(id), "bad request: missing v".to_string())),
    }
    let Some(request) = object.get("request") else {
        return Err((Some(id), "bad request: missing request".to_string()));
    };
    Ok((id, request.clone()))
}

/// One newline-terminated response frame: `{"id":..,"ok":..}` or `{"id":..,"err":..}`.
pub fn encode_response(id: Option<u64>, result: &Result<serde_json::Value, String>) -> String {
    let id = id.map_or_else(|| "null".to_string(), |id| id.to_string());
    let mut line = match result {
        Ok(value) => format!("{{\"id\":{id},\"ok\":{value}}}"),
        Err(message) => format!(
            "{{\"id\":{id},\"err\":{}}}",
            serde_json::Value::String(message.clone())
        ),
    };
    line.push('\n');
    line
}

/// Decode one response line. The outer error means the frame itself was unreadable.
pub fn parse_response_line(
    line: &str,
) -> Result<(Option<u64>, Result<serde_json::Value, String>), String> {
    let value: serde_json::Value = serde_json::from_str(line.trim_end_matches(['\n', '\r']))
        .map_err(|err| format!("bad response: {err}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "bad response: expected a JSON object".to_string())?;
    let id = match object.get("id") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Number(number)) => {
            let id = number
                .as_u64()
                .ok_or_else(|| "bad response: invalid id".to_string())?;
            Some(id)
        }
        Some(_) => return Err("bad response: invalid id".to_string()),
    };
    if let Some(ok) = object.get("ok") {
        return Ok((id, Ok(ok.clone())));
    }
    if let Some(serde_json::Value::String(message)) = object.get("err") {
        return Ok((id, Err(message.clone())));
    }
    Err("bad response: missing ok or err".to_string())
}

#[cfg(unix)]
const TARGET: &str = "remote.socket";
#[cfg(unix)]
const REQUEST_HINT: &str = "send one JSON request object per line; see F-069";
#[cfg(unix)]
const ACCEPT_POLL: std::time::Duration = std::time::Duration::from_millis(10);

/// A live listener serving each connection on its own thread.
#[cfg(unix)]
#[derive(Debug)]
pub struct RemoteServer {
    path: PathBuf,
    stop: Arc<std::sync::atomic::AtomicBool>,
    accept: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
}

#[cfg(unix)]
impl RemoteServer {
    /// Bind the user-only socket at `path`, replacing a stale socket file.
    ///
    /// The parent directory is created or tightened to mode 0700, the socket is chmodded to
    /// 0600, and a live server at the same path makes this fail instead of hijacking it.
    pub fn bind(path: &Path, handler: Handler) -> Result<RemoteServer, String> {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::net::{UnixListener, UnixStream};
        use std::sync::atomic::AtomicBool;
        use std::thread;

        let parent = path.parent().ok_or_else(|| {
            format!(
                "remote control socket path has no parent: {}",
                path.display()
            )
        })?;
        fs::create_dir_all(parent)
            .map_err(|err| format!("remote control create {}: {err}", parent.display()))?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|err| format!("remote control chmod {}: {err}", parent.display()))?;
        if UnixStream::connect(path).is_ok() {
            return Err(format!(
                "remote control already active at {}",
                path.display()
            ));
        }
        match fs::symlink_metadata(path) {
            Ok(_) => fs::remove_file(path)
                .map_err(|err| format!("remote control remove stale {}: {err}", path.display()))?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(format!("remote control inspect {}: {err}", path.display()));
            }
        }
        let listener = UnixListener::bind(path)
            .map_err(|err| format!("remote control bind {}: {err}", path.display()))?;
        if let Err(err) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
            let _ = fs::remove_file(path);
            return Err(format!("remote control chmod {}: {err}", path.display()));
        }
        if let Err(err) = listener.set_nonblocking(true) {
            let _ = fs::remove_file(path);
            return Err(format!("remote control listen {}: {err}", path.display()));
        }
        let stop = Arc::new(AtomicBool::new(false));
        let accept_stop = Arc::clone(&stop);
        let accept = match thread::Builder::new()
            .name("glide-remote-accept".to_string())
            .spawn(move || accept_loop(listener, handler, &accept_stop))
        {
            Ok(accept) => accept,
            Err(err) => {
                let _ = fs::remove_file(path);
                return Err(format!("remote control accept thread: {err}"));
            }
        };
        Ok(RemoteServer {
            path: path.to_path_buf(),
            stop,
            accept: std::sync::Mutex::new(Some(accept)),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Stop accepting, join the accept loop and remove the socket file. Idempotent.
    pub fn shutdown(&mut self) {
        use std::fs;
        use std::sync::atomic::Ordering;

        if self.stop.swap(true, Ordering::SeqCst) {
            return;
        }
        let accept = match self.accept.lock() {
            Ok(mut slot) => slot.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        if let Some(accept) = accept
            && accept.join().is_err()
        {
            crate::logging::warn(
                TARGET,
                "remote accept thread panicked",
                &[
                    ("status", "unavailable".to_string()),
                    ("hint", "restart Glide with --remote-control".to_string()),
                ],
            );
        }
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => crate::logging::warn(
                TARGET,
                "remote socket not removed",
                &[
                    ("status", "unavailable".to_string()),
                    ("detail", truncate_detail(&err.to_string())),
                    (
                        "hint",
                        "remove the file under the profile run directory".to_string(),
                    ),
                ],
            ),
        }
    }
}

#[cfg(unix)]
impl Drop for RemoteServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(unix)]
fn accept_loop(
    listener: std::os::unix::net::UnixListener,
    handler: Handler,
    stop: &std::sync::atomic::AtomicBool,
) {
    use std::sync::atomic::Ordering;
    use std::thread;

    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _address)) => {
                // BSD/macOS accepted sockets inherit the listener's non-blocking flag.
                if let Err(err) = stream.set_nonblocking(false) {
                    crate::logging::warn(
                        TARGET,
                        "remote connection setup failed",
                        &[
                            ("status", "unavailable".to_string()),
                            ("detail", truncate_detail(&err.to_string())),
                            ("hint", "restart Glide with --remote-control".to_string()),
                        ],
                    );
                    continue;
                }
                let handler = Arc::clone(&handler);
                if let Err(err) = thread::Builder::new()
                    .name("glide-remote-conn".to_string())
                    .spawn(move || serve_connection(stream, &handler))
                {
                    crate::logging::warn(
                        TARGET,
                        "remote connection thread failed",
                        &[
                            ("status", "unavailable".to_string()),
                            ("detail", truncate_detail(&err.to_string())),
                            ("hint", "restart Glide with --remote-control".to_string()),
                        ],
                    );
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL);
            }
            Err(err) => {
                crate::logging::warn(
                    TARGET,
                    "remote accept failed",
                    &[
                        ("status", "unavailable".to_string()),
                        ("detail", truncate_detail(&err.to_string())),
                        ("hint", "restart Glide with --remote-control".to_string()),
                    ],
                );
                return;
            }
        }
    }
}

#[cfg(unix)]
fn serve_connection(stream: std::os::unix::net::UnixStream, handler: &Handler) {
    crate::logging::debug(TARGET, "remote connection open", &[]);
    let status = match stream.try_clone() {
        Ok(writer) => {
            let reader = std::io::BufReader::new(stream);
            serve_lines(reader, writer, handler)
        }
        Err(_) => "unavailable",
    };
    crate::logging::debug(
        TARGET,
        "remote connection closed",
        &[("status", status.to_string())],
    );
}

#[cfg(unix)]
fn serve_lines(
    mut reader: std::io::BufReader<std::os::unix::net::UnixStream>,
    mut writer: std::os::unix::net::UnixStream,
    handler: &Handler,
) -> &'static str {
    use std::io::{BufRead, Read};

    loop {
        let mut line = Vec::new();
        let read = {
            let mut limited = reader.by_ref().take(MAX_LINE_BYTES as u64 + 1);
            limited.read_until(b'\n', &mut line)
        };
        match read {
            Ok(0) => return "eof",
            Ok(_) => {}
            Err(err) => {
                crate::logging::debug(
                    TARGET,
                    "remote read failed",
                    &[("detail", truncate_detail(&err.to_string()))],
                );
                return "read_error";
            }
        }
        let content_len = match line.strip_suffix(b"\n") {
            Some(content) => content.len(),
            None => line.len(),
        };
        if content_len > MAX_LINE_BYTES {
            log_rejected(
                "request_too_large",
                &format!("line is {content_len} bytes; limit is {MAX_LINE_BYTES}"),
                "send one request per line no longer than 1 MiB",
            );
            if write_reply(&mut writer, None, &Err("request too large".to_string())).is_err() {
                return "write_error";
            }
            drain_line(&mut reader);
            return "oversize";
        }
        let text = match std::str::from_utf8(&line) {
            Ok(text) => text,
            Err(_) => {
                log_rejected("bad_request", "line is not valid UTF-8", REQUEST_HINT);
                let reply = Err("bad request: line is not valid UTF-8".to_string());
                if write_reply(&mut writer, None, &reply).is_err() {
                    return "write_error";
                }
                continue;
            }
        };
        match parse_request_line(text) {
            Ok((id, request)) => {
                let reply = handler(request);
                if write_reply(&mut writer, Some(id), &reply).is_err() {
                    return "write_error";
                }
            }
            Err((id, message)) => {
                log_rejected("bad_request", &message, REQUEST_HINT);
                if write_reply(&mut writer, id, &Err(message)).is_err() {
                    return "write_error";
                }
            }
        }
    }
}

/// Consume the rest of an oversize line (bounded) so the peer's write can finish cleanly.
#[cfg(unix)]
fn drain_line(reader: &mut std::io::BufReader<std::os::unix::net::UnixStream>) {
    use std::io::Read;

    let mut scratch = [0u8; 8192];
    let mut drained = 0usize;
    while drained < MAX_LINE_BYTES {
        match reader.read(&mut scratch) {
            Ok(0) => return,
            Ok(read) => {
                drained += read;
                if scratch[..read].contains(&b'\n') {
                    return;
                }
            }
            Err(_) => return,
        }
    }
}

#[cfg(unix)]
fn write_reply(
    writer: &mut std::os::unix::net::UnixStream,
    id: Option<u64>,
    result: &Result<serde_json::Value, String>,
) -> std::io::Result<()> {
    use std::io::Write;

    writer.write_all(encode_response(id, result).as_bytes())
}

#[cfg(unix)]
fn log_rejected(status: &str, detail: &str, hint: &str) {
    crate::logging::warn(
        TARGET,
        "remote control rejected a request",
        &rejected_fields(status, detail, hint),
    );
}

/// Structured warn fields for a rejected request; the detail is truncated.
#[cfg(unix)]
fn rejected_fields(status: &str, detail: &str, hint: &str) -> [(&'static str, String); 3] {
    [
        ("status", status.to_string()),
        ("detail", truncate_detail(detail)),
        ("hint", hint.to_string()),
    ]
}

#[cfg(unix)]
fn truncate_detail(detail: &str) -> String {
    const LIMIT: usize = 160;

    let mut out: String = detail.chars().take(LIMIT).collect();
    if detail.chars().count() > LIMIT {
        out.push('…');
    }
    out
}

/// A client for one remote-control server connection.
#[cfg(unix)]
pub struct RemoteClient {
    writer: std::os::unix::net::UnixStream,
    reader: std::io::BufReader<std::os::unix::net::UnixStream>,
    next_id: u64,
}

#[cfg(unix)]
impl RemoteClient {
    pub fn connect(path: &Path) -> Result<RemoteClient, String> {
        let stream = std::os::unix::net::UnixStream::connect(path)
            .map_err(|err| format!("remote control connect {}: {err}", path.display()))?;
        let read_stream = stream
            .try_clone()
            .map_err(|err| format!("remote control connect {}: {err}", path.display()))?;
        Ok(RemoteClient {
            writer: stream,
            reader: std::io::BufReader::new(read_stream),
            next_id: 1,
        })
    }

    /// Send one request and read the response for its id, waiting at most `timeout`.
    pub fn call(
        &mut self,
        request: serde_json::Value,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value, String> {
        use std::io::{BufRead, Read, Write};

        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.writer
            .write_all(encode_request(id, &request).as_bytes())
            .map_err(|err| format!("remote control write failed: {err}"))?;
        self.writer
            .flush()
            .map_err(|err| format!("remote control write failed: {err}"))?;
        self.reader
            .get_ref()
            .set_read_timeout(Some(timeout))
            .map_err(|err| format!("remote control timeout: {err}"))?;
        let mut line = Vec::new();
        let read = {
            let mut limited = self.reader.by_ref().take(MAX_LINE_BYTES as u64 + 1);
            limited.read_until(b'\n', &mut line)
        };
        let _ = self.reader.get_ref().set_read_timeout(None);
        match read {
            Ok(0) => return Err("remote control connection closed".to_string()),
            Ok(_) => {}
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Err(format!("remote control did not answer within {timeout:?}"));
            }
            Err(err) => return Err(format!("remote control read failed: {err}")),
        }
        let content_len = match line.strip_suffix(b"\n") {
            Some(content) => content.len(),
            None => line.len(),
        };
        if content_len > MAX_LINE_BYTES {
            return Err("remote control response too large".to_string());
        }
        let text = std::str::from_utf8(&line)
            .map_err(|_| "remote control response is not valid UTF-8".to_string())?;
        let (response_id, outcome) = parse_response_line(text)?;
        match response_id {
            Some(answered) if answered == id => outcome,
            None => match outcome {
                Err(message) => Err(message),
                Ok(_) => Err("remote control answered without an id".to_string()),
            },
            Some(answered) => Err(format!(
                "remote control answered id {answered} for request {id}"
            )),
        }
    }
}

#[cfg(not(unix))]
const UNIX_ONLY: &str = "remote control needs a Unix platform";

#[cfg(not(unix))]
pub struct RemoteServer;

#[cfg(not(unix))]
impl RemoteServer {
    pub fn bind(_path: &Path, _handler: Handler) -> Result<RemoteServer, String> {
        Err(UNIX_ONLY.to_string())
    }

    pub fn path(&self) -> &Path {
        Path::new("")
    }

    pub fn shutdown(&mut self) {}
}

#[cfg(not(unix))]
impl Drop for RemoteServer {
    fn drop(&mut self) {}
}

#[cfg(not(unix))]
pub struct RemoteClient;

#[cfg(not(unix))]
impl RemoteClient {
    pub fn connect(_path: &Path) -> Result<RemoteClient, String> {
        Err(UNIX_ONLY.to_string())
    }

    pub fn call(
        &mut self,
        _request: serde_json::Value,
        _timeout: std::time::Duration,
    ) -> Result<serde_json::Value, String> {
        Err(UNIX_ONLY.to_string())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    /// Short, unique directory: macOS socket paths must stay under ~100 bytes.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let n = NEXT_DIR.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir().join(format!("glide-rs-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("temp dir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn echo_handler() -> Handler {
        Arc::new(Ok)
    }

    fn bind_echo(dir: &TempDir) -> RemoteServer {
        RemoteServer::bind(&socket_path(dir.path()), echo_handler()).expect("bind echo server")
    }

    fn raw_connect(dir: &TempDir) -> (UnixStream, BufReader<UnixStream>) {
        let stream = UnixStream::connect(socket_path(dir.path())).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read timeout");
        let reader = BufReader::new(stream.try_clone().expect("clone stream"));
        (stream, reader)
    }

    fn read_reply(reader: &mut BufReader<UnixStream>) -> String {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read reply");
        line
    }

    #[test]
    fn socket_path_is_under_run_dir() {
        assert_eq!(SOCKET_FILE, "remote-control.sock");
        assert_eq!(
            socket_path(Path::new("/tmp/profile")),
            PathBuf::from("/tmp/profile/run/remote-control.sock")
        );
    }

    #[test]
    fn request_and_response_lines_roundtrip() {
        let line = encode_request(7, &json!({"op": "ping", "count": 2}));
        assert!(
            line.starts_with("{\"v\":1,\"id\":7,\"request\":{"),
            "frame: {line}"
        );
        assert!(line.ends_with("}\n"), "newline-terminated frame: {line}");
        let (id, request) = parse_request_line(line.trim_end()).expect("parse request");
        assert_eq!(id, 7);
        assert_eq!(request, json!({"op": "ping", "count": 2}));

        let ok_line = encode_response(Some(7), &Ok(json!({"pong": true})));
        assert_eq!(ok_line, "{\"id\":7,\"ok\":{\"pong\":true}}\n");
        let (id, outcome) = parse_response_line(ok_line.trim_end()).expect("parse ok response");
        assert_eq!(id, Some(7));
        assert_eq!(outcome, Ok(json!({"pong": true})));

        let err_line = encode_response(Some(3), &Err("boom".to_string()));
        assert_eq!(err_line, "{\"id\":3,\"err\":\"boom\"}\n");
        assert_eq!(
            parse_response_line(err_line.trim_end()).expect("parse error response"),
            (Some(3), Err("boom".to_string()))
        );

        let unsolicited = encode_response(None, &Err("request too large".to_string()));
        assert_eq!(unsolicited, "{\"id\":null,\"err\":\"request too large\"}\n");
        let (id, outcome) = parse_response_line(unsolicited.trim_end()).expect("parse null id");
        assert_eq!(id, None);
        assert_eq!(outcome, Err("request too large".to_string()));

        assert!(parse_response_line("{not json}").is_err());
    }

    #[test]
    fn parse_request_rejects_bad_version() {
        assert_eq!(
            parse_request_line(r#"{"v":2,"id":9,"request":{"a":1}}"#),
            Err((Some(9), "unsupported protocol version".to_string()))
        );
        assert_eq!(
            parse_request_line(r#"{"id":9,"request":{"a":1}}"#),
            Err((Some(9), "bad request: missing v".to_string()))
        );
        let (id, message) = parse_request_line("{not json}").expect_err("malformed line");
        assert_eq!(id, None);
        assert!(message.starts_with("bad request: "), "message: {message}");

        let (id, message) = parse_request_line(r#"{"v":1,"id":"nope","request":null}"#)
            .expect_err("non-numeric id");
        assert_eq!(id, None);
        assert!(message.starts_with("bad request: "), "message: {message}");
    }

    #[test]
    fn roundtrip_with_echo_handler() {
        let dir = TempDir::new();
        let mut server = bind_echo(&dir);
        let mut client = RemoteClient::connect(&socket_path(dir.path())).expect("client");
        let reply = client
            .call(json!({"op": "ping", "n": 1}), Duration::from_secs(2))
            .expect("reply");
        assert_eq!(reply, json!({"op": "ping", "n": 1}));
        server.shutdown();
    }

    #[test]
    fn handler_error_becomes_err_response() {
        let dir = TempDir::new();
        let handler: Handler = Arc::new(|_| Err("handler exploded".to_string()));
        let mut server =
            RemoteServer::bind(&socket_path(dir.path()), handler).expect("bind error server");
        let mut client = RemoteClient::connect(&socket_path(dir.path())).expect("client");
        let error = client
            .call(json!(null), Duration::from_secs(2))
            .expect_err("handler error");
        assert_eq!(error, "handler exploded");
        server.shutdown();
    }

    #[test]
    fn socket_permissions_are_user_only() {
        let dir = TempDir::new();
        let mut server = bind_echo(&dir);
        let socket = socket_path(dir.path());
        let socket_mode = fs::metadata(&socket)
            .expect("socket metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(socket_mode, 0o600, "socket mode");
        let run = socket.parent().expect("run dir");
        let run_mode = fs::metadata(run)
            .expect("run metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(run_mode, 0o700, "run directory mode");
        server.shutdown();
    }

    #[test]
    fn existing_run_dir_is_tightened_to_0700() {
        let dir = TempDir::new();
        let run = dir.path().join("run");
        fs::create_dir_all(&run).expect("run dir");
        fs::set_permissions(&run, fs::Permissions::from_mode(0o755)).expect("widen run dir");
        let mut server = bind_echo(&dir);
        let mode = fs::metadata(&run)
            .expect("run metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
        server.shutdown();
    }

    #[test]
    fn live_socket_refuses_second_server() {
        let dir = TempDir::new();
        let path = socket_path(dir.path());
        let mut first = bind_echo(&dir);
        let error = RemoteServer::bind(&path, echo_handler()).expect_err("second bind");
        assert!(
            error.contains("remote control already active at"),
            "unexpected error: {error}"
        );
        assert!(
            error.contains(&path.display().to_string()),
            "path missing: {error}"
        );
        first.shutdown();
        let mut second = RemoteServer::bind(&path, echo_handler()).expect("bind after shutdown");
        second.shutdown();
    }

    #[test]
    fn stale_socket_is_replaced() {
        let dir = TempDir::new();
        let path = socket_path(dir.path());
        fs::create_dir_all(path.parent().expect("run dir")).expect("create run dir");
        let stale = UnixListener::bind(&path).expect("stale listener");
        drop(stale);
        assert!(path.exists(), "stale socket file should remain");
        let mut server = RemoteServer::bind(&path, echo_handler()).expect("bind over stale socket");
        assert!(path.exists());
        server.shutdown();
    }

    #[test]
    fn oversize_line_rejected() {
        let dir = TempDir::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let handler: Handler = Arc::new(move |request| {
            seen.fetch_add(1, Ordering::SeqCst);
            Ok(request)
        });
        let mut server =
            RemoteServer::bind(&socket_path(dir.path()), handler).expect("bind oversize server");
        let (mut stream, mut reader) = raw_connect(&dir);
        let mut huge = vec![b'a'; MAX_LINE_BYTES + 32];
        huge.push(b'\n');
        stream.write_all(&huge).expect("write oversize line");
        let reply = read_reply(&mut reader);
        assert_eq!(
            parse_response_line(reply.trim_end()).expect("parse oversize reply"),
            (None, Err("request too large".to_string()))
        );
        let mut tail = Vec::new();
        assert_eq!(
            reader.read(&mut tail).expect("read eof"),
            0,
            "server closes the connection after an oversize line"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0, "handler must not run");
        server.shutdown();
    }

    #[test]
    fn malformed_json_reports_error_and_keeps_connection() {
        let dir = TempDir::new();
        let mut server = bind_echo(&dir);
        let (mut stream, mut reader) = raw_connect(&dir);
        stream.write_all(b"this is not json\n").expect("write junk");
        let reply = read_reply(&mut reader);
        let (id, outcome) = parse_response_line(reply.trim_end()).expect("parse error reply");
        assert_eq!(id, None);
        let message = outcome.expect_err("error response");
        assert!(message.starts_with("bad request: "), "message: {message}");

        stream
            .write_all(encode_request(11, &json!({"still": "alive"})).as_bytes())
            .expect("write follow-up");
        let reply = read_reply(&mut reader);
        let (id, outcome) = parse_response_line(reply.trim_end()).expect("parse follow-up");
        assert_eq!(id, Some(11));
        assert_eq!(outcome.expect("ok response"), json!({"still": "alive"}));
        server.shutdown();
    }

    #[test]
    fn socket_removed_on_shutdown_and_drop() {
        let dir = TempDir::new();
        let path = socket_path(dir.path());
        let mut server = RemoteServer::bind(&path, echo_handler()).expect("bind");
        assert!(path.exists(), "socket exists while live");
        server.shutdown();
        assert!(!path.exists(), "shutdown removes the socket");
        server.shutdown();
        assert!(!path.exists(), "shutdown is idempotent");

        let dropped = RemoteServer::bind(&path, echo_handler()).expect("bind again");
        assert!(path.exists(), "socket exists while live");
        drop(dropped);
        assert!(!path.exists(), "drop removes the socket");
    }

    #[test]
    fn rejected_request_record_is_structured_and_truncated() {
        let long_detail = "x".repeat(1000);
        let fields = rejected_fields("bad_request", &long_detail, REQUEST_HINT);
        let record = crate::logging::format_record(
            crate::logging::Level::Warn,
            TARGET,
            "remote control rejected a request",
            &fields,
            crate::logging::Level::Info,
        )
        .expect("warn record");
        assert!(
            record.contains("target=\"remote.socket\""),
            "record: {record}"
        );
        assert!(
            record.contains("status=\"bad_request\""),
            "record: {record}"
        );
        assert!(record.contains("hint="), "record: {record}");
        assert!(
            fields[1].1.chars().count() <= 161,
            "detail is truncated: {}",
            fields[1].1.len()
        );
        assert!(fields[1].1.ends_with('…'), "detail marks truncation");
    }

    #[test]
    fn client_call_times_out_when_handler_is_slow() {
        let dir = TempDir::new();
        let handler: Handler = Arc::new(|request| {
            std::thread::sleep(Duration::from_millis(400));
            Ok(request)
        });
        let mut server =
            RemoteServer::bind(&socket_path(dir.path()), handler).expect("bind slow server");
        let mut client = RemoteClient::connect(&socket_path(dir.path())).expect("client");
        let error = client
            .call(json!(null), Duration::from_millis(100))
            .expect_err("timeout");
        assert!(
            error.contains("did not answer within"),
            "unexpected error: {error}"
        );
        server.shutdown();
    }

    #[test]
    fn requests_on_one_connection_answer_in_order() {
        let dir = TempDir::new();
        let mut server = bind_echo(&dir);
        let (mut stream, mut reader) = raw_connect(&dir);
        for (id, value) in [(1u64, 10), (2, 20), (3, 30)] {
            stream
                .write_all(encode_request(id, &json!({"n": value})).as_bytes())
                .expect("write request");
        }
        for (id, value) in [(1u64, 10), (2, 20), (3, 30)] {
            let reply = read_reply(&mut reader);
            let (reply_id, outcome) = parse_response_line(reply.trim_end()).expect("parse reply");
            assert_eq!(reply_id, Some(id));
            assert_eq!(outcome.expect("ok response"), json!({"n": value}));
        }
        server.shutdown();
    }
}
