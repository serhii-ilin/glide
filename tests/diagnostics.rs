//! End-to-end checks for structured diagnostics emitted by the headless binary.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn uncaught_script_error_is_logged_with_context() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/js-throw.html")
        .canonicalize()
        .expect("fixture exists");
    let url = format!("file://{}", path.display());

    let output = Command::new(env!("CARGO_BIN_EXE_glide"))
        .args(["--dump-text", &url])
        .env("GLIDE_LOG", "error")
        .output()
        .expect("run glide");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("level=ERROR target=\"js.uncaught\""),
        "{stderr}"
    );
    assert!(stderr.contains("msg=\"Uncaught"), "{stderr}");
    assert!(stderr.contains("__glideMissingDiagnosticProbe"), "{stderr}");
    assert!(stderr.contains("page_url=\""), "{stderr}");
    assert!(stderr.contains("script=\"script-0\""), "{stderr}");
    assert!(stderr.contains("error_kind=\"missing_api\""), "{stderr}");
    assert!(stderr.contains("hint=\""), "{stderr}");
}

#[test]
fn glide_log_warn_suppresses_informational_console_messages() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/web-api-shims.html")
        .canonicalize()
        .expect("fixture exists");
    let url = format!("file://{}", path.display());

    let output = Command::new(env!("CARGO_BIN_EXE_glide"))
        .args(["--dump-text", &url])
        .env("GLIDE_LOG", "warn")
        .output()
        .expect("run glide");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!stderr.contains("level=INFO"), "{stderr}");
    assert!(!stderr.contains("level=DEBUG"), "{stderr}");
    assert!(!stderr.contains("level=ERROR"), "{stderr}");
}
