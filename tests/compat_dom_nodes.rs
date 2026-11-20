//! Offline regression gate for vendored WPT `dom/nodes` (R-014).

use glide::compat::runner::{RunnerConfig, Suite, repo_root, run_compat};

#[test]
fn compat_dom_nodes_meets_milestone() {
    let root = repo_root();
    let status = std::process::Command::new("cargo")
        .args(["build", "--quiet", "--release", "--bin", "glide"])
        .current_dir(&root)
        .status()
        .expect("cargo build --release glide");
    assert!(status.success(), "release glide build failed");
    let glide_bin = root.join("target/release/glide");
    assert!(glide_bin.is_file(), "missing {}", glide_bin.display());

    let cfg = RunnerConfig {
        suite: Suite::Wpt,
        filter: Some("dom/nodes".into()),
        jobs: 4,
        glide_bin,
        repo_root: root.clone(),
        cache_dir: root.join("target/compat-cache-dom-nodes"),
        out_dir: root.join("target/compat-out-dom-nodes"),
        wpt_dir: root.join("tests/compat/wpt"),
        probe_html: root.join("tests/compat/probe.html"),
        es_smoke_html: root.join("tests/compat/es-smoke.html"),
        results_json: root.join("target/compat-dom-nodes-results.json"),
        report_md: root.join("target/compat-dom-nodes-report.md"),
    };

    let results = run_compat(cfg).expect("run_compat dom/nodes");
    assert!(
        !results.is_empty(),
        "expected dom/nodes harness results, got none"
    );

    let mut sub_pass = 0_u64;
    let mut sub_total = 0_u64;
    let mut full_pass_files = 0_u64;

    for r in &results {
        if r.kind != "testharness" {
            continue;
        }
        if r.status == "pass" {
            full_pass_files += 1;
        }
        let Some(metrics) = &r.metrics else {
            continue;
        };
        let passed = metrics.get("passed").and_then(|v| v.as_u64()).unwrap_or(0);
        let failed = metrics.get("failed").and_then(|v| v.as_u64()).unwrap_or(0);
        let timeout = metrics.get("timeout").and_then(|v| v.as_u64()).unwrap_or(0);
        sub_pass += passed;
        sub_total += passed + failed + timeout;
    }

    assert!(sub_total > 0, "no dom/nodes subtests were counted");

    let pct = (sub_pass as f64) * 100.0 / (sub_total as f64);
    assert!(
        pct >= 15.0,
        "dom/nodes subtest pass rate {pct:.1}% ({sub_pass}/{sub_total}) below 15%"
    );
    assert!(
        full_pass_files >= 3,
        "only {full_pass_files} dom/nodes files fully pass (need ≥3)"
    );
}
