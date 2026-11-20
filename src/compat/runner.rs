//! Orchestrate compatibility suites by spawning release `glide` subprocesses.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::parse::{ReftestKind, parse_acid3_score, parse_reftest_link, parse_testharness_json};
use super::png::{MAX_DIFF_FRACTION, reftest_images_match};
use super::subprocess::{RunFailureKind, classify_run};

const RUN_TIMEOUT: Duration = Duration::from_secs(30);
const STDERR_TAIL: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suite {
    Acid,
    Wpt,
    Probe,
    All,
}

impl Suite {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "acid" => Some(Self::Acid),
            "wpt" => Some(Self::Wpt),
            "probe" => Some(Self::Probe),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub suite: String,
    pub path: String,
    pub kind: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reproduce: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub suite: Suite,
    pub filter: Option<String>,
    pub jobs: usize,
    pub glide_bin: PathBuf,
    pub repo_root: PathBuf,
    pub cache_dir: PathBuf,
    pub out_dir: PathBuf,
    pub wpt_dir: PathBuf,
    pub probe_html: PathBuf,
    pub es_smoke_html: PathBuf,
    pub results_json: PathBuf,
    pub report_md: PathBuf,
}

pub struct GlideRun {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

pub fn resolve_glide_binary() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe
        .parent()
        .ok_or_else(|| "current_exe has no parent".to_string())?;
    let glide = dir.join("glide");
    if glide.is_file() {
        return Ok(glide);
    }
    let root = repo_root();
    for sub in ["release", "debug"] {
        let candidate = root.join("target").join(sub).join("glide");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "glide binary not found next to compat ({}) or under target/{{release,debug}}/",
        glide.display()
    ))
}

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn default_jobs() -> usize {
    thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(4)
}

pub fn run_compat(cfg: RunnerConfig) -> Result<Vec<TestResult>, String> {
    let _ = std::fs::create_dir_all(&cfg.cache_dir);
    let _ = std::fs::create_dir_all(&cfg.out_dir);
    let _ = std::fs::create_dir_all(
        cfg.results_json
            .parent()
            .ok_or_else(|| "results_json path has no parent".to_string())?,
    );

    let mut results = Vec::new();
    match cfg.suite {
        Suite::Acid | Suite::All => results.extend(run_acid_suite(&cfg)?),
        _ => {}
    }
    match cfg.suite {
        Suite::Wpt | Suite::All => results.extend(run_wpt_suite(&cfg)?),
        _ => {}
    }
    match cfg.suite {
        Suite::Probe | Suite::All => {
            results.extend(run_probe(&cfg)?);
            results.extend(run_es_smoke(&cfg)?);
        }
        _ => {}
    }

    if let Some(filter) = &cfg.filter {
        results.retain(|r| r.path.contains(filter) || r.suite.contains(filter));
    }

    write_results(&cfg.results_json, &results)?;
    write_report(&cfg, &results)?;
    Ok(results)
}

fn run_acid_suite(cfg: &RunnerConfig) -> Result<Vec<TestResult>, String> {
    let mut out = Vec::new();
    out.push(run_acid1(cfg)?);
    out.extend(run_acid2(cfg)?);
    out.extend(run_acid3(cfg)?);
    Ok(out)
}

