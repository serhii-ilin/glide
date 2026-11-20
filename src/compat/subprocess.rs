//! Classify headless `glide` subprocess outcomes for the compatibility runner.

/// Outcome bucket for super-critical triage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunFailureKind {
    Crash,
    Panic,
    Hang,
}

/// First `Uncaught …` JavaScript error line from headless `glide` stderr, if any.
pub fn first_uncaught_js_error(stderr: &str) -> Option<String> {
    for line in stderr.lines() {
        let trimmed = line.trim();
        let Some(idx) = trimmed.find("Uncaught ") else {
            continue;
        };
        let msg = trimmed[idx + "Uncaught ".len()..].trim();
        if !msg.is_empty() {
            return Some(msg.to_string());
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
}
