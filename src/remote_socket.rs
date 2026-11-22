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

/// Longest accepted response line; responses (for example screenshots) may exceed a request.
pub const MAX_RESPONSE_BYTES: usize = 64 << 20;

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
const BROKEN: &str = "remote control connection is broken; reconnect";
#[cfg(unix)]
const MAX_CONNECTIONS: usize = 16;
#[cfg(unix)]
const MAX_REJECTION_LOGS: usize = 3;
#[cfg(unix)]
const CONNECTION_WRITE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
#[cfg(unix)]
const ACCEPT_POLL: std::time::Duration = std::time::Duration::from_millis(100);
#[cfg(unix)]
const ACCEPT_BACKOFF: std::time::Duration = std::time::Duration::from_millis(50);

#[cfg(all(
    unix,
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )
))]
const SUN_PATH_MAX: usize = 104;
#[cfg(all(
    unix,
    not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))
))]
const SUN_PATH_MAX: usize = 108;

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AcceptAction {
    /// Transient interruption: retry immediately without logging.
    Retry,
    /// Resource or unexpected error: log once, back off, keep serving.
    BackOff,
}

#[cfg(unix)]
fn accept_error_action(error: &std::io::Error) -> AcceptAction {
    match error.kind() {
        std::io::ErrorKind::Interrupted | std::io::ErrorKind::ConnectionAborted => {
            AcceptAction::Retry
        }
        _ => AcceptAction::BackOff,
    }
}

/// A live listener serving each connection on its own thread.
#[cfg(unix)]
#[derive(Debug)]
pub struct RemoteServer {
    path: PathBuf,
    stop: Arc<std::sync::atomic::AtomicBool>,
    accept: Option<std::thread::JoinHandle<()>>,
    connections: Arc<std::sync::Mutex<Vec<(u64, std::os::unix::net::UnixStream)>>>,
    socket_id: (u64, u64),
}

#[cfg(unix)]
impl RemoteServer {
    /// Bind the user-only socket at `path`, replacing a stale socket file.
    ///
    /// The parent directory is created if needed and set to mode 0700; it must be a real
    /// directory owned by the current user. The socket is chmodded to 0600. A live server at
    /// the same path makes this fail instead of hijacking it, and a non-socket file at `path`
    /// is never removed.
    pub fn bind(path: &Path, handler: Handler) -> Result<RemoteServer, String> {
        use std::fs;
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        use std::os::unix::net::UnixListener;
        use std::sync::atomic::AtomicBool;

        check_socket_path_length(path)?;
        let parent = path.parent().ok_or_else(|| {
            format!(
                "remote control socket path has no parent: {}",
                path.display()
            )
        })?;
        prepare_run_dir(parent)?;
        if stale_socket_file(path)? {
            fs::remove_file(path)
                .map_err(|err| format!("remote control remove stale {}: {err}", path.display()))?;
        }
        let listener = UnixListener::bind(path)
            .map_err(|err| format!("remote control bind {}: {err}", path.display()))?;
        let cleanup = |err: std::io::Error, action: &str| {
            let _ = fs::remove_file(path);
            format!("remote control {action} {}: {err}", path.display())
        };
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|err| cleanup(err, "chmod"))?;
        listener
            .set_nonblocking(true)
            .map_err(|err| cleanup(err, "listen"))?;
        let metadata = fs::symlink_metadata(path)
            .map_err(|err| format!("remote control inspect {}: {err}", path.display()))?;
        let stop = Arc::new(AtomicBool::new(false));
        let connections = Arc::new(std::sync::Mutex::new(Vec::new()));
        let accept = std::thread::Builder::new()
            .name("glide-remote-accept".to_string())
            .spawn({
                let stop = Arc::clone(&stop);
                let connections = Arc::clone(&connections);
                move || accept_loop(listener, handler, stop, connections)
            })
            .map_err(|err| cleanup(err, "accept thread"))?;
        Ok(RemoteServer {
            path: path.to_path_buf(),
            stop,
            accept: Some(accept),
            connections,
            socket_id: (metadata.dev(), metadata.ino()),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Stop accepting, close open connections and remove this server's socket file.
    /// Idempotent.
    pub fn shutdown(&mut self) {
        use std::sync::atomic::Ordering;

        if self.stop.swap(true, Ordering::SeqCst) {
            return;
        }
        close_connections(&self.connections);
        if let Some(accept) = self.accept.take()
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
        // A connection accepted just before the stop flag can register late; close it too.
        close_connections(&self.connections);
        self.remove_socket_file();
    }

    /// Remove the socket only if it is still the file this server bound.
    fn remove_socket_file(&self) {
        use std::fs;
        use std::os::unix::fs::MetadataExt;

        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
            Err(err) => {
                log_socket_removal_failure(&err);
                return;
            }
        };
        if (metadata.dev(), metadata.ino()) != self.socket_id {
            crate::logging::debug(
                TARGET,
                "remote socket replaced; not removing",
                &[("detail", truncate_detail(&self.path.display().to_string()))],
            );
            return;
        }
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => log_socket_removal_failure(&err),
        }
    }
}