fn run_acid1(cfg: &RunnerConfig) -> Result<TestResult, String> {
    let url = "https://www.w3.org/Style/CSS/Test/CSS1/current/test5526c.htm";
    let png_ref_url = "https://www.w3.org/Style/CSS/Test/CSS1/current/test5526c.png";
    let path = "acid1/test5526c.htm";
    let shot = cfg.out_dir.join("acid1-test5526c.png");
    if let Err(reason) = fetch_cached(&cfg.cache_dir, url) {
        return Ok(fail_result(
            "acid",
            path,
            "reftest",
            "unreachable",
            None,
            Some(reason),
            None,
        ));
    }
    let run = glide_screenshot(cfg, url, &shot, true)?;
    if let Some(failure) = classify_subprocess(cfg, url, true, &run, "screenshot") {
        return Ok(failure);
    }
    let ref_png = cfg.cache_dir.join("acid1-test5526c.png");
    let status;
    let reason;
    let metrics;
    if fetch_cached(&cfg.cache_dir, png_ref_url).is_ok() && ref_png.is_file() {
        match reftest_images_match(&shot, &ref_png) {
            Ok(true) => {
                status = "pass";
                reason = None;
                metrics = Some(serde_json::json!({"diff_pct": 0.0}));
            }
            Ok(false) => {
                let diff = super::png::png_diff_fraction(&shot, &ref_png).unwrap_or(1.0);
                status = "fail";
                reason = Some(format!(
                    "pixel diff {:.2}% (tolerance {:.2}%)",
                    diff * 100.0,
                    MAX_DIFF_FRACTION * 100.0
                ));
                metrics = Some(serde_json::json!({"diff_pct": diff * 100.0}));
            }
            Err(err) => {
                status = "manual";
                reason = Some(err);
                metrics = None;
            }
        }
    } else {
        status = "manual";
        reason = Some("reference PNG unavailable; screenshot saved for manual review".into());
        metrics = None;
    }
    Ok(TestResult {
        suite: "acid".into(),
        path: path.into(),
        kind: "reftest".into(),
        status: status.into(),
        metrics,
        reason,
        reproduce: Some(repro_screenshot(cfg, url, &shot, true)),
    })
}

fn run_acid2(cfg: &RunnerConfig) -> Result<Vec<TestResult>, String> {
    let test_url = "http://acid2.acidtests.org/#top";
    let ref_url = "http://acid2.acidtests.org/reference.html";
    let mut out = Vec::new();
    let test_png = cfg.out_dir.join("acid2-test.png");
    let ref_png = cfg.out_dir.join("acid2-reference.png");
    for (label, url, dest) in [
        ("acid2/test.html", test_url, test_png.clone()),
        ("acid2/reference.html", ref_url, ref_png.clone()),
    ] {
        if fetch_cached(&cfg.cache_dir, url).is_err() {
            out.push(fail_result(
                "acid",
                label,
                "reftest",
                "unreachable",
                None,
                Some(format!("unreachable: {url}")),
                None,
            ));
            continue;
        }
        let run = glide_screenshot(cfg, url, &dest, true)?;
        if let Some(failure) = classify_subprocess(cfg, url, true, &run, "screenshot") {
            out.push(failure);
            return Ok(out);
        }
    }
    let diff = super::png::png_diff_fraction(&test_png, &ref_png).unwrap_or(1.0);
    let status = if diff <= MAX_DIFF_FRACTION {
        "pass"
    } else {
        "fail"
    };
    out.push(TestResult {
        suite: "acid".into(),
        path: "acid2/compare".into(),
        kind: "reftest".into(),
        status: status.into(),
        metrics: Some(serde_json::json!({"diff_pct": diff * 100.0})),
        reason: if status == "fail" {
            Some("Acid2 test page vs reference (expected differences)".into())
        } else {
            None
        },
        reproduce: Some(repro_screenshot(cfg, test_url, &test_png, true)),
    });
    Ok(out)
}

fn run_acid3(cfg: &RunnerConfig) -> Result<Vec<TestResult>, String> {
    let url = "http://acid3.acidtests.org/";
    let mut out = Vec::new();
    for (no_js, label) in [(false, "acid3/with-js"), (true, "acid3/no-js")] {
        if fetch_cached(&cfg.cache_dir, url).is_err() {
            out.push(fail_result(
                "acid",
                label,
                "harness",
                "unreachable",
                None,
                Some(format!("unreachable: {url}")),
                None,
            ));
            continue;
        }
        let run = glide_dump_text(cfg, url, !no_js)?;
        if let Some(failure) = classify_subprocess(cfg, url, !no_js, &run, "dump-text") {
            out.push(failure);
            continue;
        }
        let score = parse_acid3_score(&run.stdout);
        let status = if no_js || score.is_some() {
            "pass"
        } else {
            "fail"
        };
        out.push(TestResult {
            suite: "acid".into(),
            path: label.into(),
            kind: "score".into(),
            status: status.into(),
            metrics: score.map(|s| serde_json::json!({"acid3_score": s})),
            reason: if score.is_none() && !no_js {
                Some("could not parse NN/100 score".into())
            } else {
                None
            },
            reproduce: Some(repro_dump(cfg, url, !no_js)),
        });
    }
    Ok(out)
}

