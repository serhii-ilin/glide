//! Parse reftest links, testharness JSON, and Acid3 score from page text.

use serde::Deserialize;

/// `match` or `mismatch` reftest reference from HTML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReftestLink {
    pub kind: ReftestKind,
    pub href: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReftestKind {
    Match,
    Mismatch,
}

/// Find the first `<link rel="match|mismatch" href="…">` (attribute order may vary).
pub fn parse_reftest_link(html: &str) -> Option<ReftestLink> {
    let lower = html.to_ascii_lowercase();
    for tag in extract_link_tags(&lower) {
        let rel = attr_value(tag, "rel")?;
        let href = attr_value(tag, "href")?;
        let kind = if rel == "match" {
            ReftestKind::Match
        } else if rel == "mismatch" {
            ReftestKind::Mismatch
        } else {
            continue;
        };
        let href_orig = slice_href_from_original(html, tag, &href)?;
        return Some(ReftestLink {
            kind,
            href: href_orig,
        });
    }
    None
}

fn extract_link_tags(html: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<link") {
        let after = &rest[start..];
        let end = after.find('>').unwrap_or(after.len());
        out.push(&after[..end]);
        rest = &after[end.min(after.len())..];
        if end >= after.len() {
            break;
        }
    }
    out
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=");
    let start = tag.find(&needle)? + needle.len();
    let bytes = tag.as_bytes();
    let quote = bytes.get(start)?;
    if *quote != b'"' && *quote != b'\'' {
        return None;
    }
    let q = *quote as char;
    let value_start = start + 1;
    let tail = &tag[value_start..];
    let end = tail.find(q)?;
    Some(tail[..end].to_string())
}

fn slice_href_from_original(html: &str, lower_tag: &str, href_lower: &str) -> Option<String> {
    let pos = html.to_ascii_lowercase().find(lower_tag)?;
    let tag = &html[pos..pos + lower_tag.len()];
    attr_value(tag, "href").or_else(|| {
        // Fallback: preserve casing from lower copy
        Some(href_lower.to_string())
    })
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct HarnessSubtest {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct HarnessSummary {
    pub status: String,
    pub tests: Vec<HarnessSubtest>,
}

/// Parse JSON written into `<pre id="__glide_results">` from `--dump-text` output.
pub fn parse_testharness_json(dump_text: &str) -> Result<HarnessSummary, String> {
    let json = extract_glide_results_json(dump_text)
        .ok_or_else(|| "missing __glide_results JSON in dump-text".to_string())?;
    serde_json::from_str(&json).map_err(|e| format!("invalid harness JSON: {e}"))
}

fn extract_glide_results_json(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(raw) = trimmed.strip_prefix("__GLIDE_RESULTS__:") {
            return Some(raw.trim().to_string());
        }
    }
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') && trimmed.contains("\"tests\"") {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Parse Acid3 `NN/100` score from settled page text.
pub fn parse_acid3_score(text: &str) -> Option<u32> {
    for token in text.split_whitespace() {
        if let Some((num, rest)) = token.split_once('/')
            && rest == "100"
            && let Ok(n) = num.parse::<u32>()
            && n <= 100
        {
            return Some(n);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_match_link() {
        let html = r#"<!DOCTYPE html><link rel="match" href="foo-ref.html"><body></body>"#;
        let link = parse_reftest_link(html).expect("link");
        assert_eq!(link.kind, ReftestKind::Match);
        assert_eq!(link.href, "foo-ref.html");
    }

    #[test]
    fn parses_mismatch_link() {
        let html = r#"<html><link href="bar-ref.html" rel="mismatch"></html>"#;
        let link = parse_reftest_link(html).expect("link");
        assert_eq!(link.kind, ReftestKind::Mismatch);
        assert_eq!(link.href, "bar-ref.html");
    }

    #[test]
    fn parses_harness_json() {
        let dump = r#"noise
{"status":"OK","tests":[{"name":"t1","status":"PASS"},{"name":"t2","status":"FAIL"}]}
"#;
        let summary = parse_testharness_json(dump).unwrap();
        assert_eq!(summary.status, "OK");
        assert_eq!(summary.tests.len(), 2);
    }

    #[test]
    fn parses_acid3_score() {
        assert_eq!(parse_acid3_score("Acid3 test\n05/100\n"), Some(5));
        assert_eq!(parse_acid3_score("no score"), None);
    }
}