#[cfg(unix)]
fn log_socket_removal_failure(err: &std::io::Error) {
    crate::logging::warn(
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
    );
}

#[cfg(unix)]
impl Drop for RemoteServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(unix)]
fn lock_connections(
    connections: &std::sync::Mutex<Vec<(u64, std::os::unix::net::UnixStream)>>,
) -> std::sync::MutexGuard<'_, Vec<(u64, std::os::unix::net::UnixStream)>> {
    match connections.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(unix)]
fn close_connections(connections: &std::sync::Mutex<Vec<(u64, std::os::unix::net::UnixStream)>>) {
    use std::net::Shutdown;

    let mut live = lock_connections(connections);
    for (_, stream) in live.iter() {
        let _ = stream.shutdown(Shutdown::Both);
    }
    live.clear();
}

#[cfg(unix)]
fn accept_loop(
    listener: std::os::unix::net::UnixListener,
    handler: Handler,
    stop: Arc<std::sync::atomic::AtomicBool>,
    connections: Arc<std::sync::Mutex<Vec<(u64, std::os::unix::net::UnixStream)>>>,
) {
    use std::sync::atomic::Ordering;
    use std::thread;

    let mut next_connection = 1u64;
    let mut logged_error = false;
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _address)) => {
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                let mut stream = stream;
                // BSD/macOS accepted sockets inherit the listener's non-blocking flag.
                if let Err(err) = stream.set_nonblocking(false) {
                    log_connection_setup_failure(&err);
                    continue;
                }
                let _ = stream.set_write_timeout(Some(CONNECTION_WRITE_TIMEOUT));
                if lock_connections(&connections).len() >= MAX_CONNECTIONS {
                    let _ =
                        write_reply(&mut stream, None, &Err("too many connections".to_string()));
                    continue;
                }
                let copy = match stream.try_clone() {
                    Ok(copy) => copy,
                    Err(err) => {
                        log_connection_setup_failure(&err);
                        continue;
                    }
                };
                let id = next_connection;
                next_connection = next_connection.wrapping_add(1);
                lock_connections(&connections).push((id, copy));
                let handler = Arc::clone(&handler);
                let registry = Arc::clone(&connections);
                let stop_flag = Arc::clone(&stop);
                let spawned = thread::Builder::new()
                    .name("glide-remote-conn".to_string())
                    .spawn(move || {
                        serve_connection(stream, &handler, &stop_flag);
                        lock_connections(&registry).retain(|(entry, _)| *entry != id);
                    });
                if let Err(err) = spawned {
                    lock_connections(&connections).retain(|(entry, _)| *entry != id);
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
            Err(err) => match accept_error_action(&err) {
                AcceptAction::Retry => {}
                AcceptAction::BackOff => {
                    if !logged_error {
                        logged_error = true;
                        crate::logging::warn(
                            TARGET,
                            "remote accept failed; backing off",
                            &[
                                ("error_kind", "accept_failed".to_string()),
                                ("detail", truncate_detail(&err.to_string())),
                                (
                                    "hint",
                                    "the listener may be out of file descriptors; free resources or restart Glide"
                                        .to_string(),
                                ),
                            ],
                        );
                    }
                    thread::sleep(ACCEPT_BACKOFF);
                }
            },
        }
    }
}