fn run_wpt_suite(cfg: &RunnerConfig) -> Result<Vec<TestResult>, String> {
    let manifest = cfg.wpt_dir.join("MANIFEST.txt");
    if !manifest.is_file() {
        return Err(format!("missing WPT manifest: {}", manifest.display()));
    }
    let manifest_text =
        std::fs::read_to_string(&manifest).map_err(|e| format!("read manifest: {e}"))?;
    let mut work: Vec<(String, String)> = Vec::new();
    for line in manifest_text.lines() {
        let path = line.trim();
        if path.is_empty() || path.starts_with('#') {
            continue;
        }
        if let Some(filter) = &cfg.filter
            && !path.contains(filter)
        {
            continue;
        }
        if !path.ends_with(".html") {
            continue;
        }
        if path.ends_with("-ref.html") || path.ends_with("_ref.html") {
            continue;
        }
        let full = cfg.wpt_dir.join(path);
        if !full.is_file() {
            continue;
        }
        let html = std::fs::read_to_string(&full).map_err(|e| e.to_string())?;
        work.push((path.to_string(), html));
    }

    let mut reftests = Vec::new();
    let mut harness = Vec::new();
    for (path, html) in work {
        if html.contains("testharness.js") {
            harness.push(path);
        } else if let Some(link) = parse_reftest_link(&html) {
            reftests.push((path, html, link));
        }
    }

    let shared = std::sync::Arc::new(cfg.clone());
    let mut results = parallel_collect(shared.clone(), cfg.jobs, reftests, {
        let cfg = shared.clone();
        move |(path, html, link)| run_wpt_reftest(&cfg, &path, &html, &link)
    });
    results.extend(parallel_collect(shared.clone(), cfg.jobs, harness, {
        let cfg = shared;
        move |path| run_wpt_harness(&cfg, &path)
    }));
    Ok(results)
}

fn parallel_collect<T, F>(
    _cfg: std::sync::Arc<RunnerConfig>,
    jobs: usize,
    items: Vec<T>,
    f: F,
) -> Vec<TestResult>
where
    T: Send + Clone + 'static,
    F: Fn(T) -> Result<TestResult, String> + Send + Sync + 'static,
{
    if items.is_empty() {
        return Vec::new();
    }
    let jobs = jobs.max(1);
    let f = std::sync::Arc::new(f);
    let (tx, rx) = mpsc::channel::<Result<TestResult, String>>();
    let chunk = items.len().div_ceil(jobs);
    let mut handles = Vec::new();
    for batch in items.chunks(chunk.max(1)) {
        let batch: Vec<T> = batch.to_vec();
        let tx = tx.clone();
        let f = f.clone();
        handles.push(thread::spawn(move || {
            for item in batch {
                match f(item) {
                    Ok(r) => {
                        let _ = tx.send(Ok(r));
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                    }
                }
            }
        }));
    }
    drop(tx);
    let mut out = Vec::new();
    for msg in rx {
        match msg {
            Ok(r) => out.push(r),
            Err(e) => out.push(fail_result(
                "wpt",
                "parallel",
                "error",
                "fail",
                None,
                Some(e),
                None,
            )),
        }
    }
    for h in handles {
        let _ = h.join();
    }
    out
}

