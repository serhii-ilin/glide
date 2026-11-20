//! Compatibility benchmark runner (`cargo run --release --bin compat`).

use glide::compat::runner::{
    RunnerConfig, Suite, default_jobs, repo_root, resolve_glide_binary, run_compat,
};

fn main() {
    if let Err(err) = run_main() {
        eprintln!("compat failed: {err}");
        std::process::exit(1);
    }
}

fn run_main() -> Result<(), String> {
    let mut suite = Suite::All;
    let mut filter = None;
    let mut jobs = default_jobs();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--suite" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--suite requires acid|wpt|probe|all".to_string())?;
                suite = Suite::parse(&value).ok_or_else(|| format!("unknown suite {value}"))?;
            }
            "--filter" => {
                filter = Some(
                    args.next()
                        .ok_or_else(|| "--filter requires a substring".to_string())?,
                );
            }
            "--jobs" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--jobs requires a number".to_string())?;
                jobs = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --jobs {value}"))?;
            }
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }

    let root = repo_root();
    let glide_bin = resolve_glide_binary()?;
    let cfg = RunnerConfig {
        suite,
        filter,
        jobs,
        glide_bin,
        repo_root: root.clone(),
        cache_dir: root.join("target/compat-cache"),
        out_dir: root.join("target/compat-out"),
        wpt_dir: root.join("tests/compat/wpt"),
        probe_html: root.join("tests/compat/probe.html"),
        es_smoke_html: root.join("tests/compat/es-smoke.html"),
        results_json: root.join("docs/compat/results.json"),
        report_md: root.join("docs/compat/REPORT.md"),
    };
    let results = run_compat(cfg)?;
    eprintln!("wrote {} results", results.len());
    Ok(())
}

fn print_usage() {
    eprintln!(
        "Usage: compat [--suite acid|wpt|probe|all] [--filter SUBSTR] [--jobs N]\n\
         Requires `target/release/glide` next to this binary."
    );
}
