//! Integration test: run compat runner offline on vendored integration fixtures.

use glide::compat::runner::{RunnerConfig, Suite, repo_root, resolve_glide_binary, run_compat};

#[test]
fn compat_runner_filter_integration_offline() {
    let root = repo_root();
    let _ = std::process::Command::new("cargo")
        .args(["build", "--quiet", "--bin", "glide"])
        .current_dir(&root)
        .status()
        .expect("cargo build glide");
    let glide_bin = resolve_glide_binary().expect("build glide binary first");
    let cfg = RunnerConfig {
        suite: Suite::Wpt,
        filter: Some("integration".into()),
        jobs: 1,
        glide_bin,
        repo_root: root.clone(),
        cache_dir: root.join("target/compat-cache-test"),
        out_dir: root.join("target/compat-out-test"),
        wpt_dir: root.join("tests/compat/wpt"),
        probe_html: root.join("tests/compat/probe.html"),
        es_smoke_html: root.join("tests/compat/es-smoke.html"),
        results_json: root.join("target/compat-results-test.json"),
        report_md: root.join("target/compat-report-test.md"),
    };
    let results = run_compat(cfg).expect("run_compat");
    let green = results
        .iter()
        .find(|r| r.path == "integration/green-box.html")
        .expect("green-box result");
    assert_eq!(green.status, "pass");
    let harness = results
        .iter()
        .find(|r| r.path == "integration/harness-minimal.html")
        .expect("harness-minimal result");
    assert_eq!(harness.status, "fail");
    let reason = harness.reason.as_deref().expect("harness failure reason");
    assert!(reason.contains("harness did not load:"));
    assert!(
        reason.contains("ReferenceError") || reason.contains("TypeError"),
        "unexpected harness failure: {reason}"
    );
}