fn run_wpt_reftest(
    cfg: &RunnerConfig,
    path: &str,
    html: &str,
    link: &super::parse::ReftestLink,
) -> Result<TestResult, String> {
    let test_abs = cfg.wpt_dir.join(path);
    let test_url = file_url(&test_abs);
    let ref_path = resolve_wpt_href(&test_abs, &link.href);
    let test_png = cfg.out_dir.join(safe_name(path)).with_extension("png");
    let ref_png = cfg
        .out_dir
        .join(safe_name(&format!("{path}-ref")))
        .with_extension("png");

    let run_test = glide_screenshot(cfg, &test_url, &test_png, true)?;
    if let Some(failure) = classify_subprocess(cfg, &test_url, true, &run_test, "screenshot") {
        return Ok(failure);
    }
    let ref_url = file_url(&ref_path);
    if !ref_path.is_file() {
        let ref_from_html = parse_reftest_link(html).map(|l| l.href);
        return Ok(fail_result(
            "wpt",
            path,
            "reftest",
            "fail",
            None,
            Some(format!(
                "missing reference {}",
                ref_from_html.unwrap_or(link.href.clone())
            )),
            Some(repro_screenshot(cfg, &test_url, &test_png, true)),
        ));
    }
    let run_ref = glide_screenshot(cfg, &ref_url, &ref_png, true)?;
    if let Some(mut failure) = classify_subprocess(cfg, &ref_url, true, &run_ref, "screenshot") {
        failure.path = path.into();
        failure.suite = "wpt".into();
        return Ok(failure);
    }
    let same = reftest_images_match(&test_png, &ref_png)?;
    let diff = super::png::png_diff_fraction(&test_png, &ref_png).unwrap_or(1.0);
    let matched = match link.kind {
        ReftestKind::Match => same,
        ReftestKind::Mismatch => !same,
    };
    Ok(TestResult {
        suite: "wpt".into(),
        path: path.into(),
        kind: "reftest".into(),
        status: if matched { "pass" } else { "fail" }.into(),
        metrics: Some(serde_json::json!({
            "diff_pct": diff * 100.0,
            "tolerance_pct": MAX_DIFF_FRACTION * 100.0,
            "rel": if link.kind == ReftestKind::Match { "match" } else { "mismatch" },
        })),
        reason: if matched {
            None
        } else {
            Some(format!("pixel diff {:.4}%", diff * 100.0))
        },
        reproduce: Some(repro_screenshot(cfg, &test_url, &test_png, true)),
    })
}

fn run_wpt_harness(cfg: &RunnerConfig, path: &str) -> Result<TestResult, String> {
    let full = cfg.wpt_dir.join(path);
    let url = file_url(&full);
    let run = glide_dump_text(cfg, &url, true)?;
    if let Some(mut failure) = classify_subprocess(cfg, &url, true, &run, "dump-text") {
        failure.path = path.into();
        failure.suite = "wpt".into();
        failure.kind = "testharness".into();
        return Ok(failure);
    }
    match parse_testharness_json(&run.stdout) {
        Ok(summary) => {
            let passed = summary.tests.iter().filter(|t| t.status == "PASS").count();
            let failed = summary.tests.iter().filter(|t| t.status == "FAIL").count();
            let timeout = summary
                .tests
                .iter()
                .filter(|t| t.status == "TIMEOUT")
                .count();
            let all_pass = summary.status == "OK" && failed == 0 && timeout == 0;
            Ok(TestResult {
                suite: "wpt".into(),
                path: path.into(),
                kind: "testharness".into(),
                status: if all_pass { "pass" } else { "fail" }.into(),
                metrics: Some(serde_json::json!({
                    "passed": passed,
                    "failed": failed,
                    "timeout": timeout,
                    "harness_status": summary.status,
                })),
                reason: if all_pass {
                    None
                } else {
                    Some(format!("{failed} failed, {timeout} timeout"))
                },
                reproduce: Some(repro_dump(cfg, &url, true)),
            })
        }
        Err(err) => Ok(fail_result(
            "wpt",
            path,
            "testharness",
            "fail",
            None,
            Some(err),
            Some(repro_dump(cfg, &url, true)),
        )),
    }
}

