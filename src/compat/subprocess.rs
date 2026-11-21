//! Classify headless `glide` subprocess outcomes for the compatibility runner.

/// Outcome bucket for super-critical triage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunFailureKind {
    Crash,
    Panic,
    Hang,
}

/// First `Uncaught …` JavaScript error line from headless `glide` stderr, if any.
///
/// Glide emits structured `msg="Uncaught …"` records now; the raw V8 format is still
/// accepted so older fixtures and embedded V8 diagnostics keep working.
pub fn first_uncaught_js_error(stderr: &str) -> Option<String> {
    for line in stderr.lines() {
        let message = structured_message(line).unwrap_or_else(|| line.trim().to_string());
        let Some(idx) = message.find("Uncaught ") else {
            continue;
        };
        let msg = message[idx + "Uncaught ".len()..].trim();
        if !msg.is_empty() {
            return Some(msg.to_string());
        }
    }
    None
}

fn structured_message(line: &str) -> Option<String> {
    const MARKER: &str = "msg=\"";
    let start = line.find(MARKER)? + MARKER.len();
    let mut out = String::new();
    let mut chars = line[start..].chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(out),
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(code) = u32::from_str_radix(&hex, 16)
                        && let Some(decoded) = char::from_u32(code)
                    {
                        out.push(decoded);
                    }
                }
                Some(other) => out.push(other),
                None => break,
            },
            ch => out.push(ch),
        }
    }
    None
}

/// Classify a completed or timed-out subprocess run.
pub fn classify_run(
    timed_out: bool,
    exit_code: Option<i32>,
    stderr: &str,
) -> Option<RunFailureKind> {
    if timed_out {
        return Some(RunFailureKind::Hang);
    }
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("panicked at") || (lower.contains("thread '") && lower.contains("panic")) {
        return Some(RunFailureKind::Panic);
    }
    if let Some(code) = exit_code
        && code != 0
    {
        if lower.contains("panic") {
            return Some(RunFailureKind::Panic);
        }
        return Some(RunFailureKind::Crash);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_is_hang() {
        assert_eq!(classify_run(true, None, ""), Some(RunFailureKind::Hang));
    }

    #[test]
    fn nonzero_exit_is_crash() {
        assert_eq!(
            classify_run(false, Some(1), "screenshot failed"),
            Some(RunFailureKind::Crash)
        );
    }

    #[test]
    fn panic_in_stderr() {
        let err = "thread 'main' panicked at 'boom', src/main.rs:1:1";
        assert_eq!(
            classify_run(false, Some(101), err),
            Some(RunFailureKind::Panic)
        );
    }

    #[test]
    fn success_is_none() {
        assert_eq!(classify_run(false, Some(0), ""), None);
    }

    #[test]
    fn first_uncaught_from_glide_stderr() {
        let err = "script run failed in script-0\n\n<unknown>:1: Uncaught ReferenceError: self is not defined\n";
        assert_eq!(
            first_uncaught_js_error(err),
            Some("ReferenceError: self is not defined".into())
        );
    }

    #[test]
    fn first_uncaught_from_structured_log() {
        let err = concat!(
            "level=ERROR target=\"js.uncaught\" ",
            "msg=\"Uncaught TypeError: atob is not defined\" ",
            "page_url=\"file:///page.html\" error_kind=\"missing_api\"\n"
        );
        assert_eq!(
            first_uncaught_js_error(err),
            Some("TypeError: atob is not defined".into())
        );
    }
}
