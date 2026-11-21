//! Structured terminal logging shared by the UI, loader, and JS worker threads.
//!
//! Every line is a sequence of `key=value` fields with quoted values, so a failure can be
//! correlated by page, script, and root-cause hint without parsing prose. `GLIDE_LOG`
//! selects the minimum level: `off`, `error`, `warn`, `info` (default), or `debug`.

use std::env;
use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Off,
    Error,
    Warn,
    Info,
    Debug,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
        }
    }

    pub fn from_filter(filter: &str) -> Self {
        match filter.trim().to_ascii_lowercase().as_str() {
            "off" | "silent" | "none" => Self::Off,
            "error" => Self::Error,
            "warn" | "warning" => Self::Warn,
            "debug" | "trace" => Self::Debug,
            _ => Self::Info,
        }
    }

    fn enabled_with(self, minimum: Self) -> bool {
        self != Self::Off && minimum != Self::Off && self <= minimum
    }
}

static MIN_LEVEL: OnceLock<Level> = OnceLock::new();

fn minimum_level() -> Level {
    *MIN_LEVEL.get_or_init(|| Level::from_filter(&env::var("GLIDE_LOG").unwrap_or_default()))
}

pub fn enabled(level: Level) -> bool {
    level.enabled_with(minimum_level())
}

/// Format a log record for tests and for [`emit`].
pub fn format_record(
    level: Level,
    target: &str,
    message: &str,
    fields: &[(&str, String)],
    minimum: Level,
) -> Option<String> {
    if !level.enabled_with(minimum) {
        return None;
    }
    let mut line = String::new();
    let _ = write!(
        line,
        "level={} target={} msg={}",
        level.as_str(),
        quote(target),
        quote(message)
    );
    for (key, value) in fields {
        let _ = write!(line, " {}={}", sanitize_key(key), quote(value));
    }
    Some(line)
}

pub fn emit(level: Level, target: &str, message: impl AsRef<str>, fields: &[(&str, String)]) {
    let Some(line) = format_record(level, target, message.as_ref(), fields, minimum_level()) else {
        return;
    };
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{line}");
}

pub fn error(target: &str, message: impl AsRef<str>, fields: &[(&str, String)]) {
    emit(Level::Error, target, message, fields);
}

pub fn warn(target: &str, message: impl AsRef<str>, fields: &[(&str, String)]) {
    emit(Level::Warn, target, message, fields);
}

pub fn info(target: &str, message: impl AsRef<str>, fields: &[(&str, String)]) {
    emit(Level::Info, target, message, fields);
}

pub fn debug(target: &str, message: impl AsRef<str>, fields: &[(&str, String)]) {
    emit(Level::Debug, target, message, fields);
}

fn sanitize_key(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for ch in key.chars() {
        let keep = ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.');
        out.push(if keep { ch } else { '_' });
    }
    if out.is_empty() {
        out.push_str("field");
    }
    out
}

fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_are_structured_key_value_lines() {
        let line = format_record(
            Level::Error,
            "js.uncaught",
            "Uncaught ReferenceError: atob is not defined",
            &[
                ("page_url", "file:///tmp/page.html".into()),
                ("error_kind", "missing_api".into()),
            ],
            Level::Info,
        )
        .expect("error record");
        assert_eq!(
            line,
            concat!(
                "level=ERROR target=\"js.uncaught\" ",
                "msg=\"Uncaught ReferenceError: atob is not defined\" ",
                "page_url=\"file:///tmp/page.html\" error_kind=\"missing_api\""
            )
        );
    }

    #[test]
    fn filter_suppresses_info_but_keeps_warnings_and_errors() {
        assert!(format_record(Level::Info, "t", "i", &[], Level::Warn).is_none());
        assert!(format_record(Level::Warn, "t", "w", &[], Level::Warn).is_some());
        assert!(format_record(Level::Error, "t", "e", &[], Level::Warn).is_some());
    }

    #[test]
    fn off_filter_suppresses_every_level() {
        for level in [Level::Error, Level::Warn, Level::Info, Level::Debug] {
            assert!(format_record(level, "t", "m", &[], Level::Off).is_none());
        }
    }

    #[test]
    fn unknown_filter_defaults_to_info() {
        assert_eq!(Level::from_filter("nonsense"), Level::Info);
        assert_eq!(Level::from_filter(" WARN "), Level::Warn);
        assert_eq!(Level::from_filter("trace"), Level::Debug);
    }

    #[test]
    fn values_escape_quotes_and_controls() {
        let line = format_record(
            Level::Warn,
            "js.console",
            "bad \"value\"\nnext",
            &[("odd key", "a\\b".into())],
            Level::Info,
        )
        .expect("warn record");
        assert_eq!(
            line,
            "level=WARN target=\"js.console\" msg=\"bad \\\"value\\\"\\nnext\" odd_key=\"a\\\\b\""
        );
    }
}