fn run_probe(cfg: &RunnerConfig) -> Result<Vec<TestResult>, String> {
    let url = file_url(&cfg.probe_html);
    let run = glide_dump_text(cfg, &url, true)?;
    if let Some(failure) = classify_subprocess(cfg, &url, true, &run, "dump-text") {
        return Ok(vec![failure]);
    }
    let json_line = run
        .stdout
        .lines()
        .find(|l| l.trim_start().starts_with('{'))
        .map(|l| l.trim().to_string());
    let mut results = Vec::new();
    let Some(json) = json_line else {
        return Ok(vec![fail_result(
            "probe",
            "probe.html",
            "api",
            "fail",
            None,
            Some("missing API probe JSON in output".into()),
            Some(repro_dump(cfg, &url, true)),
        )]);
    };
    let map: HashMap<String, String> =
        serde_json::from_str(&json).map_err(|e| format!("probe json: {e}"))?;
    let mut missing = Vec::new();
    let mut present = 0_u32;
    for (name, ty) in &map {
        results.push(TestResult {
            suite: "probe".into(),
            path: name.clone(),
            kind: "api".into(),
            status: if ty == "undefined" { "fail" } else { "pass" }.into(),
            metrics: Some(serde_json::json!({"typeof": ty})),
            reason: if ty == "undefined" {
                Some("missing".into())
            } else {
                None
            },
            reproduce: None,
        });
        if ty == "undefined" {
            missing.push(name.clone());
        } else {
            present += 1;
        }
    }
    results.push(TestResult {
        suite: "probe".into(),
        path: "__summary__".into(),
        kind: "api".into(),
        status: if missing.is_empty() {
            "pass".into()
        } else {
            "fail".into()
        },
        metrics: Some(serde_json::json!({
            "present": present,
            "missing": missing.len(),
        })),
        reason: if missing.is_empty() {
            None
        } else {
            Some(format!("missing: {}", missing.join(", ")))
        },
        reproduce: Some(repro_dump(cfg, &url, true)),
    });
    Ok(results)
}

fn run_es_smoke(cfg: &RunnerConfig) -> Result<Vec<TestResult>, String> {
    let url = file_url(&cfg.es_smoke_html);
    let run = glide_dump_text(cfg, &url, true)?;
    if let Some(failure) = classify_subprocess(cfg, &url, true, &run, "dump-text") {
        return Ok(vec![failure]);
    }
    let ok = run.stdout.lines().any(|l| l.trim() == "ES_SMOKE_OK");
    Ok(vec![TestResult {
        suite: "probe".into(),
        path: "es-smoke.html".into(),
        kind: "ecmascript".into(),
        status: if ok { "pass" } else { "fail" }.into(),
        metrics: None,
        reason: if ok {
            None
        } else {
            Some("ES smoke line ES_SMOKE_OK not found".into())
        },
        reproduce: Some(repro_dump(cfg, &url, true)),
    }])
}

fn classify_subprocess(
    cfg: &RunnerConfig,
    url: &str,
    js: bool,
    run: &GlideRun,
    mode: &str,
) -> Option<TestResult> {
    let stderr_tail = tail_str(&run.stderr, STDERR_TAIL);
    if let Some(kind) = classify_run(run.timed_out, run.exit_code, &run.stderr) {
        let status = match kind {
            RunFailureKind::Hang => "hang",
            RunFailureKind::Panic => "panic",
            RunFailureKind::Crash => "crash",
        };
        let repro = if mode == "screenshot" {
            repro_screenshot(cfg, url, &cfg.out_dir.join("failed.png"), js)
        } else {
            repro_dump(cfg, url, js)
        };
        return Some(TestResult {
            suite: "unknown".into(),
            path: url.into(),
            kind: mode.into(),
            status: status.into(),
            metrics: None,
            reason: Some(stderr_tail),
            reproduce: Some(repro),
        });
    }
    None
}

pub fn glide_screenshot(
    cfg: &RunnerConfig,
    url: &str,
    output: &Path,
    js: bool,
) -> Result<GlideRun, String> {
    let mut cmd = Command::new(&cfg.glide_bin);
    cmd.arg("--screenshot");
    if !js {
        cmd.arg("--no-js");
    }
    cmd.arg(url).arg(output);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    run_with_timeout(cmd)
}

