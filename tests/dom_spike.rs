//! Ensures the DOM library spike fork harness respects per-file timeouts.

use std::process::Command;

#[test]
fn dom_spike_fork_wall_clock_budget() {
    let spike_nm = "target/dom-spike/node_modules";
    if !std::path::Path::new(spike_nm).exists() {
        eprintln!(
            "skip dom_spike_fork_wall_clock_budget: run `node scripts/dom-spike/run.mjs` once to install deps"
        );
        return;
    }
    let out = Command::new("node")
        .args(["--test", "scripts/dom-spike/tests/fork-timeout.test.mjs"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("node --test");
    assert!(
        out.status.success(),
        "dom spike fork test failed:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
