//! Compatibility benchmark helpers (PNG diff, HTML harness parsing, subprocess classification).

mod parse;
mod png;
pub mod runner;
mod subprocess;

pub use parse::{
    HarnessSubtest, HarnessSummary, ReftestKind, ReftestLink, parse_acid3_score,
    parse_reftest_link, parse_testharness_json,
};
pub use png::{
    MAX_DIFF_FRACTION, decode_png, diff_pixels, png_diff_fraction, reftest_images_match,
};
pub use subprocess::{RunFailureKind, classify_run};