#[cfg(unix)]
fn log_connection_setup_failure(err: &std::io::Error) {
    crate::logging::warn(
        TARGET,
        "remote connection setup failed",
        &[
            ("status", "unavailable".to_string()),
            ("detail", truncate_detail(&err.to_string())),
            ("hint", "restart Glide with --remote-control".to_string()),
        ],
    );
}

#[cfg(unix)]
fn serve_connection(
    stream: std::os::unix::net::UnixStream,
    handler: &Handler,
    stop: &std::sync::atomic::AtomicBool,
) {
    crate::logging::debug(TARGET, "remote connection open", &[]);
    let status = match stream.try_clone() {
        Ok(writer) => {
            let reader = std::io::BufReader::new(stream);
            serve_lines(reader, writer, handler, stop)
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
    stop: &std::sync::atomic::AtomicBool,
) -> &'static str {
    use std::io::{BufRead, Read};
    use std::net::Shutdown;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::atomic::Ordering;

    let mut rejected = 0usize;
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
            log_rejection(
                &mut rejected,
                "request_too_large",
                &format!("line is {content_len} bytes; limit is {MAX_LINE_BYTES}"),
                "send one request per line no longer than 1 MiB",
            );
            if write_reply(&mut writer, None, &Err("request too large".to_string())).is_err() {
                return "write_error";
            }
            let _ = writer.shutdown(Shutdown::Write);
            return "oversize";
        }
        let text = match std::str::from_utf8(&line) {
            Ok(text) => text,
            Err(_) => {
                log_rejection(
                    &mut rejected,
                    "bad_request",
                    "line is not valid UTF-8",
                    REQUEST_HINT,
                );
                let reply = Err("bad request: line is not valid UTF-8".to_string());
                if write_reply(&mut writer, None, &reply).is_err() {
                    return "write_error";
                }
                continue;
            }
        };
        match parse_request_line(text) {
            Ok((id, request)) => {
                if stop.load(Ordering::SeqCst) {
                    return "stopped";
                }
                let reply = match catch_unwind(AssertUnwindSafe(|| handler(request))) {
                    Ok(reply) => reply,
                    Err(_) => {
                        crate::logging::error(
                            TARGET,
                            "remote handler panicked",
                            &[
                                ("error_kind", "handler_panic".to_string()),
                                (
                                    "hint",
                                    "the handler must not panic; the connection stays open"
                                        .to_string(),
                                ),
                            ],
                        );
                        Err("internal error".to_string())
                    }
                };
                if write_reply(&mut writer, Some(id), &reply).is_err() {
                    return "write_error";
                }
            }
            Err((id, message)) => {
                log_rejection(&mut rejected, "bad_request", &message, REQUEST_HINT);
                if write_reply(&mut writer, id, &Err(message)).is_err() {
                    return "write_error";
                }
            }
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
fn log_rejection(rejected: &mut usize, error_kind: &str, detail: &str, hint: &str) {
    if *rejected < MAX_REJECTION_LOGS {
        crate::logging::warn(
            TARGET,
            "remote control rejected a request",
            &rejected_fields(error_kind, detail, hint),
        );
    } else if *rejected == MAX_REJECTION_LOGS {
        crate::logging::warn(
            TARGET,
            "further remote rejections suppressed",
            &[
                ("error_kind", "suppressed".to_string()),
                (
                    "hint",
                    "fix the client framing; later rejections on this connection are not logged"
                        .to_string(),
                ),
            ],
        );
    }
    *rejected = rejected.saturating_add(1);
}

/// Structured warn fields for a rejected request; the detail is truncated.
#[cfg(unix)]
fn rejected_fields(error_kind: &str, detail: &str, hint: &str) -> [(&'static str, String); 3] {
    [
        ("error_kind", error_kind.to_string()),
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

/// Reject a path that cannot fit in the platform's `sun_path`.
#[cfg(unix)]
fn check_socket_path_length(path: &Path) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt;

    let len = path.as_os_str().as_bytes().len();
    if len >= SUN_PATH_MAX {
        return Err(format!(
            "remote control socket path is too long: {len} bytes; this platform allows \
             {SUN_PATH_MAX} bytes including the terminating NUL; use a shorter profile path"
        ));
    }
    Ok(())
}

/// Create the run directory, require that it is a real directory owned by this user and
/// tighten it to mode 0700.
#[cfg(unix)]
fn prepare_run_dir(parent: &Path) -> Result<(), String> {
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    fs::create_dir_all(parent)
        .map_err(|err| format!("remote control create {}: {err}", parent.display()))?;
    let metadata = fs::symlink_metadata(parent)
        .map_err(|err| format!("remote control inspect {}: {err}", parent.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "remote control run directory {} must be a real directory, not a symlink",
            parent.display()
        ));
    }
    // A file created here is owned by our effective uid; use it instead of libc.
    let probe = parent.join(format!(
        ".remote-control-probe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default()
    ));
    let probe_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|err| format!("remote control cannot write {}: {err}", parent.display()))?;
    let owner = match probe_file.metadata() {
        Ok(metadata) => metadata.uid(),
        Err(err) => {
            drop(probe_file);
            let _ = fs::remove_file(&probe);
            return Err(format!(
                "remote control inspect {}: {err}",
                parent.display()
            ));
        }
    };
    drop(probe_file);
    let _ = fs::remove_file(&probe);
    if metadata.uid() != owner {
        return Err(format!(
            "remote control run directory {} is not owned by the current user",
            parent.display()
        ));
    }
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
        .map_err(|err| format!("remote control chmod {}: {err}", parent.display()))
}

/// Check that `path` is absent, a stale socket (returns `true`) or a live server (error).
#[cfg(unix)]
fn stale_socket_file(path: &Path) -> Result<bool, String> {
    use std::fs;
    use std::io::ErrorKind;
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::net::UnixStream;

    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_socket() {
                return Err(format!("{} exists and is not a socket", path.display()));
            }
            match UnixStream::connect(path) {
                Ok(_) => Err(format!(
                    "remote control already active at {}",
                    path.display()
                )),
                Err(err)
                    if matches!(
                        err.kind(),
                        ErrorKind::ConnectionRefused | ErrorKind::NotFound
                    ) =>
                {
                    Ok(true)
                }
                Err(err) => Err(format!(
                    "remote control cannot probe {}: {err}",
                    path.display()
                )),
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(format!("remote control inspect {}: {err}", path.display())),
    }
}

/// A client for one remote-control server connection.
#[cfg(unix)]
#[derive(Debug)]
pub struct RemoteClient {
    writer: std::os::unix::net::UnixStream,
    reader: std::os::unix::net::UnixStream,
    buffer: Vec<u8>,
    next_id: u64,
    response_limit: usize,
    poisoned: bool,
}

#[cfg(unix)]
impl RemoteClient {
    pub fn connect(path: &Path) -> Result<RemoteClient, String> {
        RemoteClient::open(path, MAX_RESPONSE_BYTES)
    }

    #[cfg(test)]
    fn connect_with_response_limit(
        path: &Path,
        response_limit: usize,
    ) -> Result<RemoteClient, String> {
        RemoteClient::open(path, response_limit)
    }

    fn open(path: &Path, response_limit: usize) -> Result<RemoteClient, String> {
        check_socket_path_length(path)?;
        let stream = std::os::unix::net::UnixStream::connect(path)
            .map_err(|err| format!("remote control connect {}: {err}", path.display()))?;
        let read_stream = stream
            .try_clone()
            .map_err(|err| format!("remote control connect {}: {err}", path.display()))?;
        Ok(RemoteClient {
            writer: stream,
            reader: read_stream,
            buffer: Vec::new(),
            next_id: 1,
            response_limit,
            poisoned: false,
        })
    }

    /// Send one request and read the response for its id, waiting at most `timeout` overall.
    ///
    /// Responses for earlier (timed-out) requests are discarded, so one connection stays in
    /// sync. An unreadable or oversize response marks the connection broken.
    pub fn call(
        &mut self,
        request: serde_json::Value,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value, String> {
        use std::io::{Read, Write};
        use std::time::Instant;

        if self.poisoned {
            return Err(BROKEN.to_string());
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| "remote control timeout is too large".to_string())?;
        self.writer
            .write_all(encode_request(id, &request).as_bytes())
            .map_err(|err| format!("remote control write failed: {err}"))?;
        self.writer
            .flush()
            .map_err(|err| format!("remote control write failed: {err}"))?;
        loop {
            if let Some(line) = take_response_line(&mut self.buffer) {
                if line.len() > self.response_limit {
                    self.poisoned = true;
                    return Err(BROKEN.to_string());
                }
                let text = std::str::from_utf8(&line)
                    .map_err(|_| "remote control response is not valid UTF-8".to_string())?;
                let (response_id, outcome) = parse_response_line(text)?;
                match response_id {
                    Some(answered) if answered < id => continue,
                    Some(answered) if answered == id => return outcome,
                    None => {
                        return match outcome {
                            Err(message) => Err(message),
                            Ok(_) => Err("remote control answered without an id".to_string()),
                        };
                    }
                    Some(answered) => {
                        return Err(format!(
                            "remote control answered id {answered} for request {id}"
                        ));
                    }
                }
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(timeout_message(timeout));
            }
            self.reader
                .set_read_timeout(Some(remaining))
                .map_err(|err| format!("remote control timeout: {err}"))?;
            let mut chunk = [0u8; 64 * 1024];
            match self.reader.read(&mut chunk) {
                Ok(0) => return Err("remote control connection closed".to_string()),
                Ok(read) => {
                    self.buffer.extend_from_slice(&chunk[..read]);
                    if !self.buffer.contains(&b'\n') && self.buffer.len() > self.response_limit {
                        self.poisoned = true;
                        return Err(BROKEN.to_string());
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
                Err(err)
                    if matches!(
                        err.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Err(timeout_message(timeout));
                }
                Err(err) => return Err(format!("remote control read failed: {err}")),
            }
        }
    }
}

#[cfg(unix)]
fn timeout_message(timeout: std::time::Duration) -> String {
    format!("remote control did not answer within {timeout:?}")
}

/// Take one complete newline-terminated line out of the persistent read buffer.
#[cfg(unix)]
fn take_response_line(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let end = buffer.iter().position(|byte| *byte == b'\n')?;
    let mut line: Vec<u8> = buffer.drain(..=end).collect();
    line.pop();
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Some(line)
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
    use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

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
        let mut client = RemoteClient::connect(&path).expect("client on replaced socket");
        let reply = client
            .call(json!({"fresh": true}), Duration::from_secs(2))
            .expect("reply from new server");
        assert_eq!(reply, json!({"fresh": true}));
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
        // The server may close after answering, so a late write can fail; the reply is queued.
        let mut written = 0;
        while written < huge.len() {
            match stream.write(&huge[written..]) {
                Ok(0) => break,
                Ok(count) => written += count,
                Err(_) => break,
            }
        }
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
            record.contains("error_kind=\"bad_request\""),
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

    #[test]
    fn client_stays_in_sync_after_timeout() {
        let dir = TempDir::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let handler: Handler = Arc::new(move |request| {
            let call = seen.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                thread::sleep(Duration::from_millis(400));
            }
            Ok(request)
        });
        let mut server =
            RemoteServer::bind(&socket_path(dir.path()), handler).expect("bind slow-then-fast");
        let mut client = RemoteClient::connect(&socket_path(dir.path())).expect("client");
        let error = client
            .call(json!({"n": 1}), Duration::from_millis(100))
            .expect_err("first call times out");
        assert!(
            error.contains("did not answer within"),
            "unexpected error: {error}"
        );
        let reply = client
            .call(json!({"n": 2}), Duration::from_secs(3))
            .expect("second call gets its own answer");
        assert_eq!(reply, json!({"n": 2}));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        server.shutdown();
    }

    #[test]
    fn call_deadline_holds_against_trickling_server() {
        let dir = TempDir::new();
        let path = socket_path(dir.path());
        fs::create_dir_all(path.parent().expect("run dir")).expect("create run dir");
        let listener = UnixListener::bind(&path).expect("trickle listener");
        let trickle = thread::spawn(move || {
            let Ok((mut stream, _address)) = listener.accept() else {
                return;
            };
            for _ in 0..12 {
                if stream.write_all(b" ").is_err() {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
            let _ = stream.write_all(b"\n");
        });
        let mut client = RemoteClient::connect(&path).expect("client");
        let started = Instant::now();
        let error = client
            .call(json!(null), Duration::from_millis(250))
            .expect_err("deadline expires");
        let elapsed = started.elapsed();
        assert!(
            error.contains("did not answer within"),
            "unexpected error: {error}"
        );
        assert!(
            elapsed < Duration::from_millis(800),
            "the whole call must respect one deadline, took {elapsed:?}"
        );
        drop(client);
        trickle.join().expect("trickle thread");
    }

    #[test]
    fn large_response_over_one_mib_roundtrips() {
        let dir = TempDir::new();
        let payload = "x".repeat(3 << 20);
        let handler: Handler = Arc::new(move |_| Ok(json!({"blob": payload.as_str()})));
        let mut server = RemoteServer::bind(&socket_path(dir.path()), handler).expect("bind large");
        let mut client = RemoteClient::connect(&socket_path(dir.path())).expect("client");
        let reply = client
            .call(json!(null), Duration::from_secs(10))
            .expect("large reply");
        let blob = reply["blob"].as_str().expect("blob string");
        assert_eq!(blob.len(), 3 << 20);
        server.shutdown();
    }

    #[test]
    fn oversize_response_poisons_client() {
        let dir = TempDir::new();
        let handler: Handler = Arc::new(|_| Ok(json!("y".repeat(4096))));
        let mut server = RemoteServer::bind(&socket_path(dir.path()), handler).expect("bind");
        let mut client = RemoteClient::connect_with_response_limit(&socket_path(dir.path()), 1024)
            .expect("client");
        let error = client
            .call(json!(null), Duration::from_secs(2))
            .expect_err("oversize reply");
        assert_eq!(error, "remote control connection is broken; reconnect");
        let error = client
            .call(json!(null), Duration::from_secs(2))
            .expect_err("stays poisoned");
        assert_eq!(error, "remote control connection is broken; reconnect");
        server.shutdown();
    }

    #[test]
    fn accept_error_action_classifies_transient_and_resource_errors() {
        use super::AcceptAction;

        assert_eq!(
            accept_error_action(&std::io::Error::from(ErrorKind::Interrupted)),
            AcceptAction::Retry
        );
        assert_eq!(
            accept_error_action(&std::io::Error::from(ErrorKind::ConnectionAborted)),
            AcceptAction::Retry
        );
        assert_eq!(
            accept_error_action(&std::io::Error::from(ErrorKind::WouldBlock)),
            AcceptAction::BackOff
        );
        assert_eq!(
            accept_error_action(&std::io::Error::from(ErrorKind::Other)),
            AcceptAction::BackOff
        );
    }

    #[test]
    fn shutdown_closes_open_connections() {
        let dir = TempDir::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let handler: Handler = Arc::new(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
            Ok(serde_json::Value::Null)
        });
        let mut server =
            RemoteServer::bind(&socket_path(dir.path()), handler).expect("bind guarded server");
        let mut client = RemoteClient::connect(&socket_path(dir.path())).expect("client");
        thread::sleep(Duration::from_millis(50));
        server.shutdown();
        let error = client
            .call(json!(null), Duration::from_secs(1))
            .expect_err("connection closed by shutdown");
        assert!(!error.is_empty(), "an error explains the closed connection");
        assert_eq!(calls.load(Ordering::SeqCst), 0, "handler must not run");
    }

    #[test]
    fn shutdown_does_not_remove_replaced_socket() {
        let dir = TempDir::new();
        let path = socket_path(dir.path());
        let mut server = RemoteServer::bind(&path, echo_handler()).expect("bind");
        fs::remove_file(&path).expect("unlink live socket");
        fs::write(&path, b"replacement").expect("replace with a regular file");
        server.shutdown();
        assert_eq!(
            fs::read(&path).expect("replacement survives"),
            b"replacement"
        );
    }

    #[test]
    fn regular_file_at_socket_path_is_not_removed() {
        let dir = TempDir::new();
        let path = socket_path(dir.path());
        fs::create_dir_all(path.parent().expect("run dir")).expect("create run dir");
        fs::write(&path, b"keep me").expect("regular file at socket path");
        let error = RemoteServer::bind(&path, echo_handler()).expect_err("regular file");
        assert!(
            error.contains("exists and is not a socket"),
            "unexpected error: {error}"
        );
        assert_eq!(fs::read(&path).expect("file survives"), b"keep me");
    }

    #[test]
    fn symlink_at_socket_path_is_not_followed_or_removed() {
        let dir = TempDir::new();
        let path = socket_path(dir.path());
        fs::create_dir_all(path.parent().expect("run dir")).expect("create run dir");
        let target = dir.path().join("target-file");
        fs::write(&target, b"target").expect("target file");
        std::os::unix::fs::symlink(&target, &path).expect("symlink at socket path");
        let error = RemoteServer::bind(&path, echo_handler()).expect_err("symlink");
        assert!(
            error.contains("exists and is not a socket"),
            "unexpected error: {error}"
        );
        assert!(
            fs::symlink_metadata(&path)
                .expect("symlink metadata")
                .file_type()
                .is_symlink(),
            "symlink survives"
        );
        assert_eq!(fs::read(&target).expect("target survives"), b"target");
    }

    #[test]
    fn symlinked_run_dir_is_refused() {
        let dir = TempDir::new();
        let real_run = dir.path().join("real-run");
        fs::create_dir_all(&real_run).expect("real run dir");
        fs::set_permissions(&real_run, fs::Permissions::from_mode(0o755)).expect("widen real run");
        let run = dir.path().join("run");
        std::os::unix::fs::symlink(&real_run, &run).expect("symlink run dir");
        let error = RemoteServer::bind(&socket_path(dir.path()), echo_handler())
            .expect_err("symlinked run dir");
        assert!(error.contains("symlink"), "unexpected error: {error}");
        assert!(
            !real_run.join(SOCKET_FILE).exists(),
            "no socket is created through the symlink"
        );
        let mode = fs::metadata(&real_run)
            .expect("real run metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o755, "the symlink target is not tightened");
    }

    #[test]
    fn overlong_socket_path_has_clear_error() {
        let dir = TempDir::new();
        let path = dir.path().join("x".repeat(SUN_PATH_MAX));
        let error = RemoteServer::bind(&path, echo_handler()).expect_err("overlong path");
        assert!(error.contains("too long"), "unexpected error: {error}");
        assert!(error.contains("shorter"), "hint missing: {error}");
        let error = RemoteClient::connect(&path).expect_err("overlong client path");
        assert!(error.contains("too long"), "unexpected error: {error}");
    }

    #[test]
    fn handler_panic_returns_error_response() {
        let dir = TempDir::new();
        let handler: Handler = Arc::new(|request| {
            if request == json!("boom") {
                panic!("handler panicked on purpose");
            }
            Ok(request)
        });
        let mut server = RemoteServer::bind(&socket_path(dir.path()), handler).expect("bind");
        let mut client = RemoteClient::connect(&socket_path(dir.path())).expect("client");
        let error = client
            .call(json!("boom"), Duration::from_secs(2))
            .expect_err("panic becomes an error response");
        assert_eq!(error, "internal error");
        let reply = client
            .call(json!({"after": true}), Duration::from_secs(2))
            .expect("connection survives a handler panic");
        assert_eq!(reply, json!({"after": true}));
        server.shutdown();
    }

    #[test]
    fn too_many_connections_are_refused() {
        let dir = TempDir::new();
        let mut server = bind_echo(&dir);
        let mut held = Vec::with_capacity(MAX_CONNECTIONS);
        for _ in 0..MAX_CONNECTIONS {
            held.push(UnixStream::connect(socket_path(dir.path())).expect("held connection"));
        }
        let extra = UnixStream::connect(socket_path(dir.path())).expect("extra connection");
        extra
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read timeout");
        let mut reader = BufReader::new(extra.try_clone().expect("clone extra"));
        let reply = read_reply(&mut reader);
        assert_eq!(
            parse_response_line(reply.trim_end()).expect("parse refusal"),
            (None, Err("too many connections".to_string()))
        );
        drop(held);
        server.shutdown();
    }
}
