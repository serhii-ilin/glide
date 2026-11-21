//! Offline regression gate for vendored WPT `dom/nodes` (R-014).
//!
//! The subset is every HTML file under `tests/compat/wpt/dom/nodes/`, sorted by
//! name. It is not a hand-picked list of files that currently pass. A file that
//! produces no subtest results counts as a failed file and as one failed subtest,
//! so the pass rate is not inflated by timeouts and harness errors.

use std::path::PathBuf;

use glide::compat::runner::{RunnerConfig, Suite, repo_root, run_compat};

fn dom_nodes_subset() -> Vec<String> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/compat/wpt/dom/nodes");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("dom/nodes dir")
        .filter_map(|ent| ent.ok())
        .filter_map(|ent| {
            let name = ent.file_name().to_string_lossy().into_owned();
            name.ends_with(".html").then(|| format!("dom/nodes/{name}"))
        })
        .collect();
    names.sort();
    names
}

#[test]
fn compat_dom_nodes_meets_milestone() {
    let root = repo_root();
    let glide_bin = PathBuf::from(env!("CARGO_BIN_EXE_glide"));
    let subset = dom_nodes_subset();
    assert!(
        subset.len() >= 20,
        "expected the vendored dom/nodes directory, found {}",
        subset.len()
    );

    let cfg = RunnerConfig {
        suite: Suite::Wpt,
        filter: None,
        only_paths: Some(subset.clone()),
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

    let results = run_compat(cfg).expect("run_compat dom/nodes subset");
    let by_path: std::collections::HashMap<&str, &glide::compat::runner::TestResult> =
        results.iter().map(|r| (r.path.as_str(), r)).collect();

    let mut sub_pass = 0_u64;
    let mut sub_total = 0_u64;
    let mut full_pass_files = 0_u64;

    for path in &subset {
        let Some(r) = by_path.get(path.as_str()) else {
            sub_total += 1;
            continue;
        };
        let Some(metrics) = &r.metrics else {
            sub_total += 1;
            continue;
        };
        let passed = metrics.get("passed").and_then(|v| v.as_u64()).unwrap_or(0);
        let failed = metrics.get("failed").and_then(|v| v.as_u64()).unwrap_or(0);
        let timeout = metrics.get("timeout").and_then(|v| v.as_u64()).unwrap_or(0);
        let reported = passed + failed + timeout;
        if reported == 0 {
            sub_total += 1;
            continue;
        }
        sub_pass += passed;
        sub_total += reported;
        if r.status == "pass" && failed == 0 && timeout == 0 {
            full_pass_files += 1;
        }
    }

    assert!(sub_total > 0, "no dom/nodes subtests were counted");
    let pct = (sub_pass as f64) * 100.0 / (sub_total as f64);
    // Floors are below the debug measurement (12 fully passing files, ~66% of
    // subtests when files with no results count as one failed subtest) so one
    // flaky file does not fail the gate, and well above the R-014 collapse
    // line (15% / 3 files).
    assert!(
        pct >= 50.0,
        "dom/nodes subtest pass rate {pct:.1}% ({sub_pass}/{sub_total}) below 50% (files with no results count as failures)"
    );
    assert!(
        full_pass_files >= 10,
        "only {full_pass_files} dom/nodes files fully pass (need ≥10 of the vendored set); subtests {sub_pass}/{sub_total}"
    );
}
