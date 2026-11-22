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

fn run_fixture(name: &str, log: &str) -> (String, String) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .canonicalize()
        .expect("fixture exists");
    let url = format!("file://{}", path.display());
    let output = Command::new(env!("CARGO_BIN_EXE_glide"))
        .args(["--dump-text", &url])
        .env("GLIDE_LOG", log)
        .output()
        .expect("run glide");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn dynamically_inserted_script_runs() {
    let (stdout, stderr) = run_fixture("dynamic-script.html", "debug");

    assert!(
        stderr.contains("script_kind=\"dynamic\"") && stderr.contains("msg=\"script executed\""),
        "expected a record for the executed dynamic script: {stderr}"
    );
    assert!(
        !stderr.contains("feature=\"dynamic-script\""),
        "a runnable dynamic script is not an unsupported feature: {stderr}"
    );
    assert!(
        stderr.contains("msg=\"page scripts settled\"") && stderr.contains("status=\"clean\""),
        "expected a clean per-page summary: {stderr}"
    );
    assert!(stdout.contains("dynamic"), "dump: {stdout}");
}

#[test]
fn dynamic_script_fetch_failure_is_reported() {
    let (stdout, stderr) = run_fixture("dynamic-script-missing.html", "debug");

    assert!(
        stderr.contains("target=\"js.unsupported\"")
            && stderr.contains("feature=\"dynamic-script\"")
            // The detail names the URL and then the fetch failure reason, so a
            // transport error is distinguishable from an unimplemented feature.
            && stderr.contains("detail=\"missing-script.js ("),
        "expected an unsupported record with the fetch reason: {stderr}"
    );
    assert!(
        stderr.contains("msg=\"page scripts settled\"")
            && stderr.contains("unsupported=\"1\"")
            && stderr.contains("status=\"degraded\""),
        "expected the per-page summary with the unsupported count: {stderr}"
    );
    // The element's error handler still runs, like a browser's script error.
    assert!(stdout.contains("error fired"), "dump: {stdout}");
}

#[test]
fn blank_script_shell_is_reported_after_load() {
    let (stdout, stderr) = run_fixture("blank-shell.html", "warn");

    assert!(
        stderr.contains("target=\"js.page\"")
            && stderr.contains("msg=\"page rendered no visible text\""),
        "expected a blank-page record: {stderr}"
    );
    assert!(stderr.contains("js.caps"), "{stderr}");
    assert!(stdout.trim().is_empty(), "dump: {stdout}");
}

#[test]
fn module_script_runs() {
    let (stdout, stderr) = run_fixture("module-script.html", "warn");

    assert!(
        !stderr.contains("feature=\"module-script\""),
        "module scripts execute: {stderr}"
    );
    assert!(stdout.contains("module ran"), "dump: {stdout}");
}

#[test]
fn module_import_is_reported_as_unsupported() {
    let (stdout, stderr) = run_fixture("module-import.html", "debug");

    assert!(
        stderr.contains("target=\"js.unsupported\"")
            && stderr.contains("feature=\"module-import\""),
        "expected a module-import record: {stderr}"
    );
    assert!(
        stderr.contains("msg=\"page scripts settled\"") && stderr.contains("status=\"degraded\""),
        "expected a degraded summary: {stderr}"
    );
    assert!(stdout.contains("before"), "dump: {stdout}");
    assert!(!stdout.contains("imported"), "dump: {stdout}");
}