pub fn glide_dump_text(cfg: &RunnerConfig, url: &str, js: bool) -> Result<GlideRun, String> {
    let mut cmd = Command::new(&cfg.glide_bin);
    cmd.arg("--dump-text");
    if !js {
        cmd.arg("--no-js");
    }
    cmd.arg(url);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    run_with_timeout(cmd)
}

pub fn run_with_timeout(mut cmd: Command) -> Result<GlideRun, String> {
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let stdout_handle = stdout_pipe.map(|mut out| {
        thread::spawn(move || {
            let mut buf = String::new();
            let _ = out.read_to_string(&mut buf);
            buf
        })
    });
    let stderr_handle = stderr_pipe.map(|mut err| {
        thread::spawn(move || {
            let mut buf = String::new();
            let _ = err.read_to_string(&mut buf);
            buf
        })
    });
    let started = Instant::now();
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => {
                let stdout = stdout_handle
                    .map(|h| h.join().unwrap_or_default())
                    .unwrap_or_default();
                let stderr = stderr_handle
                    .map(|h| h.join().unwrap_or_default())
                    .unwrap_or_default();
                return Ok(GlideRun {
                    exit_code: status.code(),
                    stdout,
                    stderr,
                    timed_out: false,
                });
            }
            None => {
                if started.elapsed() >= RUN_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    let stderr = stderr_handle
                        .map(|h| h.join().unwrap_or_default())
                        .unwrap_or_default();
                    return Ok(GlideRun {
                        exit_code: None,
                        stdout: String::new(),
                        stderr: if stderr.is_empty() {
                            format!("timed out after {}s", RUN_TIMEOUT.as_secs())
                        } else {
                            stderr
                        },
                        timed_out: true,
                    });
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

fn fetch_cached(cache_dir: &Path, url: &str) -> Result<(), String> {
    let name = safe_name(url);
    let path = cache_dir.join(name);
    if path.is_file() {
        return Ok(());
    }
    use std::sync::Arc;

    use blitz_net::Provider;
    use blitz_traits::net::Request;

    let parsed = url::Url::parse(url).map_err(|e| format!("bad url {url}: {e}"))?;
    let runtime = crate::tokio_runtime();
    let _guard = runtime.enter();
    let net = Arc::new(Provider::new(None));
    let (_, body) = runtime
        .block_on(net.fetch_async(Request::get(parsed)))
        .map_err(|e| format!("fetch {url}: {e}"))?;
    std::fs::write(&path, &body).map_err(|e| e.to_string())?;
    Ok(())
}

fn write_results(path: &Path, results: &[TestResult]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(results).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

fn write_report(cfg: &RunnerConfig, results: &[TestResult]) -> Result<(), String> {
    let date = chrono_lite_date();
    let commit = git_short_head(&cfg.repo_root).unwrap_or_else(|| "unknown".into());
    let os = std::env::consts::OS;
    let mut md = String::new();
    md.push_str("# Glide compatibility report\n\n");
    md.push_str(&format!("Date: {date}\n"));
    md.push_str(&format!("Git: `{commit}`\n"));
    md.push_str(&format!("OS: {os}\n\n"));

    md.push_str("## Summary\n\n");
    md.push_str("| Suite | Pass | Fail | Other | Total |\n");
    md.push_str("| --- | ---: | ---: | ---: | ---: |\n");
    for suite in ["acid", "wpt", "probe"] {
        let subset: Vec<_> = results
            .iter()
            .filter(|r| r.suite == suite && r.path != "__summary__")
            .collect();
        let pass = subset.iter().filter(|r| r.status == "pass").count();
        let fail = subset.iter().filter(|r| r.status == "fail").count();
        let other = subset.len() - pass - fail;
        md.push_str(&format!(
            "| {suite} | {pass} | {fail} | {other} | {} |\n",
            subset.len()
        ));
    }
    md.push('\n');

    md.push_str("## Super-critical\n\n");
    let critical: Vec<_> = results
        .iter()
        .filter(|r| matches!(r.status.as_str(), "crash" | "panic" | "hang"))
        .collect();
    if critical.is_empty() {
        md.push_str("None recorded.\n\n");
    } else {
        for r in critical {
            md.push_str(&format!(
                "- **{}** `{}` — {}\n",
                r.status,
                r.path,
                r.reason.as_deref().unwrap_or("")
            ));
            if let Some(repro) = &r.reproduce {
                md.push_str(&format!("  - Reproduce: `{repro}`\n"));
            }
        }
        md.push('\n');
    }

    md.push_str("## Top failure categories\n\n");
    for (label, count) in failure_categories(results) {
        md.push_str(&format!("- {label}: {count}\n"));
    }
    md.push('\n');

    md.push_str("## API probe\n\n");
    md.push_str("| API | typeof |\n| --- | --- |\n");
    for r in results
        .iter()
        .filter(|r| r.suite == "probe" && r.kind == "api" && r.path != "__summary__")
    {
        let ty = r
            .metrics
            .as_ref()
            .and_then(|m| m.get("typeof"))
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        md.push_str(&format!("| {} | {} |\n", r.path, ty));
    }
    md.push('\n');

    md.push_str("## Acid scores\n\n");
    for r in results.iter().filter(|r| r.suite == "acid") {
        let metric = r
            .metrics
            .as_ref()
            .map(|m| m.to_string())
            .unwrap_or_default();
        md.push_str(&format!("- `{}` — **{}** {metric}\n", r.path, r.status));
    }
    md.push('\n');

    md.push_str("## ECMAScript\n\n");
    md.push_str(
        "Language conformance is provided by embedded V8; see [V8 test262 status](https://v8.dev/docs/test262).\n",
    );
    md.push_str("This run includes a short in-page ES smoke test only.\n\n");

    md.push_str("## Benchmarks considered\n\n");
    md.push_str(
        "- **Acid1–3**: classic layout/DOM/JS regression targets; Acid1-3 URLs used live (not vendored).\n",
    );
    md.push_str(
        "- **WPT subset**: BSD-licensed reftests and testharness files vendored under `tests/compat/wpt/`.\n",
    );
    md.push_str("- **css3test.com / html5test**: JS-heavy feature probes; deferred (overlap with probe + WPT).\n");
    md.push_str("- **test262**: V8 responsibility; not re-run here.\n");
    md.push_str("- **Speedometer / JetStream**: performance, not compatibility.\n");

    std::fs::write(&cfg.report_md, md).map_err(|e| e.to_string())
}

fn failure_categories(results: &[TestResult]) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for r in results {
        if r.status == "fail" {
            let key = format!("{}:{}", r.suite, r.kind);
            *counts.entry(key).or_default() += 1;
        }
    }
    let mut items: Vec<_> = counts.into_iter().collect();
    items.sort_by_key(|a| std::cmp::Reverse(a.1));
    items
}

fn fail_result(
    suite: &str,
    path: &str,
    kind: &str,
    status: &str,
    metrics: Option<serde_json::Value>,
    reason: Option<String>,
    reproduce: Option<String>,
) -> TestResult {
    TestResult {
        suite: suite.into(),
        path: path.into(),
        kind: kind.into(),
        status: status.into(),
        metrics,
        reason,
        reproduce,
    }
}

fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn resolve_wpt_href(test_html: &Path, href: &str) -> PathBuf {
    test_html
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(href)
}

fn safe_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn tail_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        s[s.len() - max..].to_string()
    }
}

fn repro_screenshot(cfg: &RunnerConfig, url: &str, out: &Path, js: bool) -> String {
    let js_flag = if js { "" } else { " --no-js" };
    format!(
        "{} --screenshot{js_flag} {url} {}",
        cfg.glide_bin.display(),
        out.display()
    )
}

fn repro_dump(cfg: &RunnerConfig, url: &str, js: bool) -> String {
    let js_flag = if js { "" } else { " --no-js" };
    format!("{} --dump-text{js_flag} {url}", cfg.glide_bin.display())
}

fn git_short_head(root: &Path) -> Option<String> {
    let out = Command::new("git")
        .args([
            "-C",
            &root.display().to_string(),
            "rev-parse",
            "--short",
            "HEAD",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn chrono_lite_date() -> String {
    use std::process::Command;
    Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}
